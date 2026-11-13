#!/usr/bin/env bash
set -euo pipefail

git diff --name-only -z --diff-filter=d "${BASE:-origin/main}" HEAD -- '*.rs' ':!**/main.rs' ':!**/lib.rs' | xargs -t -0 -I{} rustfmt --check --files-with-diff {}

cat scripts/formatted-crates.txt | xargs -t -I{} cargo fmt --check -p {} -- --files-with-diff