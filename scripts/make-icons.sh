#!/bin/sh
# Render the application icons from assets/icons/app/*.svg: openomsi.icns (macOS),
# openomsi.ico (Windows) and the PNG window icons. Needs resvg (cargo install resvg),
# iconutil (macOS) and Python 3 with Pillow and fontTools. Also the pictures of the Windows
# installer and of the macOS disk image (in the launcher's Inter).
set -eu
cd "$(CDPATH= cd -- "$(dirname -- "$0")/../assets/icons/app" && pwd)"
tmp=$(mktemp -d)
mkdir "$tmp/icon.iconset"
for n in 16 32 128 256 512; do
  resvg -w "$n" openomsi-macos.svg "$tmp/icon.iconset/icon_${n}x${n}.png"
  resvg -w $((n * 2)) openomsi-macos.svg "$tmp/icon.iconset/icon_${n}x${n}@2x.png"
done
iconutil -c icns "$tmp/icon.iconset" -o openomsi.icns
for n in 16 24 32 48 64 128 256; do resvg -w "$n" openomsi.svg "$tmp/w$n.png"; done
cp "$tmp/w256.png" openomsi-256.png
resvg -w 512 openomsi.svg openomsi-512.png
resvg -w 328 installer-sidebar.svg "$tmp/sidebar.png"
resvg -w 300 installer-header.svg "$tmp/header.png"
python3 -c 'import sys; from fontTools.ttLib import TTFont; f = TTFont(sys.argv[1]); f.flavor = None; f.save(sys.argv[2])' ../../../launcher/ui/fonts/InterVariable.woff2 "$tmp/Inter.ttf"
resvg --use-font-file "$tmp/Inter.ttf" -w 540 dmg-background.svg dmg-background.png
resvg --use-font-file "$tmp/Inter.ttf" -w 1080 dmg-background.svg dmg-background@2x.png
python3 - "$tmp" <<'PY'
import sys
from PIL import Image
t = sys.argv[1]
ims = [Image.open(f"{t}/w{n}.png") for n in (256, 128, 64, 48, 32, 24, 16)]
ims[0].save("openomsi.ico", sizes=[i.size for i in ims], append_images=ims[1:])
for n in ("sidebar", "header"):
    Image.open(f"{t}/{n}.png").convert("RGB").quantize(256).save(f"installer-{n}.bmp")
PY
rm -rf "$tmp"
echo "icons written to assets/icons/app"
