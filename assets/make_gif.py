#!/usr/bin/env python3
"""
make_gif.py — Convert a video to a high-quality GIF using FFmpeg + Gifski.
Works on Windows, macOS, and Linux.

Strategy: FFmpeg transcodes the video to a .y4m intermediate (lossless raw
frames in a single file), then Gifski reads that directly. This avoids the
Windows CreateProcess command-line length limit that breaks passing thousands
of PNG paths as arguments.

Usage:
    python make_gif.py [options] <input_video>

Examples:
    python make_gif.py clip.mp4
    python make_gif.py -f 24 -w 480 -q 85 clip.mp4
    python make_gif.py -o demo.gif -f 50 clip.mov
"""

import argparse
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


# ── Helpers ───────────────────────────────────────────────────────────────────

def check_dependency(name: str) -> Path:
    path = shutil.which(name)
    if path is None:
        print(f"Error: '{name}' not found in PATH.", file=sys.stderr)
        if name == "gifski":
            print(
                "  Install options:\n"
                "    cargo install gifski\n"
                "    brew install gifski          (macOS)\n"
                "    winget install gifski        (Windows)\n"
                "    https://gif.ski",
                file=sys.stderr,
            )
        sys.exit(1)
    return Path(path)


def run(cmd: list, description: str) -> None:
    """Run a subprocess, streaming stderr live so progress is visible."""
    try:
        proc = subprocess.Popen(
            cmd,
            stderr=subprocess.PIPE,
            text=True,
            encoding="utf-8",
            errors="replace",
        )
        for line in proc.stderr:
            line = line.rstrip()
            # Suppress routine FFmpeg banner/build lines, keep warnings/errors
            if any(line.startswith(p) for p in ("ffmpeg version", "built with", "configuration:", "lib")):
                continue
            if line:
                print(f"      {line}")
        proc.wait()
        if proc.returncode != 0:
            print(f"Error: {description} failed (exit {proc.returncode}).", file=sys.stderr)
            sys.exit(proc.returncode)
    except FileNotFoundError as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)


def human_size(path: Path) -> str:
    size = path.stat().st_size
    for unit in ("B", "KB", "MB", "GB"):
        if size < 1024:
            return f"{size:.1f} {unit}"
        size /= 1024
    return f"{size:.1f} TB"


# ── Steps ─────────────────────────────────────────────────────────────────────

def transcode_to_y4m(ffmpeg: Path, input_file: Path, y4m: Path, fps: int, width: int) -> None:
    """Transcode video to raw YUV4MPEG2 (.y4m) at the target fps and width.

    y4m is a container of raw uncompressed frames — gifski reads it natively,
    so we skip per-frame PNG extraction entirely. The file can be large (~1 GB
    for a minute of 640px video) but lives in a temp dir and is deleted after.
    """
    print("[1/2] Transcoding to y4m intermediate...")
    run(
        [
            str(ffmpeg), "-loglevel", "warning", "-y",
            "-i", str(input_file),
            "-vf", f"fps={fps},scale={width}:-1:flags=lanczos",
            # y4m requires a pixel format gifski can consume; yuv420p is safe
            "-pix_fmt", "yuv420p",
            str(y4m),
        ],
        "y4m transcode",
    )
    print(f"      Intermediate: {y4m.name}  ({human_size(y4m)})")


def encode_gif(gifski_bin: Path, y4m: Path, output: Path, fps: int, width: int, quality: int) -> None:
    """Encode the y4m intermediate into a GIF with Gifski."""
    print("[2/2] Encoding GIF with Gifski...")
    run(
        [
            str(gifski_bin),
            "-r",          str(fps),
            "-Q",          str(quality),
            "-W",          str(width),
            "--output",    str(output),
            str(y4m),
        ],
        "GIF encoding",
    )


# ── CLI ───────────────────────────────────────────────────────────────────────

def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        prog="make_gif.py",
        description="Convert a video to a high-quality GIF using FFmpeg + Gifski.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=(
            "Examples:\n"
            "  python make_gif.py clip.mp4\n"
            "  python make_gif.py -f 24 -w 480 -q 85 clip.mp4\n"
            "  python make_gif.py -o demo.gif -f 50 clip.mov"
        ),
    )
    parser.add_argument("input",  metavar="<input_video>", help="Input video file")
    parser.add_argument("-o", metavar="<file>",   dest="output",  default=None, help="Output GIF filename (default: <input>.gif)")
    parser.add_argument("-f", metavar="<fps>",    dest="fps",     default=30,   type=int, help="Frame rate (default: 30)")
    parser.add_argument("-w", metavar="<px>",     dest="width",   default=640,  type=int, help="Output width in pixels (default: 640)")
    parser.add_argument("-q", metavar="<1-100>",  dest="quality", default=90,   type=int, help="Gifski quality (default: 90)")
    return parser.parse_args()


# ── Main ──────────────────────────────────────────────────────────────────────

def main() -> None:
    args = parse_args()

    input_file = Path(args.input)
    if not input_file.exists():
        print(f"Error: File not found: {input_file}", file=sys.stderr)
        sys.exit(1)

    ffmpeg = check_dependency("ffmpeg")
    gifski = check_dependency("gifski")

    output = Path(args.output) if args.output else input_file.with_suffix(".gif")

    print(f"==> Input:    {input_file}")
    print(f"==> Output:   {output}")
    print(f"==> FPS:      {args.fps}")
    print(f"==> Width:    {args.width}px")
    print(f"==> Quality:  {args.quality}")
    print()

    # Temp dir next to the input file so we stay on the same filesystem
    work_parent = input_file.resolve().parent
    tmp_dir = Path(tempfile.mkdtemp(prefix=f"{input_file.stem}_tmp_", dir=work_parent))
    y4m = tmp_dir / f"{input_file.stem}.y4m"

    try:
        transcode_to_y4m(ffmpeg, input_file, y4m, args.fps, args.width)
        encode_gif(gifski, y4m, output, args.fps, args.width, args.quality)
    finally:
        shutil.rmtree(tmp_dir, ignore_errors=True)

    if output.exists():
        print(f"\n==> Done!  {output}  ({human_size(output)})")
    else:
        print("Error: Output GIF was not created.", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()