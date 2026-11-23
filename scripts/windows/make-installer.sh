#!/bin/sh
set -eu
cd "$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)"
dist="${1:-dist/windows}"
out="${2:-out/openBus-setup.exe}"
version="${OPENBUS_VERSION:-$(sh scripts/version.sh 2>/dev/null || echo 0.0.0)}"
mkdir -p "$(dirname "$out")"
list="${out%.exe}-uninstall.nsh"
for f in "$dist"/*; do
  [ -f "$f" ] && printf 'Delete "$INSTDIR\\%s"\n' "${f##*/}"
done > "$list"
include="$(cd "$(dirname "$list")" && pwd)/$(basename "$list")"
command -v cygpath >/dev/null && include=$(cygpath -w "$include")
makensis -V2 -NOCD -DDIST="$dist" -DOUT="$out" -DVERSION="$version" -DUNINSTALL_LIST="$include" scripts/windows/installer.nsi
rm -f "$list"
