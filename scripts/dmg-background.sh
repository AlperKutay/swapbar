#!/usr/bin/env bash
# Regenerate assets/dmg/background.tiff (1x + @2x) from assets/dmg/background.svg.
# Needs rsvg-convert (brew install librsvg). The SVG uses SF Pro Text and New York, so render on a Mac.
set -euo pipefail
cd "$(dirname "$0")/.."
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
rsvg-convert -w 660 -h 420 assets/dmg/background.svg -o "$TMP/background.png"
rsvg-convert -w 1320 -h 840 assets/dmg/background.svg -o "$TMP/background@2x.png"
tiffutil -cathidpicheck "$TMP/background.png" "$TMP/background@2x.png" -out assets/dmg/background.tiff
echo "wrote assets/dmg/background.tiff"
