#!/bin/sh
# Renderer-free Linux CPU AI test host. Run with ./run-test.sh /path/to/OMSI2
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
echo "Experimental AI and passengers simulation; type stop to exit"
exec "$here/omsi-server" "$root" "$here/server-simulation.cfg" --experimental-simulation
