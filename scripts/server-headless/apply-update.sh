#!/bin/sh
# Offline update of the native server binary, only while it is STOPPED.
# Never extract the package's server.cfg, maps or any other files into place.
set -eu
here="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
stage="$here/.headless-updates"
ready="$stage/ready.txt"
[ -r "$ready" ] || { echo "No verified update staged; run /update download first." >&2; exit 1; }
archive_name="$(sed -n '1p' "$ready")"
expected="$(sed -n '2p' "$ready")"
version="$(sed -n '3p' "$ready")"
case "$archive_name" in
    *[!a-zA-Z0-9._-]*|'') echo "Invalid staged filename" >&2; exit 1 ;;
esac
case "$version" in
    *[!0-9.]*|'') echo "Invalid version" >&2; exit 1 ;;
esac
case "$expected" in
    *[!a-f0-9]*|'') echo "Invalid SHA-256" >&2; exit 1 ;;
esac
[ "${#expected}" -eq 64 ] || { echo "Invalid SHA-256 length" >&2; exit 1; }
case "$archive_name" in
    "openOMSI-$version-headless-test-linux-x64.zip"|"openOMSI-$version-headless-test-linux-arm64.zip") ;;
    *) echo "Unexpected headless update package" >&2; exit 1 ;;
esac
archive="$stage/$archive_name"
[ -f "$archive" ] || { echo "Staged archive is missing" >&2; exit 1; }
[ -x "$here/omsi-server" ] || { echo "Server binary is missing" >&2; exit 1; }
command -v unzip >/dev/null 2>&1 || { echo "unzip is required" >&2; exit 1; }
command -v sha256sum >/dev/null 2>&1 || { echo "sha256sum is required" >&2; exit 1; }
actual="$(sha256sum "$archive" | cut -d' ' -f1)"
[ "$actual" = "$expected" ] || { echo "Update SHA-256 mismatch; refusing installation" >&2; exit 1; }
# Linux permits renaming a running executable; refuse this explicitly,
# rather than silently installing a new binary while the old server serves.
if [ -d /proc ]; then
    for process in /proc/[0-9]*/exe; do
        running="$(readlink "$process" 2>/dev/null || true)"
        if [ "$running" = "$here/omsi-server" ]; then
            echo "Server is still running; type stop before applying the update." >&2
            exit 1
        fi
    done
fi
candidate="$stage/omsi-server.next"
backup="$here/omsi-server.previous"
[ ! -e "$backup" ] || { echo "Backup $backup already exists; archive it before another update" >&2; exit 1; }
unzip -p "$archive" omsi-server > "$candidate"
chmod +x "$candidate"
new_version="$("$candidate" --version)"
[ "$new_version" = "$version" ] || { rm -f "$candidate"; echo "Package version mismatch" >&2; exit 1; }
"$candidate" --self-test || { rm -f "$candidate"; echo "New binary failed self-test" >&2; exit 1; }
mv "$here/omsi-server" "$backup"
if ! mv "$candidate" "$here/omsi-server"; then
    mv "$backup" "$here/omsi-server"
    echo "Could not install new binary; restored original" >&2
    exit 1
fi
if ! "$here/omsi-server" --self-test; then
    mv "$here/omsi-server" "$candidate"
    mv "$backup" "$here/omsi-server"
    echo "New binary failed after installation; original restored" >&2
    exit 1
fi
rm -f "$ready"
echo "Updated headless executable to $version. Backup retained at $backup."
echo "Start the server using your original launch command; server.cfg and content were untouched."
