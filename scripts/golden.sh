#!/usr/bin/env bash
# Golden pictures: render a fixed set of offscreen scenes with one openbus binary, and compare
# two such sets pixel by pixel. A refactor that must not change behaviour renders the same
# pictures as the binary built from the commit before it. See docs/REFACTOR_CHECKS.md.
#
#   scripts/golden.sh render  <openbus binary> <out dir> [scene ...]
#   scripts/golden.sh compare <dir a> <dir b>
#   scripts/golden.sh list
#
# GOLDEN_SIZE (960x540) and GOLDEN_SEED (1) change the picture size and OMSI_SEED.
#
# render needs the original OMSI 2 install: $OMSI_ROOT, else "OMSI 2 Original" beside the
# checkout. The pictures are made from licensed content: keep the out dirs outside the
# repository and never commit them.
#
# Every scene runs with a fresh, empty $HOME (default settings, nothing remembered from the
# last run) and OMSI_SEED fixed, so two runs of one binary give the same bytes.
# compare exits 0 when every picture is identical (or, for the few scenes with an allowance,
# within it), 1 otherwise, and prints for each one that differs the share of differing pixels
# and the largest and the mean channel difference (0..255); it also writes an amplified
# <name>.diff.png into dir b's diff/ (python3, standard library only).
set -euo pipefail

repo="$(git -C "$(dirname "$0")" rev-parse --show-toplevel)"

# name | arguments (eval'd, so quoting works) | extra environment | allowed share of
# differing pixels in % (empty: none; only where something moves with the wall clock)
SCENES=(
  "grundorf_day|--time 12:00|"
  "grundorf_night|--time 23:30|"
  "grundorf_rain|--time 14:00 --weather Weather/Schmuddelwetter.owt|"
  "grundorf_fog_dusk|--time 20:45 --weather Weather/Daemmerungsnebel.owt|"
  "grundorf_enhanced|--time 18:30 --enhanced||0.01"
  "grundorf_bus_outside|--time 10:00 --bus Vehicles/MAN_SD200/MAN_SD80.bus --view outside|"
  # (a dozen pixels of an instrument below the speedometer change from run to run)
  "grundorf_cockpit|--time 10:00 --bus Vehicles/MAN_SD202/MAN_D86.bus --view driver||0.01"
  # (the SD202 started by its switches and driven off from stop Bauernhof; the exhaust smoke
  # in the left mirror is animated by the renderer's real-time clock, hence its allowance)
  "grundorf_drive_traffic|--time 08:00 --bus Vehicles/MAN_SD202/MAN_D86.bus --spawn 500,748,339 --view driver --traffic 15 --drive 12 --triggers cp_batterietrennschalter_toggle@0,kw_m_enginestart@1,kw_m_enginestart_off@3,automatic_D@4,parking_brake_release@5||1"
  # (now and then a single pixel off by one)
  "spandau_day|--map maps/Berlin-Spandau/global.cfg --time 11:00||0.01"
)
# settings.cfg of every run: the defaults, less what moves with the wall clock (the
# renderer's animation clock is real time since start, so swaying trees are never twice alike)
SETTINGS=(
  "windy_trees=0"
)
SIZE="${GOLDEN_SIZE:-960x540}"
SEED="${GOLDEN_SEED:-1}"

usage() {
  sed -n '2,/^set -euo/p' "$0" | grep '^#' | sed 's/^# \{0,1\}//'
  exit 2
}

scene_names() {
  local s
  for s in "${SCENES[@]}"; do echo "${s%%|*}"; done
}

render() {
  local bin="$1" out="$2"
  shift 2
  [ -x "$bin" ] || { echo "not an executable: $bin" >&2; exit 2; }
  local root="${OMSI_ROOT:-$(dirname "$repo")/OMSI 2 Original}"
  [ -d "$root/maps" ] || { echo "no OMSI 2 install at $root (set OMSI_ROOT)" >&2; exit 2; }
  mkdir -p "$out"
  case "$(realpath "$out")/" in
    "$(realpath "$repo")"/*) echo "refusing to write pictures of licensed content inside the repository: $out" >&2; exit 2 ;;
  esac
  local want=" $* " s name args env home status=0
  for s in "${SCENES[@]}"; do
    name="${s%%|*}"
    args="${s#*|}"
    env="${args#*|}"
    env="${env%%|*}"
    args="${args%%|*}"
    if [ $# -gt 0 ] && [[ "$want" != *" $name "* ]]; then continue; fi
    home="$(mktemp -d "${TMPDIR:-/tmp}/golden-home.XXXXXX")"
    mkdir -p "$home/.openbus"
    printf '%s\n' "${SETTINGS[@]}" >"$home/.openbus/settings.cfg"
    printf '%-24s' "$name"
    if eval "env HOME=\"\$home\" OMSI_SEED=\"\$SEED\" $env \"\$bin\" --root \"\$root\" --offscreen \"\$out/$name.png\" --size \"\$SIZE\" $args" >"$out/$name.log" 2>&1; then
      echo "ok"
    else
      echo "FAILED (see $out/$name.log)"
      status=1
    fi
    rm -rf "$home"
  done
  head -1 "$out/$(scene_names | head -1).log" 2>/dev/null | sed 's/^/build: /' || true
  return $status
}

compare() {
  local a="$1" b="$2" s name allow differ=0 missing=0
  [ -d "$a" ] && [ -d "$b" ] || { echo "compare needs two directories" >&2; exit 2; }
  for s in "${SCENES[@]}"; do
    name="${s%%|*}"
    allow="${s##*|}"
    if [ ! -f "$a/$name.png" ] && [ ! -f "$b/$name.png" ]; then
      continue
    elif [ ! -f "$a/$name.png" ] || [ ! -f "$b/$name.png" ]; then
      printf '%-24s missing\n' "$name"
      missing=$((missing + 1))
    elif cmp -s "$a/$name.png" "$b/$name.png"; then
      printf '%-24s identical\n' "$name"
    else
      printf '%-24s ' "$name"
      if command -v python3 >/dev/null; then
        mkdir -p "$b/diff"
        python3 -I "$repo/scripts/png_diff.py" "$a/$name.png" "$b/$name.png" "$b/diff/$name.diff.png" "${allow:-0}" ||
          differ=$((differ + 1))
      else
        echo "differs (no python3 for the numbers)"
        differ=$((differ + 1))
      fi
    fi
  done
  echo "$differ differ, $missing missing"
  [ "$differ" -eq 0 ] && [ "$missing" -eq 0 ]
}

case "${1:-}" in
  render) [ $# -ge 3 ] || usage; shift; render "$@" ;;
  compare) [ $# -eq 3 ] || usage; compare "$2" "$3" ;;
  list) scene_names ;;
  *) usage ;;
esac
