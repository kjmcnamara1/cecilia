#!/bin/sh
# Renders an SVG into the ten PNGs of a macOS .iconset (iconutil turns it into an .icns).
#   make-iconset.sh SVG OUT.iconset
set -eu
[ $# -eq 2 ] || { echo "usage: $0 SVG OUT.iconset" >&2; exit 2; }
svg=$1
out=$2
mkdir -p "$out"
for size in 16 32 128 256 512; do
    rsvg-convert -w "$size" -h "$size" -o "$out/icon_${size}x${size}.png" "$svg"
    rsvg-convert -w $((size * 2)) -h $((size * 2)) -o "$out/icon_${size}x${size}@2x.png" "$svg"
done
