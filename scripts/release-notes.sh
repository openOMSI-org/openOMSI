#!/bin/sh
set -eu
cd "$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
version=$1
since=${2:-}

newer() { [ "$1" != "$2" ] && [ "$(printf '%s\n%s\n' "$1" "$2" | sort -V | tail -1)" = "$1" ]; }

added_since() {
  git diff --unified=0 "$1" "$2" -- CHANGELOG.md | grep '^+' | grep -v '^+++' | sed 's/^+//' | grep -v '^## ' || true
}

if [ -z "$since" ]; then
  changes=$(awk -v v="## $version" 'index($0, v) == 1 && (length($0) == length(v) || substr($0, length(v) + 1, 1) == " ") {f = 1; next} f && /^## / {exit} f {print}' CHANGELOG.md)
  if ! printf '%s' "$changes" | grep -q '[^[:space:]]'; then
    prev=$(git tag -l 'v*' --sort=-v:refname | grep -vx "v$version" | head -1)
    [ -n "$prev" ] && changes=$(added_since "$prev" HEAD)
  fi
else
  start= end=
  for entry in $(grep -n '^## [0-9]' CHANGELOG.md | sed 's/^\([0-9]*\):## \([0-9.]*\).*/\1:\2/'); do
    n=${entry%%:*} v=${entry#*:}
    if [ -z "$start" ] && ! newer "$v" "$version"; then start=$n; fi
    if [ -n "$start" ] && ! newer "$v" "$since"; then end=$n; break; fi
  done
  changes=
  if [ -n "$start" ] && [ "$start" != "$end" ]; then
    changes=$(if [ -n "$end" ]; then sed -n "${start},$((end - 1))p" CHANGELOG.md; else sed -n "${start},\$p" CHANGELOG.md; fi)
  fi
  if ! printf '%s' "$changes" | grep -q '[^[:space:]]' && git rev-parse -q --verify "v$since" >/dev/null && git rev-parse -q --verify "v$version" >/dev/null; then
    changes=$(added_since "v$since" "v$version")
  fi
fi
printf '%s' "$changes" | grep -q '[^[:space:]]' || changes="Small changes and fixes."

echo "**openBus $version**"
echo
if [ -z "$since" ]; then
  echo "> **Beta, for testing.** The launcher keeps players on the stable release; this build is for anyone who wants to try what comes next. Report what breaks in an issue, with the version. openBus needs an original OMSI 2 installation to run; it contains no game content."
else
  echo "> **Early release, expect bugs.** openBus needs an original OMSI 2 installation to run; it contains no game content."
  echo
  echo "Changes since $since:"
fi
echo
printf '%s\n' "$changes"
cat <<NOTES

### Downloads

| Platform | Download |
| --- | --- |
| Windows x64 | \`openBus-$version-windows-x64-setup.exe\` (installer) or \`openBus-$version-windows-x64.zip\` |
| Windows ARM64 | \`openBus-$version-windows-arm64-setup.exe\` (installer) or \`openBus-$version-windows-arm64.zip\` |
| macOS (Apple silicon) | \`openBus-$version-macos-arm64.dmg\` (disk image) or \`openBus-$version-macos-arm64.zip\` |
| macOS (Intel) | \`openBus-$version-macos-x64.dmg\` (disk image) or \`openBus-$version-macos-x64.zip\` |
| Linux x64 | \`openBus-$version-linux-x64.zip\` |
| Linux ARM64 | \`openBus-$version-linux-arm64.zip\` |
| Android (arm64, 8.0+) | \`openBus-$version-android-arm64.apk\` |
| Dedicated server, Linux | \`openBus-$version-server-linux-x64.zip\`, \`openBus-$version-server-linux-arm64.zip\` |
| Dedicated server, Windows | \`openBus-$version-server-windows-x64.zip\`, \`openBus-$version-server-windows-arm64.zip\` |

The SHA-256 of every file is in \`SHA256SUMS\`: \`sha256sum -c SHA256SUMS --ignore-missing\` checks the ones downloaded.
NOTES
