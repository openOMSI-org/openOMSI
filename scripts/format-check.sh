#!/usr/bin/env bash
set -euo pipefail

# Check that all changed Rust files are properly formatted: compares the branches and lists
# all changed files, greps Rust code and checks the formatting via rustfmt.
git diff --name-only --diff-filter=d "${BASE:-origin/main}" HEAD -- '*.rs' | xargs -t -I{} rustfmt --check --files-with-diff {}

# Check that all crates listed in formatted-crates.txt are properly formatted (runs `cargo fmt` cratewise).
cat scripts/formatted-crates.txt | xargs -t -I{} cargo fmt --check -p {} -- --files-with-diff