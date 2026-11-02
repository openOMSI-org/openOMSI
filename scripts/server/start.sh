#!/bin/sh
# Start the openOMSI dedicated server: start.sh /path/to/OMSI2 [server.cfg]
# (the OMSI 2 folder: a complete original installation, as the players have it; mods
# go into this folder's Vehicles, maps, Sceneryobjects … as in the game's content folder)
set -eu
here="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
root="${1:?usage: start.sh /path/to/OMSI2 [server.cfg]}"
cfg="${2:-$here/server.cfg}"
log="$here/server.log"

# server.log: begun anew at every start and past log_max_mb (server.cfg), the earlier ones
# kept as server.1.log (the newest) .. server.<log_keep>.log (see run-server.sh)
cfg_get() { [ -f "$cfg" ] || return 0; awk -F= -v k="$1" '{ key = $1; gsub(/^[ \t]+|[ \t]+$/, "", key) } key == k && $0 !~ /^[ \t]*#/ { v = substr($0, index($0, "=") + 1); gsub(/^[ \t]+|[ \t]+$/, "", v); last = v } END { print last }' "$cfg"; }
log_number() { v="$(cfg_get "$1")"; case "$v" in ''|*[!0-9]*) echo "$2" ;; *) echo "$v" ;; esac; }
rotate_log() {
  base="${log%.log}"
  k=$((keep + 1))
  while [ -f "$base.$k.log" ]; do rm -f "$base.$k.log"; k=$((k + 1)); done
  [ -f "$log" ] || return 0
  if [ "$keep" -le 0 ]; then rm -f "$log"; return 0; fi
  k=$keep
  while [ "$k" -gt 1 ]; do
    if [ -f "$base.$((k - 1)).log" ]; then mv -f "$base.$((k - 1)).log" "$base.$k.log"; fi
    k=$((k - 1))
  done
  mv -f "$log" "$base.1.log"
}
log_lines() {
  trap '' INT
  size=0
  while IFS= read -r line || [ -n "$line" ]; do
    printf '%s\n' "$line"
    printf '%s\n' "$line" >> "$log"
    size=$((size + ${#line} + 1))
    if [ "$max_bytes" -gt 0 ] && [ "$size" -gt "$max_bytes" ]; then rotate_log; size=0; fi
  done
}
keep="$(log_number log_keep 5)"
[ "$keep" -le 1000 ] || keep=1000
max_bytes=$(( $(log_number log_max_mb 20) * 1048576 ))
rotate_log
# (Ctrl+C is the server's: it stops cleanly and its last lines still reach the log)
trap ':' INT
"$here/openomsi" --root "$root" --server "$cfg" 2>&1 | log_lines
