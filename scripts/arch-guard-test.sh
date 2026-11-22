#!/usr/bin/env bash
# Tests of scripts/arch-guard.sh's growth limit for omsi-app: each case is a throwaway
# repository with a base commit and a change on it, checked by the real guard with
# `app-growth 10`.
#
#   scripts/arch-guard-test.sh
#
# Covers: a change that only formats a file counts no growth; real growth over the limit
# fails; formatting with a little growth counts only the growth; a base rustfmt cannot parse,
# or no working rustfmt at all, falls back to the lines as they are; test files do not count.
# Needs bash, git, python3 and rustfmt. Exits 0 when every case passed.
set -uo pipefail

repo="$(git -C "$(dirname "$0")" rev-parse --show-toplevel)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
fails=0

# An unformatted file: one long line that rustfmt spreads over many.
UGLY='pub fn add(a: i32, b: i32) -> i32 { let x = a + b; let y = x * 2; let z = y - a; let w = z + b; if w > 10 { w } else { x } }'
fmt() { rustfmt --emit stdout --config-path "$repo" <<<"$1"; }
PRETTY="$(fmt "$UGLY")"
# three such functions on one line: their reformat alone is more than the limit's 10 lines
BIG="${UGLY} ${UGLY//add/sub} ${UGLY//add/mul}"
BIG_PRETTY="$(fmt "$BIG")"
more() { # n new lines of code: n/3 small functions of 3 lines each
  local i
  for ((i = 0; i < $1 / 3; i++)); do printf 'pub fn f%d() -> i32 {\n    %d\n}\n' "$i" "$i"; done
}

# check <name> <expected exit> <expected text> <base lib.rs> <changed lib.rs> [extra setup command]
check() {
  local name=$1 want=$2 text=$3 base=$4 head=$5 setup=${6:-}
  local d="$work/$name"
  mkdir -p "$d/scripts" "$d/crates/omsi-app/src"
  cp "$repo/scripts/arch-guard.sh" "$d/scripts/"
  cp "$repo/rustfmt.toml" "$d/" 2>/dev/null || true
  printf 'app-growth 10\n' >"$d/scripts/arch-rules.txt"
  printf '[package]\nname = "omsi-app"\nversion = "0.0.0"\n' >"$d/crates/omsi-app/Cargo.toml"
  printf '%s\n' "$base" >"$d/crates/omsi-app/src/lib.rs"
  git -C "$d" init -q && git -C "$d" config core.autocrlf false && git -C "$d" add -A && git -C "$d" -c user.name=t -c user.email=t@t commit -qm base
  local b
  b="$(git -C "$d" rev-parse HEAD)"
  printf '%s\n' "$head" >"$d/crates/omsi-app/src/lib.rs"
  [ -n "$setup" ] && (cd "$d" && eval "$setup")
  git -C "$d" add -A && git -C "$d" -c user.name=t -c user.email=t@t commit -qm change
  local out code
  out="$(cd "$d" && BASE=$b RUSTFMT="${GUARD_RUSTFMT:-rustfmt}" bash scripts/arch-guard.sh 2>&1)"
  code=$?
  if [ "$code" = "$want" ] && grep -qF -- "$text" <<<"$out"; then
    echo "ok    $name"
  else
    echo "FAIL  $name: exit $code (wanted $want), output:"
    sed 's/^/      /' <<<"$out"
    fails=$((fails + 1))
  fi
}

check formatting_only 0 "omsi-app +0 lines" "$BIG" "$BIG_PRETTY"
check growth_over_the_limit 1 "adds 12 lines" "$PRETTY" "$PRETTY
$(more 12)"
check growth_within_the_limit 0 "omsi-app +6 lines" "$PRETTY" "$PRETTY
$(more 6)"
check formatting_and_growth_counts_the_growth 0 "omsi-app +9 lines" "$UGLY" "$PRETTY
$(more 9)"
# (rustfmt cannot parse the base: its lines count as they are, so the reformat is growth)
check unparsable_base_counts_lines_as_they_are 1 "more than the 10" "$BIG {" "$BIG_PRETTY"
check test_files_do_not_count 0 "omsi-app +0 lines" "$PRETTY" "$PRETTY" \
  "mkdir -p crates/omsi-app/src/tests && printf '%s\n' \"\$(more 30)\" >crates/omsi-app/src/tests/mod.rs"

# no rustfmt to run: the reformat counts as growth
GUARD_RUSTFMT="$work/no-such-rustfmt" check without_rustfmt_counts_lines_as_they_are 1 "more than the 10" "$BIG" "$BIG_PRETTY"

if [ "$fails" -gt 0 ]; then
  echo "arch guard tests: $fails failed"
  exit 1
fi
echo "arch guard tests: all passed"
