# Generates the Shiori application icon: the kanji 栞 (shiori, "bookmark")
# in ink on warm paper, with a vermillion bookmark ribbon.
#
# Outputs (all committed):
#   shiori-{16,32,48,64,128,256}.png
#   shiori.ico        — multi-size, PNG-compressed entries
#   shiori-64.rgba    — raw RGBA bytes for the runtime window icon
#
# Requires Pillow and the Windows Yu Mincho Demibold font (yumindb.ttf).

from PIL import Image, ImageDraw, ImageFont

SS = 4  # supersampling factor
BASE = 256
S = BASE * SS

PAPER = (246, 234, 210, 255)
PAPER_EDGE = (224, 205, 170, 255)
INK = (43, 33, 24, 255)
RIBBON = (200, 62, 38, 255)
RIBBON_DARK = (164, 48, 28, 255)

FONT_PATH = "C:/Windows/Fonts/yumindb.ttf"  # Yu Mincho Demibold
GLYPH = "栞"  # 栞


def draw_base() -> Image.Image:
    img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)

    # Paper: rounded square with a subtle darker edge.
    radius = int(S * 0.22)
    d.rounded_rectangle((0, 0, S - 1, S - 1), radius=radius, fill=PAPER)
    d.rounded_rectangle(
        (0, 0, S - 1, S - 1),
        radius=radius,
        outline=PAPER_EDGE,
        width=int(S * 0.012),
    )

    # Bookmark ribbon down the right side, with the classic notch.
    rib_w = int(S * 0.16)
    rib_x0 = int(S * 0.70)
    rib_x1 = rib_x0 + rib_w
    rib_top = 0
    rib_bot = int(S * 0.86)
    notch = int(S * 0.07)
    d.polygon(
        [
            (rib_x0, rib_top),
            (rib_x1, rib_top),
            (rib_x1, rib_bot),
            ((rib_x0 + rib_x1) // 2, rib_bot - notch),
            (rib_x0, rib_bot),
        ],
        fill=RIBBON,
    )
    # Thin darker stripe on the ribbon's left edge for depth.
    d.rectangle((rib_x0, rib_top, rib_x0 + int(S * 0.015), rib_bot - 6), fill=RIBBON_DARK)

    # The ribbon must not poke outside the rounded corner: re-mask.
    mask = Image.new("L", (S, S), 0)
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, S - 1, S - 1), radius=radius, fill=255)
    img.putalpha(Image.composite(img.getchannel("A"), Image.new("L", (S, S), 0), mask))

    # 栞 centered slightly left, drawn over everything.
    font = ImageFont.truetype(FONT_PATH, int(S * 0.62))
    bbox = d.textbbox((0, 0), GLYPH, font=font)
    gw, gh = bbox[2] - bbox[0], bbox[3] - bbox[1]
    x = int(S * 0.46) - gw // 2 - bbox[0]
    y = int(S * 0.52) - gh // 2 - bbox[1]
    d.text((x, y), GLYPH, font=font, fill=INK)
    return img


def main() -> None:
    big = draw_base()
    sizes = [256, 128, 64, 48, 32, 16]
    pngs = {}
    for size in sizes:
        img = big.resize((size, size), Image.LANCZOS)
        img.save(f"shiori-{size}.png")
        pngs[size] = img

    # Multi-size ICO (Pillow embeds the listed sizes from the 256px master).
    pngs[256].save("shiori.ico", sizes=[(s, s) for s in sizes])

    # Raw RGBA for eframe's window icon.
    with open("shiori-64.rgba", "wb") as f:
        f.write(pngs[64].tobytes())

    print("generated", ", ".join(f"shiori-{s}.png" for s in sizes), "shiori.ico shiori-64.rgba")


if __name__ == "__main__":
    main()
