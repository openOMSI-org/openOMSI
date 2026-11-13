#!/usr/bin/env bash
set -euo pipefail

git diff --name-only --diff-filter=d "${BASE:-origin/main}" HEAD -- '*.rs' | xargs -t -I{} rustfmt --check --files-with-diff {}

cat scripts/formatted-crates.txt | xargs -t -I{} cargo fmt --check -p {} -- --files-with-diff