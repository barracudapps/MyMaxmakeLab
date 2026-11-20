#!/bin/bash
# Builds MyMaxmakeLab.app from one or several release binaries.
# usage: scripts/bundle-macos.sh <version> <output dir> <binary> [binary...]
# Several binaries (arm64 and x86_64) are merged into a universal one.
set -euo pipefail
version="$1"; out="$2"; shift 2
root="$(cd "$(dirname "$0")/.." && pwd)"
app="$out/MyMaxmakeLab.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
if [ "$#" -gt 1 ]; then
    lipo -create "$@" -output "$app/Contents/MacOS/mymaxmakelab"
else
    cp "$1" "$app/Contents/MacOS/mymaxmakelab"
fi
iconset="$(mktemp -d)/AppIcon.iconset"
mkdir -p "$iconset"
for size in 16 32 128 256 512; do
    sips -z $size $size "$root/packaging/icon-1024.png" --out "$iconset/icon_${size}x${size}.png" >/dev/null
    sips -z $((size * 2)) $((size * 2)) "$root/packaging/icon-1024.png" --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$app/Contents/Resources/AppIcon.icns"
sed "s/@VERSION@/$version/g" "$root/packaging/macos/Info.plist" > "$app/Contents/Info.plist"
# Ad-hoc signature: required to run on Apple silicon.
codesign --force --deep --sign - "$app"
echo "$app"
