#!/usr/bin/env bash
# Renders the installer images from their HTML sources in src-tauri/installer/:
#   dmg-background.tiff  macOS installer window, 1x and 2x (Retina) in one TIFF
#   nsis-sidebar.bmp     Windows installer, welcome and finish pages
# Run it on a Mac (tiffutil) with Google Chrome and ImageMagick installed
# (brew install imagemagick), then commit the images.
set -euo pipefail

cd "$(dirname "$0")/../src-tauri/installer"
CHROME="${CHROME:-/Applications/Google Chrome.app/Contents/MacOS/Google Chrome}"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# shot <page> <width> <height> <scale> <out.png>
shot() {
  "$CHROME" --headless=new --disable-gpu --hide-scrollbars --allow-file-access-from-files \
    --window-size="$2,$3" --force-device-scale-factor="$4" \
    --screenshot="$5" "file://$PWD/$1" >/dev/null 2>&1
  local size
  size=$(magick identify -format '%wx%h' "$5")
  if [ "$size" != "$(( $2 * $4 ))x$(( $3 * $4 ))" ]; then
    echo "$1: Chrome rendered $size instead of $2x$3 at ${4}x" >&2
    exit 1
  fi
}

shot dmg-background.html 680 448 1 "$tmp/dmg.png"
shot dmg-background.html 680 448 2 "$tmp/dmg@2x.png"
tiffutil -cathidpicheck "$tmp/dmg.png" "$tmp/dmg@2x.png" -out "$tmp/dmg.tiff" >/dev/null
tiffutil -lzw "$tmp/dmg.tiff" -out dmg-background.tiff >/dev/null

# NSIS wants a 24-bit bitmap without alpha.
shot nsis-sidebar.html 164 314 1 "$tmp/sidebar.png"
magick "$tmp/sidebar.png" -alpha off -type TrueColor BMP3:nsis-sidebar.bmp

echo "Rendered dmg-background.tiff and nsis-sidebar.bmp in src-tauri/installer/"
