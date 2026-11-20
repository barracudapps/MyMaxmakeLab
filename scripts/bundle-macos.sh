#!/bin/bash
# Builds MyMaxmakeLab.app from one or several cargo release folders,
# each holding mymaxmakelab and mymaxmakelab-mcp.
# usage: scripts/bundle-macos.sh <version> <output dir> <release dir> [release dir...]
# Several folders (arm64 and x86_64) are merged into universal binaries.
set -euo pipefail
version="$1"; out="$2"; shift 2
root="$(cd "$(dirname "$0")/.." && pwd)"
app="$out/MyMaxmakeLab.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
for binary in mymaxmakelab mymaxmakelab-mcp; do
    inputs=()
    for dir in "$@"; do inputs+=("$dir/$binary"); done
    if [ "${#inputs[@]}" -gt 1 ]; then
        lipo -create "${inputs[@]}" -output "$app/Contents/MacOS/$binary"
    else
        cp "${inputs[0]}" "$app/Contents/MacOS/$binary"
    fi
done
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
