#!/bin/sh
# Renderer-free Linux test host. Run with ./run-test.sh /path/to/OMSI2
set -eu
here="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
root="${1:-}"
if [ -z "$root" ]; then
    printf "Full OMSI 2 installation directory: "
    IFS= read -r root
fi
if [ ! -d "$root/Maps" ] && [ ! -d "$root/maps" ]; then
    echo "OMSI 2 Maps folder not found at: $root" >&2
    exit 1
fi
echo "Experimental LAN relay only; stop using the console command: stop"
exec "$here/omsi-server" "$root" "$here/server-test.cfg" --experimental-free-roam
