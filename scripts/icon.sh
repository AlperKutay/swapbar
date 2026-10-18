#!/usr/bin/env bash
# Regenerate assets/AppIcon.icns from assets/AppIcon.svg (needs rsvg-convert: brew install librsvg).
# Optional: pngquant and oxipng (brew install pngquant oxipng) for a smaller .icns.
set -euo pipefail
cd "$(dirname "$0")/.."
SET="$(mktemp -d)/AppIcon.iconset"
mkdir -p "$SET"
for s in 16 32 128 256 512; do
  rsvg-convert -w "$s" -h "$s" assets/AppIcon.svg -o "$SET/icon_${s}x${s}.png"
  rsvg-convert -w $((s * 2)) -h $((s * 2)) assets/AppIcon.svg -o "$SET/icon_${s}x${s}@2x.png"
done

# The icon is flat, so a small palette is visually lossless and shrinks the PNGs a lot.
if command -v pngquant >/dev/null; then
  pngquant --force --ext .png --quality 90-100 --speed 1 --strip "$SET"/*.png
fi
if command -v oxipng >/dev/null; then
  oxipng --quiet -o max --strip safe "$SET"/*.png
fi

# Pack the PNGs into the .icns as-is. iconutil re-encodes them, which undoes the optimization
# above (~37 KB of PNGs became a 129 KB .icns).
python3 - "$SET" assets/AppIcon.icns <<'PY'
import struct, sys, pathlib
src, out = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
types = [
    ("icp4", "icon_16x16.png"), ("ic11", "icon_16x16@2x.png"),
    ("icp5", "icon_32x32.png"), ("ic12", "icon_32x32@2x.png"),
    ("ic07", "icon_128x128.png"), ("ic13", "icon_128x128@2x.png"),
    ("ic08", "icon_256x256.png"), ("ic14", "icon_256x256@2x.png"),
    ("ic09", "icon_512x512.png"), ("ic10", "icon_512x512@2x.png"),
]
body = b"".join(
    t.encode() + struct.pack(">I", 8 + len(d)) + d
    for t, d in ((t, (src / f).read_bytes()) for t, f in types)
)
out.write_bytes(b"icns" + struct.pack(">I", 8 + len(body)) + body)
PY
echo "wrote assets/AppIcon.icns ($(stat -f %z assets/AppIcon.icns) bytes)"
