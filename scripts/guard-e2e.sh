#!/usr/bin/env bash
# The start guard end to end (see crates/omsi-guard): real release builds in a throwaway
# installation, fake releases (the GitHub API's format, file:// addresses), versions broken on
# purpose, and the outcome read from the files - which version is installed afterwards, what is
# kept, the journal, the bad versions, the messages the player would have seen.
#
#   scripts/guard-e2e.sh <old openbus> <new openbus> [scenario ...]
#   scripts/guard-e2e.sh list
#
# <old> and <new> are release builds of the same commit with different versions
# (OPENBUS_VERSION=0.9.1 / 0.9.2 cargo build --release -p omsi-app), each with
# libsteam_api.dylib beside it on macOS. Nothing is shown on the screen (hidden launcher
# windows, the guard's messages written to a file), nothing touches the real installation or
# ~/.openbus (a fresh HOME per scenario), and no update is fetched from GitHub. Needs bash,
# python3 and zip. Exits 0 when every scenario passed. GUARD_E2E_BUNDLE=1 (macOS) lays every
# installation and release out as an openBus.app bundle, as players have it.
set -uo pipefail

SCENARIOS=(update_crashes update_hangs fixed_release_heals update_cannot_start listed_broken nothing_helps good_start_cleans_up)

usage() {
  sed -n '2,/^set -uo/p' "$0" | grep '^#' | sed 's/^# \{0,1\}//'
  exit 2
}
[ "${1:-}" = list ] && { printf '%s\n' "${SCENARIOS[@]}"; exit 0; }
[ $# -ge 2 ] || usage
OLD_BIN="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
NEW_BIN="$(cd "$(dirname "$2")" && pwd)/$(basename "$2")"
shift 2
[ $# -gt 0 ] && SCENARIOS=("$@")

version_of() { "$1" --self-test 2>/dev/null | awk '{print $2}'; }
OLD_V="$(version_of "$OLD_BIN")"
NEW_V="$(version_of "$NEW_BIN")"
[ -n "$OLD_V" ] && [ -n "$NEW_V" ] && [ "$OLD_V" != "$NEW_V" ] || { echo "two builds with different versions needed (got '$OLD_V' and '$NEW_V')"; exit 2; }
case "$(uname -s)-$(uname -m)" in
  Darwin-arm64) SUFFIX=macos-arm64.zip ;;
  Darwin-x86_64) SUFFIX=macos-x64.zip ;;
  Linux-x86_64) SUFFIX=linux-x64.zip ;;
  Linux-aarch64) SUFFIX=linux-arm64.zip ;;
  *) echo "unsupported platform"; exit 2 ;;
esac

WORK="$(cd "$(mktemp -d "${TMPDIR:-/tmp}/openbus-guard-e2e.XXXXXX")" && pwd -P)"
echo "work folder: $WORK (old $OLD_V, new $NEW_V)"
PASS=0
FAIL=0

BUNDLE="${GUARD_E2E_BUNDLE:-}"
# the program of an installation, and the top-level items a release holds
prog() { if [ -n "$BUNDLE" ]; then echo "$1/openBus.app/Contents/MacOS/openbus"; else echo "$1/openbus"; fi; }
if [ -n "$BUNDLE" ]; then ITEMS='openBus.app\n'; else ITEMS='openbus\nlibsteam_api.dylib\n'; fi

# copy a build's program files into a folder (an openBus.app in it with GUARD_E2E_BUNDLE)
put_build() { # <bin> <dir>
  local to="$2"
  if [ -n "$BUNDLE" ]; then
    to="$2/openBus.app/Contents/MacOS"; mkdir -p "$to"
    printf '<?xml version="1.0" encoding="UTF-8"?>\n<plist version="1.0"><dict><key>CFBundleExecutable</key><string>openbus</string><key>CFBundleIdentifier</key><string>io.github.openbus.e2e</string><key>CFBundlePackageType</key><string>APPL</string></dict></plist>\n' > "$2/openBus.app/Contents/Info.plist"
  fi
  mkdir -p "$to"
  cp "$1" "$to/openbus"
  local lib; lib="$(dirname "$1")/libsteam_api.dylib"
  [ -f "$lib" ] && cp "$lib" "$to/"
  return 0
}

# a release: zip + the API's JSON naming it; extra files (name=content) go into the zip too
make_release() { # <version> <bin or 'garbage'> <out dir> [name=content ...]
  local v="$1" bin="$2" out="$3"; shift 3
  local stage="$out/stage-$v"; rm -rf "$stage"; mkdir -p "$stage"
  if [ "$bin" = garbage ]; then
    put_build "$OLD_BIN" "$stage"
    printf 'not a program\n' > "$(prog "$stage")"; chmod +x "$(prog "$stage")"
  else
    put_build "$bin" "$stage"
  fi
  local kv
  for kv in "$@"; do printf '%s\n' "${kv#*=}" > "$stage/${kv%%=*}"; done
  local zip="$out/openBus-$v-$SUFFIX"
  (cd "$stage" && zip -qry "$zip" .)
  python3 -I - "$v" "$zip" "$out/release-$v.json" <<'PY'
import hashlib, json, os, sys
v, z, out = sys.argv[1:4]
data = open(z, "rb").read()
json.dump({"tag_name": "v" + v, "html_url": "file://" + z, "body": "", "draft": False, "prerelease": False,
           "assets": [{"name": os.path.basename(z), "browser_download_url": "file://" + z, "size": len(data),
                       "digest": "sha256:" + hashlib.sha256(data).hexdigest()}]}, open(out, "w"))
PY
  echo "$out/release-$v.json"
}

# an installation as an update to <new> leaves it: the old files set aside, the version noted
updated_install() { # <dir> <new bin> [name=content ...]
  local dir="$1" bin="$2"; shift 2
  put_build "$OLD_BIN" "$dir"
  printf "$ITEMS" > "$dir/.openbus-files"
  for f in openbus libsteam_api.dylib openBus.app .openbus-files; do [ -e "$dir/$f" ] && mv "$dir/$f" "$dir/$f.old-update"; done
  put_build "$bin" "$dir"
  local list="$ITEMS" kv
  for kv in "$@"; do printf '%s\n' "${kv#*=}" > "$dir/${kv%%=*}"; list="$list${kv%%=*}\n"; done
  printf "$list" > "$dir/.openbus-files"
  printf '%s' "$OLD_V" > "$dir/.openbus-previous"
}

# start the launcher of <dir> as a player would (hidden), and wait until no program of that
# installation runs any more (the guard may start it again in between)
run_launcher() { # <dir> <home> [VAR=value ...]
  local dir="$1" home="$2"; shift 2
  mkdir -p "$home/.openbus"
  env -i PATH="$PATH" HOME="$home" TMPDIR="${TMPDIR:-/tmp}" \
    OMSI_ROOT="${OMSI_ROOT:-/nonexistent}" OMSI_BACKGROUND=1 OMSI_HIDDEN_WINDOW=1 \
    OMSI_GUARD_TEST_BREAK=1 OMSI_GUARD_DIALOG_FILE="$home/messages.txt" OMSI_HEALTH_URL="file://$WORK/health-empty.json" \
    "$@" "$(prog "$dir")" > "$home/first-start.log" 2>&1 &
  local t=0
  sleep 2
  while pgrep -f "$dir/" > /dev/null 2>&1; do
    sleep 1; t=$((t + 1))
    [ $t -gt 240 ] && { echo "    (still running after 240 s - ending it)"; pkill -9 -f "$dir/"; break; }
  done
}

installed_version() { version_of "$(prog "$1")"; }

check() { # <description> <condition command...>
  local what="$1"; shift
  if "$@"; then echo "    ok   $what"; else echo "    FAIL $what"; FAILED=1; fi
}

printf '{"bad": [], "message": ""}\n' > "$WORK/health-empty.json"
REL="$WORK/releases"; mkdir -p "$REL"
# the releases the scenarios use
REL_NEW_GOOD="$(make_release "$NEW_V" "$NEW_BIN" "$REL")"
mkdir -p "$REL/crash" "$REL/hang" "$REL/garbage"
REL_NEW_CRASH="$(make_release "$NEW_V" "$NEW_BIN" "$REL/crash" guard-test-break=panic)"
REL_NEW_HANG="$(make_release "$NEW_V" "$NEW_BIN" "$REL/hang" guard-test-break=hang)"
REL_NEW_GARBAGE="$(make_release "$NEW_V" garbage "$REL/garbage")"

for s in "${SCENARIOS[@]}"; do
  echo "== $s"
  FAILED=0
  D="$WORK/$s/install"; H="$WORK/$s/home"; mkdir -p "$D" "$H/.openbus"
  G="$H/.openbus/guard"
  case "$s" in
    update_crashes)
      # the launcher of the old version updates itself (update_auto) to a new one that crashes
      # at every start: the guard puts the old one back and never installs the new one again
      put_build "$OLD_BIN" "$D"
      printf 'update_check=true\nupdate_auto=true\n' > "$H/.openbus/settings.cfg"
      run_launcher "$D" "$H" OMSI_UPDATE_URL="file://$REL_NEW_CRASH"
      check "the old version $OLD_V is installed again" test "$(installed_version "$D")" = "$OLD_V"
      check "$NEW_V is marked broken here" grep -qx "$NEW_V" "$G/bad-versions"
      check "the player was told it went back" grep -q "went back to $OLD_V" "$H/messages.txt"
      check "the crash is in the journal" grep -q '"end": "crash"' "$G/starts.json"
      check "the update's break file is set aside" test ! -e "$D/guard-test-break"
      ;;
    update_hangs)
      # as above, but the new version hangs before its first frames
      put_build "$OLD_BIN" "$D"
      printf 'update_check=true\nupdate_auto=true\n' > "$H/.openbus/settings.cfg"
      run_launcher "$D" "$H" OMSI_UPDATE_URL="file://$REL_NEW_HANG" OMSI_GUARD_DEADLINE=10
      check "the old version $OLD_V is installed again" test "$(installed_version "$D")" = "$OLD_V"
      check "the hang is in the journal" grep -q '"end": "hang"' "$G/starts.json"
      check "the player was told it went back" grep -q "went back to $OLD_V" "$H/messages.txt"
      check "no hung program is left" bash -c "! pgrep -f '$D/' > /dev/null"
      ;;
    fixed_release_heals)
      # the installed version crashes at its start, a fixed newer release is out: installed and
      # started by the guard (no earlier version kept)
      put_build "$OLD_BIN" "$D"
      printf 'panic\n' > "$D/guard-test-break"
      printf "${ITEMS}guard-test-break\n" > "$D/.openbus-files"
      run_launcher "$D" "$H" OMSI_UPDATE_URL="file://$REL_NEW_GOOD"
      check "the fixed release $NEW_V is installed" test "$(installed_version "$D")" = "$NEW_V"
      check "the player was told it updated" grep -q "updated itself to $NEW_V" "$H/messages.txt"
      check "$NEW_V started well afterwards" python3 -I -c "import json,sys; j=json.load(open('$G/starts.json')); sys.exit(0 if any(e['version']=='$NEW_V' and e['healthy'] for e in j['starts']) else 1)"
      check "$OLD_V is marked broken here" grep -qx "$OLD_V" "$G/bad-versions"
      ;;
    update_cannot_start)
      # the launcher updates itself to a release whose program cannot even run: its self-test
      # refuses it before anything is replaced
      put_build "$OLD_BIN" "$D"
      printf 'update_check=true\nupdate_auto=true\n' > "$H/.openbus/settings.cfg"
      run_launcher "$D" "$H" OMSI_UPDATE_URL="file://$REL_NEW_GARBAGE"
      check "$OLD_V is still installed" test "$(installed_version "$D")" = "$OLD_V"
      check "nothing was set aside" bash -c "! ls '$D' | grep -q old-update"
      check "$NEW_V is marked broken here" grep -qx "$NEW_V" "$G/bad-versions"
      ;;
    listed_broken)
      # the new version starts fine, but the project lists it as broken: the next start goes back
      updated_install "$D" "$NEW_BIN"
      printf '{"bad": ["%s"], "message": "test listing"}\n' "$NEW_V" > "$WORK/health-bad.json"
      run_launcher "$D" "$H" OMSI_HEALTH_URL="file://$WORK/health-bad.json" OMSI_UPDATE_URL="file://$REL_NEW_GOOD"
      check "the first start ran $NEW_V and noted the listing" grep -q '"remote_bad"' "$G/starts.json"
      check "the way back was kept" bash -c "ls -a '$D' | grep -q old-update"
      run_launcher "$D" "$H" OMSI_HEALTH_URL="file://$WORK/health-bad.json" OMSI_UPDATE_URL="file://$REL_NEW_GOOD"
      check "the next start went back to $OLD_V" test "$(installed_version "$D")" = "$OLD_V"
      check "the player was told why" grep -q "lists openBus $NEW_V as broken" "$H/messages.txt"
      check "$NEW_V is marked broken here" grep -qx "$NEW_V" "$G/bad-versions"
      ;;
    nothing_helps)
      # broken, no newer release, nothing kept: safe mode once, then the player is told - and
      # no endless restarts; the start after that just runs
      put_build "$OLD_BIN" "$D"
      printf 'exit\n' > "$D/guard-test-break"
      printf '{"tag_name": "v0.0.1", "draft": false, "prerelease": false, "assets": []}\n' > "$WORK/release-old.json"
      run_launcher "$D" "$H" OMSI_UPDATE_URL="file://$WORK/release-old.json"
      check "safe mode was tried" grep -q "safe mode" "$H/messages.txt"
      check "then the player was told what to do" grep -q "could not heal itself" "$H/messages.txt"
      check "exactly two messages" test "$(grep -c '^---$' "$H/messages.txt")" = 2
      check "the safe-mode start had safe mode on" grep -q "in safe mode" "$G/guard.log"
      local_starts() { python3 -I -c "import json; print(len(json.load(open('$G/starts.json'))['starts']))"; }
      check "two starts, no loop" test "$(local_starts)" = 2
      run_launcher "$D" "$H" OMSI_UPDATE_URL="file://$WORK/release-old.json"
      check "a later start shows nothing more" test "$(grep -c '^---$' "$H/messages.txt")" = 2
      ;;
    good_start_cleans_up)
      # a good start of an updated version: the kept earlier version goes, the watcher ends
      updated_install "$D" "$NEW_BIN"
      run_launcher "$D" "$H" OMSI_UPDATE_URL="file://$REL_NEW_GOOD"
      check "$NEW_V is installed" test "$(installed_version "$D")" = "$NEW_V"
      check "the kept earlier version is gone" bash -c "! ls -a '$D' | grep -q 'old-update\|openbus-previous'"
      check "the start is healthy in the journal" grep -q '"healthy": true' "$G/starts.json"
      check "no message was shown" test ! -s "$H/messages.txt"
      check "no watcher is left" bash -c "! pgrep -f 'openbus --guard-watch' > /dev/null"
      ;;
    *) echo "    unknown scenario"; FAILED=1 ;;
  esac
  if [ $FAILED = 0 ]; then PASS=$((PASS + 1)); else FAIL=$((FAIL + 1)); echo "    (files in $WORK/$s; guard log: $G/guard.log)"; fi
done
echo
echo "guard e2e: $PASS passed, $FAIL failed"
[ $FAIL = 0 ]
