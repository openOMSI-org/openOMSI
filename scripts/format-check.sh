#!/usr/bin/env bash
set -euo pipefail

if [ -z ${BASE+x} ]; then comp="main...HEAD"; else comp="HEAD^1 HEAD"; fi
git diff --name-only --line-prefix="$(git rev-parse --show-toplevel)/" -z --diff-filter=d "$comp" -- '*.rs' | xargs -0 -I{} scripts/format-check-file.sh {}

cat scripts/formatted-crates.txt | xargs -t -I{} cargo fmt --check -p {} -- --files-with-diff
