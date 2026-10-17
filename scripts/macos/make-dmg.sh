#!/bin/sh
set -eu
cd "$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)"
app="${1:-dist/macos/openOMSI.app}"
out="${2:-out/openOMSI.dmg}"
tmp=$(mktemp -d)
trap 'hdiutil detach -force "$tmp/mnt" >/dev/null 2>&1; rm -rf "$tmp"' EXIT
mkdir "$tmp/stage" "$tmp/mnt"
ditto "$app" "$tmp/stage/openOMSI.app"
ln -s /Applications "$tmp/stage/Applications"
mkdir "$tmp/stage/.background"
tiffutil -cathidpicheck assets/icons/app/dmg-background.png assets/icons/app/dmg-background@2x.png -out "$tmp/stage/.background/background.tiff" >/dev/null
for try in 1 2 3; do
  hdiutil create -volname openOMSI -srcfolder "$tmp/stage" -fs HFS+ -format UDRW -ov "$tmp/rw.dmg" && break
  [ "$try" = 3 ] && exit 1
  sleep 5
done
hdiutil attach -readwrite -noverify -noautoopen -nobrowse -mountpoint "$tmp/mnt" "$tmp/rw.dmg" >/dev/null
osascript - "$tmp/mnt" <<'APPLESCRIPT' || echo "Finder did not lay the window out: the disk image opens with Finder's default view"
on run argv
  with timeout of 30 seconds
  tell application "Finder"
    set d to (POSIX file (item 1 of argv)) as alias
    open d
    set w to container window of d
    set current view of w to icon view
    set toolbar visible of w to false
    set statusbar visible of w to false
    set bounds of w to {200, 120, 740, 520}
    set o to icon view options of w
    set arrangement of o to not arranged
    set icon size of o to 128
    set text size of o to 13
    set background picture of o to file ".background:background.tiff" of d
    set position of item "openOMSI.app" of d to {140, 180}
    set position of item "Applications" of d to {400, 180}
    update d without registering applications
    delay 2
    close w
  end tell
  end timeout
end run
APPLESCRIPT
cp assets/icons/app/openomsi.icns "$tmp/mnt/.VolumeIcon.icns"
xattr -wx com.apple.FinderInfo "0000000000000000040000000000000000000000000000000000000000000000" "$tmp/mnt"
sync
hdiutil detach "$tmp/mnt" >/dev/null || { sleep 5; hdiutil detach -force "$tmp/mnt" >/dev/null; }
mkdir -p "$(dirname "$out")"
hdiutil convert "$tmp/rw.dmg" -format UDZO -imagekey zlib-level=9 -ov -o "$out" >/dev/null
