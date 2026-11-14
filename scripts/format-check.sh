#!/usr/bin/env bash
set -euo pipefail


git diff --name-only --line-prefix="$(git rev-parse --show-toplevel)/" -z --diff-filter=d "${BASE:-origin/main}" HEAD -- '*.rs' | xargs -0 -I{} scripts/format-check-file.sh {}


cat scripts/formatted-crates.txt | xargs -t -I{} cargo fmt --check -p {} -- --files-with-diff
