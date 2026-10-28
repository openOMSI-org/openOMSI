#!/bin/sh
set -eu
cd "$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)"
app="${1:-dist/macos/openOMSI.app}"
out="${2:-out/openOMSI.dmg}"
tmp=$(mktemp -d)
trap 'hdiutil detach -force "${device:-$tmp/mnt}" >/dev/null 2>&1 || true; rm -rf "$tmp"' EXIT
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
hdiutil attach -readwrite -noverify -noautoopen -nobrowse -mountpoint "$tmp/mnt" "$tmp/rw.dmg" > "$tmp/attached"
# Keep the device name: a failed eject may remove the mount point before detaching the image.
device=$(awk '/^\/dev\/disk/ { print $1; exit }' "$tmp/attached")
[ -n "$device" ] || { echo "No disk device returned by hdiutil attach" >&2; exit 1; }
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
image_still_attached() {
  hdiutil info > "$tmp/attached-now" || { echo "Could not check the attached disk images" >&2; exit 1; }
  awk -v device="$device" '$1 == device { found=1 } END { exit !found }' "$tmp/attached-now"
}
detached=0
for try in 1 2 3 4 5; do
  hdiutil detach "$device" >/dev/null 2>&1 || true
  if ! image_still_attached; then
    detached=1
    break
  fi
  [ "$try" = 5 ] || sleep 3
done
if [ "$detached" = 0 ]; then
  # If something still holds the mounted filesystem, unmount it before retrying eject.
  diskutil unmountDisk force "$device" >/dev/null 2>&1 || true
  for try in 1 2 3 4 5; do
    hdiutil detach -force "$device" >/dev/null 2>&1 || true
    if ! image_still_attached; then
      detached=1
      break
    fi
    [ "$try" = 5 ] || sleep 3
  done
fi
if [ "$detached" = 0 ]; then
  echo "Disk image $device is still attached after detach and forced unmount retries" >&2
  lsof -nP +D "$tmp/mnt" >&2 || true
  exit 1
fi
device=
mkdir -p "$(dirname "$out")"
for try in 1 2 3; do
  hdiutil convert "$tmp/rw.dmg" -format UDZO -imagekey zlib-level=9 -ov -o "$out" >/dev/null && break
  [ "$try" = 3 ] && exit 1
  sleep 5
done
