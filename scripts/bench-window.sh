#!/usr/bin/env bash
# The window's own frame measured without a window on the screen: a session run with
# OMSI_HIDDEN_WINDOW (never shown, frames drawn into a texture of its size) and OMSI_PROFILE,
# its exit summary as JSON for scripts/compare-performance.py.
#
#   scripts/bench-window.sh <openbus binary> <out prefix> [plain|enhanced] [seconds] [extra args ...]
#
# The scene: Berlin-Spandau at 08:00, the MAN EN92 started (--autostart), traffic 30, the
# passengers and the timetable, the driver's view at 1280x720; at 44 s the parking brake
# is let go and the bus driven off on full throttle (it runs into the scenery a little
# later and stands there). The run ends after <seconds> (110 by default; the warm-up of
# the summary is the first 15 s of play). Writes <out prefix>.log and <out prefix>.json.
#
# BENCH_STILL=1: the bus is never driven off (no input at all): the camera stands still the
# whole run, so whatever makes the frame uneven is the game's own periodic work, not the drive.
#
# BENCH_FPS=<n>: the frame rate limited to n (30 or 60: what most players see; the fixed
# simulation step runs more ticks a frame the lower it is, which an unlimited run on a fast
# machine never shows). BENCH_TRAFFIC=<n>: the traffic's density (30 by default).
# BENCH_TIME=<HH:MM>: the time of day (08:00 by default).
#
# Every run has a fresh, empty $HOME (default settings, sound off) and OMSI_SEED fixed, and no frame
# rate limit unless BENCH_FPS asks for one. Compare runs of one machine only, and several
# of each: the frame times move with whatever else the machine does; the stages' CPU times
# much less.
# Needs the original OMSI 2 install: $OMSI_ROOT, else "OMSI 2 Original" beside the checkout.
set -euo pipefail

repo="$(git -C "$(dirname "$0")" rev-parse --show-toplevel)"
[ $# -ge 2 ] || { sed -n '2,/^set -euo/p' "$0" | grep '^#' | sed 's/^# \{0,1\}//'; exit 2; }
bin="$1"; out="$2"; mode="${3:-plain}"; secs="${4:-110}"
shift $(( $# < 4 ? $# : 4 ))
[ -x "$bin" ] || { echo "not an executable: $bin" >&2; exit 2; }
root="${OMSI_ROOT:-$(dirname "$repo")/OMSI 2 Original}"
[ -d "$root/maps" ] || { echo "no OMSI 2 install at $root (set OMSI_ROOT)" >&2; exit 2; }
extra=()
[ "$mode" = enhanced ] && extra+=(--enhanced)
home="$(mktemp -d "${TMPDIR:-/tmp}/bench-home.XXXXXX")"
trap 'rm -rf "$home"' EXIT
mkdir -p "$home/.openbus"
# (the swaying trees move with the wall clock)
printf 'windy_trees=0\nvolume=0\nvol_ai=0\nvol_ambient=0\nambient=0\n' >"$home/.openbus/settings.cfg"
drive="t=44 key .; t=45 keydown W"
[ "${BENCH_STILL:-0}" = 1 ] && drive="t=99999 key ."
env HOME="$home" OMSI_SEED=1 OMSI_HIDDEN_WINDOW=1 OMSI_NO_UPDATE=1 OMSI_MAX_FPS="${BENCH_FPS:-1000}" \
  OMSI_PROFILE=1 OMSI_PROFILE_JSON="$out.json" \
  OMSI_INPUT="$drive" \
  "$bin" --root "$root" --no-menu --map maps/Berlin-Spandau/global.cfg \
  --bus Vehicles/MAN_NL_NG/MAN_EN92_main.bus --autostart --view driver \
  --traffic "${BENCH_TRAFFIC:-30}" --passengers --schedule --time "${BENCH_TIME:-08:00}" --size 1280x720 \
  --exit-after "$secs" ${extra[@]+"${extra[@]}"} ${@+"$@"} >"$out.log" 2>&1
grep -E "profile: (since|window)" "$out.log" | sed 's/^.*profile: /profile: /'
