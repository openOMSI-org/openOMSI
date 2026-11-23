#!/usr/bin/env bash
# Audio traces: run a fixed set of offscreen drives with one openbus binary, each writing
# what the game asks of the mixer (OMSI_AUDIO_TRACE) and the sound mixed from it
# (OMSI_RECORD), and compare two such sets. A refactor that must not change what is heard
# writes the same traces as the binary built from the commit before it - the golden
# pictures (scripts/golden.sh) cover only what is seen. See docs/REFACTOR_CHECKS.md.
#
#   scripts/audio-trace.sh render  <openbus binary> <out dir> [scene ...]
#   scripts/audio-trace.sh window  <openbus binary> <out dir> [scene ...]
#   scripts/audio-trace.sh compare <dir a> <dir b>
#   scripts/audio-trace.sh list
#
# AUDIO_TRACE_SEED (1) changes OMSI_SEED. Needs the original OMSI 2 install ($OMSI_ROOT,
# else "OMSI 2 Original" beside the checkout); the ambience's recordings are taken from
# $OMSI_AMBIENCE_DIR, else ~/.openbus/ambience (without them the ambience is silent in
# both runs alike). Every scene runs with a fresh, empty $HOME and OMSI_SEED fixed.
#
# A run writes <scene>.trace (one line per request to the mixer: voices started with their
# file and parameters, parameter changes, stops and ends, the listener each step), <scene>.wav
# (the mix, 48 kHz) and <scene>.log. compare exits 0 when every trace is identical, 1
# otherwise, and prints for each trace that differs the first differing step, the sound
# files started more or less often, and whether the mixes differ (python3, standard library).
#
# window runs whole game sessions in a window that is never shown (OMSI_HIDDEN_WINDOW, muted)
# for what an offscreen run does not play: the scenery's sound objects, the bus radio, the
# passengers' voices. Their frames follow the wall clock, so <scene>.wtrace is compared only
# as which sounds were played (any of them played by one run only fails); run each binary
# twice to see what moves from run to run anyway (see docs/REFACTOR_CHECKS.md).
set -euo pipefail

repo="$(git -C "$(dirname "$0")" rev-parse --show-toplevel)"

# name | arguments (eval'd) | extra environment
SCENES=(
  # (golden.sh's drive: the SD202 started by its switches, driven off, AI traffic around it)
  "grundorf_drive_traffic|--time 08:00 --bus Vehicles/MAN_SD202/MAN_D86.bus --spawn 500,748,339 --view driver --traffic 15 --drive 12 --triggers cp_batterietrennschalter_toggle@0,kw_m_enginestart@1,kw_m_enginestart_off@3,automatic_D@4,parking_brake_release@5|"
  # (the same drive heard from outside, in the rain at night: rain on the street, the
  # bus's outside sounds, the traffic's lights-on hour)
  "grundorf_rain_outside|--time 22:30 --weather Weather/Schmuddelwetter.owt --bus Vehicles/MAN_SD202/MAN_D86.bus --spawn 500,748,339 --view outside --traffic 15 --drive 12 --triggers cp_batterietrennschalter_toggle@0,kw_m_enginestart@1,kw_m_enginestart_off@3,automatic_D@4,parking_brake_release@5|"
  # (a big city's ambience: the soundscape's places, birds and far life forty times as
  # often, the traffic, a standing bus heard from outside)
  "spandau_ambience|--map maps/Berlin-Spandau/global.cfg --time 08:00 --bus Vehicles/MAN_SD202/MAN_D86.bus --view outside --traffic 20 --schedule --passengers --drive 20 --triggers cp_batterietrennschalter_toggle@0,kw_m_enginestart@1,kw_m_enginestart_off@3|OMSI_AMBIENCE_DENSITY=40"
)
# name | arguments | seconds (whole sessions; the radio is tuned to a station nobody
# answers: its voice and volume are traced, not what it would play)
WINDOW_SCENES=(
  # (the farm stop: the scenery's cock and forest birds, the ambience, traffic, passengers)
  "grundorf_window|--map maps/Grundorf/global.cfg --bus Vehicles/MAN_SD202/MAN_D86.bus --spawn 500,748,339 --autostart --view driver --traffic 15 --passengers --time 07:59 --setvar Snd_Radio=1|50"
  "spandau_window|--map maps/Berlin-Spandau/global.cfg --bus Vehicles/MAN_NL_NG/MAN_EN92_main.bus --autostart --view driver --traffic 20 --passengers --schedule --time 08:00 --setvar Snd_Radio=1|45"
)
SEED="${AUDIO_TRACE_SEED:-1}"

usage() {
  sed -n '2,/^set -euo/p' "$0" | grep '^#' | sed 's/^# \{0,1\}//'
  exit 2
}

render() {
  local bin="$1" out="$2"
  shift 2
  [ -x "$bin" ] || { echo "not an executable: $bin" >&2; exit 2; }
  local root="${OMSI_ROOT:-$(dirname "$repo")/OMSI 2 Original}"
  [ -d "$root/maps" ] || { echo "no OMSI 2 install at $root (set OMSI_ROOT)" >&2; exit 2; }
  local amb="${OMSI_AMBIENCE_DIR:-$HOME/.openbus/ambience}"
  mkdir -p "$out"
  case "$(realpath "$out")/" in
    "$(realpath "$repo")"/*) echo "refusing to write recordings of licensed content inside the repository: $out" >&2; exit 2 ;;
  esac
  local want=" $* " s name args env home status=0
  for s in "${SCENES[@]}"; do
    name="${s%%|*}"
    args="${s#*|}"
    env="${args#*|}"
    args="${args%%|*}"
    if [ $# -gt 0 ] && [[ "$want" != *" $name "* ]]; then continue; fi
    home="$(mktemp -d "${TMPDIR:-/tmp}/audio-trace-home.XXXXXX")"
    mkdir -p "$home/.openbus"
    printf 'windy_trees=0\n' >"$home/.openbus/settings.cfg"
    rm -f "$out/$name.trace" "$out/$name.wav"
    printf '%-24s' "$name"
    # (a picture every 10 s is the least OMSI_RECORD takes; the pictures are not compared)
    if eval "env HOME=\"\$home\" OMSI_SEED=\"\$SEED\" OMSI_NO_UPDATE=1 OMSI_AMBIENCE_DIR=\"\$amb\" OMSI_RECORD=0.1 OMSI_AUDIO_TRACE=\"\$out/$name.trace\" $env \"\$bin\" --root \"\$root\" --offscreen \"\$out/$name.png\" --size 320x180 $args" >"$out/$name.log" 2>&1; then
      echo "ok ($(grep -c '^+ ' "$out/$name.trace" 2>/dev/null || echo 0) voices started)"
    else
      echo "FAILED (see $out/$name.log)"
      status=1
    fi
    rm -rf "$home" "$out/${name}_frames"
  done
  return $status
}

window() {
  local bin="$1" out="$2"
  shift 2
  [ -x "$bin" ] || { echo "not an executable: $bin" >&2; exit 2; }
  local root="${OMSI_ROOT:-$(dirname "$repo")/OMSI 2 Original}"
  [ -d "$root/maps" ] || { echo "no OMSI 2 install at $root (set OMSI_ROOT)" >&2; exit 2; }
  local amb="${OMSI_AMBIENCE_DIR:-$HOME/.openbus/ambience}"
  mkdir -p "$out"
  case "$(realpath "$out")/" in
    "$(realpath "$repo")"/*) echo "refusing to write traces of licensed content inside the repository: $out" >&2; exit 2 ;;
  esac
  local want=" $* " s name args secs home status=0
  for s in "${WINDOW_SCENES[@]}"; do
    name="${s%%|*}"
    args="${s#*|}"
    secs="${args##*|}"
    args="${args%|*}"
    if [ $# -gt 0 ] && [[ "$want" != *" $name "* ]]; then continue; fi
    home="$(mktemp -d "${TMPDIR:-/tmp}/audio-trace-home.XXXXXX")"
    mkdir -p "$home/.openbus"
    printf 'windy_trees=0\n' >"$home/.openbus/settings.cfg"
    printf 'volume = 0.7\nsilent = http://127.0.0.1:9/station.mp3\n' >"$home/.openbus/radio.cfg"
    rm -f "$out/$name.wtrace"
    printf '%-24s' "$name"
    if eval "env HOME=\"\$home\" OMSI_SEED=\"\$SEED\" OMSI_NO_UPDATE=1 OMSI_HIDDEN_WINDOW=1 OMSI_MUTE=1 OMSI_AMBIENCE_DIR=\"\$amb\" OMSI_AUDIO_TRACE=\"\$out/$name.wtrace\" \"\$bin\" --root \"\$root\" --no-menu --size 640x360 --exit-after $secs $args" >"$out/$name.log" 2>&1; then
      echo "ok ($(grep -c '^+ ' "$out/$name.wtrace" 2>/dev/null || echo 0) voices started)"
    else
      echo "FAILED (see $out/$name.log)"
      status=1
    fi
    rm -rf "$home"
    # (the session's last situation is saved beside the binary; the next run starts afresh)
    rm -f "$(dirname "$bin")"/maps/*/laststn.osn "$(dirname "$bin")"/maps/*/laststn.osn.scripttex/*.png
  done
  return $status
}

compare() {
  local a="$1" b="$2" s name differ=0 missing=0
  [ -d "$a" ] && [ -d "$b" ] || { echo "compare needs two directories" >&2; exit 2; }
  for s in "${SCENES[@]}"; do
    name="${s%%|*}"
    if [ ! -f "$a/$name.trace" ] && [ ! -f "$b/$name.trace" ]; then
      continue
    elif [ ! -f "$a/$name.trace" ] || [ ! -f "$b/$name.trace" ]; then
      printf '%-24s missing\n' "$name"
      missing=$((missing + 1))
    else
      # (the same requests in another order, as hash maps give them, are the same sound)
      if out="$(python3 -I "$repo/scripts/audio_trace_diff.py" "$a/$name.trace" "$b/$name.trace" "$a/$name.wav" "$b/$name.wav")"; then
        printf '%-24s same (%s)\n' "$name" "$(echo "$out" | grep -E '^(voices|mix)' | tr '\n' ';' | sed 's/;$//; s/;/; /g')"
      else
        printf '%-24s DIFFERS\n' "$name"
        echo "$out" | sed 's/^/    /'
        differ=$((differ + 1))
      fi
    fi
  done
  for s in "${WINDOW_SCENES[@]}"; do
    name="${s%%|*}"
    if [ ! -f "$a/$name.wtrace" ] && [ ! -f "$b/$name.wtrace" ]; then
      continue
    elif [ ! -f "$a/$name.wtrace" ] || [ ! -f "$b/$name.wtrace" ]; then
      printf '%-24s missing\n' "$name"
      missing=$((missing + 1))
    elif out="$(python3 -I "$repo/scripts/audio_trace_diff.py" --window "$a/$name.wtrace" "$b/$name.wtrace")"; then
      printf '%-24s same sounds (%s)\n' "$name" "$(echo "$out" | head -1)"
    else
      printf '%-24s OTHER SOUNDS\n' "$name"
      echo "$out" | sed '/^played by both:/,$d; s/^/    /'
      differ=$((differ + 1))
    fi
  done
  echo "$differ differ, $missing missing"
  [ "$differ" -eq 0 ] && [ "$missing" -eq 0 ]
}

case "${1:-}" in
  render) [ $# -ge 3 ] || usage; shift; render "$@" ;;
  window) [ $# -ge 3 ] || usage; shift; window "$@" ;;
  compare) [ $# -eq 3 ] || usage; compare "$2" "$3" ;;
  list) for s in "${SCENES[@]}" "${WINDOW_SCENES[@]}"; do echo "${s%%|*}"; done ;;
  *) usage ;;
esac
