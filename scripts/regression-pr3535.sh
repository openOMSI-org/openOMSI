#!/usr/bin/env bash
# Reproduce the PR #3535 regression matrix without a live VPS or a graphics device.
# Run from any directory: bash scripts/regression-pr3535.sh
set -euo pipefail
cd "$(dirname "$0")/.."

# Cargo succeeds even when a stale filter selects zero tests. Require each group
# to execute at least one assertion before reporting this matrix as green.
rust_tests() {
  local krate="$1" filter="$2" output
  shift 2
  output=$(mktemp)
  if ! cargo test --locked --color never -p "$krate" --lib "$filter" -- --color never "$@" 2>&1 | tee "$output"; then
    rm -f "$output"
    return 1
  fi
  if ! grep -Eq '^test result: ok\. [1-9][0-9]* passed;' "$output"; then
    echo "ERROR: $krate / $filter did not execute any passing tests." >&2
    rm -f "$output"
    return 1
  fi
  rm -f "$output"
}

echo "[1/8] Debug flag registry and generated documentation"
rust_tests omsi-cfg flags::tests

echo "[2/8] Cache budgets, game leases, server pins and symlink safety"
rust_tests omsi-cfg cache_budget::tests
rust_tests omsi-cfg cache_activity::tests

echo "[3/8] Simulated unreachable mod channel and verified local fleet fallback"
rust_tests omsi-lan game::local_fleet::tests

echo "[4/8] Fleet validation and authenticated loopback request"
rust_tests omsi-lan mods::server_fleet::tests
rust_tests omsi-lan mods::server::tests::selected_bus_request_is_rejected_without_an_authorized_fleet --exact

echo "[5/8] Renderer road-glare vs. ground-pass flag regression"
rust_tests omsi-render shaders::tests::road_glare_uses_dedicated_lod_lane --exact

echo "[6/8] Concurrent joins, FIFO/WAIT, legacy peers and per-IP limits"
rust_tests omsi-net tests::join_queue

echo "[7/8] Graphics confirmation rollback and active renderer preservation"
rust_tests omsi-app game_lists::graphics::graphics_confirm_tests

echo "[8/8] Mobile consent, launcher fleet UI, translations and legacy servers"
node --experimental-vm-modules --test \
  launcher/tests/mobile-fleet-consent.test.mjs \
  launcher/tests/server-fleet.test.mjs \
  launcher/tests/server-fleet-translations.test.mjs

echo "PR #3535 regression matrix passed (unit/loopback simulation; live VPS/GPU QA still required)."
