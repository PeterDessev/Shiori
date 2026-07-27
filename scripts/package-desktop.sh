#!/usr/bin/env bash
# Regenerate the shipping desktop artifact on Linux/macOS: build the release
# binary and bundle it with the licenses and docs into a .tar.gz, mirroring
# the Unix packaging in .github/workflows/release.yml.
#
# Usage: scripts/package-desktop.sh [VERSION]
#   VERSION defaults to "dev"; releases use the tag name (e.g. v0.4.0).
#
# Requires a stable Rust toolchain. The first build downloads and embeds the
# IPADIC morphological dictionary (needs network, once).
set -euo pipefail

version="${1:-dev}"
root="$(cd "$(dirname "$0")/.." && pwd)"

# The Cargo workspace lives under software/; the binary lands in
# software/target/release. Licenses and docs stay at the repo root.
( cd "$root/software" && cargo build --release -p shiori-gui )

case "$(uname -s)" in
  Darwin) name="macos-$(uname -m)" ;;
  Linux)  name="linux-$(uname -m)" ;;
  *)      name="$(uname -s)-$(uname -m)" ;;
esac
# release.yml labels Apple Silicon as macos-aarch64.
name="${name/arm64/aarch64}"

staging="shiori-${version}-${name}"
rm -rf "${root:?}/${staging}"
mkdir "$root/$staging"
cp "$root/software/target/release/shiori" \
   "$root/LICENSE-MIT" "$root/LICENSE-APACHE" \
   "$root/README.md" "$root/CHANGELOG.md" \
   "$root/$staging/"
( cd "$root" && tar czf "$staging.tar.gz" "$staging" )
rm -rf "${root:?}/${staging}"

echo "Built $root/$staging.tar.gz"
