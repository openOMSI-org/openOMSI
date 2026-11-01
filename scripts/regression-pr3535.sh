#!/usr/bin/env bash
# Reproduce the PR #3535 regression matrix without a live VPS or a graphics device.
# Run from any directory: bash scripts/regression-pr3535.sh
set -euo pipefail
cd "$(dirname "$0")/.."

echo "[1/7] Generated debug flags documentation"
cargo test -p omsi-cfg --lib flags::tests::flags_doc_up_to_date

echo "[2/7] Cache budgets, grace period, server pins and symlink safety"
cargo test -p omsi-cfg --lib cache_budget::tests

echo "[3/7] Simulated unreachable mod channel and verified local fleet fallback"
cargo test -p omsi-lan --lib game::local_fleet::tests

echo "[4/7] Fleet validation and authenticated loopback request"
cargo test -p omsi-lan --lib mods::server_fleet::tests
cargo test -p omsi-lan --lib mods::server_tests::selected_bus_request_is_rejected_without_an_authorized_fleet

echo "[5/7] Renderer road-glare vs. ground-pass flag regression"
cargo test -p omsi-render --lib shaders::tests::road_glare_uses_dedicated_lod_lane

echo "[6/7] Concurrent joins, FIFO/WAIT, legacy peers and per-IP limits"
cargo test -p omsi-net --lib tests::join_queue

echo "[7/7] Mobile consent, launcher fleet UI and rejection paths"
node --experimental-vm-modules --test \
  launcher/tests/mobile-fleet-consent.test.mjs \
  launcher/tests/server-fleet.test.mjs

echo "PR #3535 regression matrix passed (unit/loopback simulation; live VPS/GPU QA still required)."
