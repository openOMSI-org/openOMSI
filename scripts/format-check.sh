#!/usr/bin/env bash
set -euo pipefail

git diff --name-only -z --diff-filter=d "${BASE:-origin/main}" HEAD -- '*.rs' | xargs -0 -I{} scripts/format-check-file.sh {}

cat scripts/formatted-crates.txt | xargs -t -I{} cargo fmt --check -p {} -- --files-with-diff