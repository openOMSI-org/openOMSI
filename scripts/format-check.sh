#!/usr/bin/env bash
# Check that all changed Rust files are properly formatted: compares the branches and lists
# all changed files, greps Rust code and checks the formatting via rustfmt.
git diff --name-only main...HEAD -- | grep .rs$ | xargs -t -i rustfmt --check --files-with-diff {}

# Check that all crates listed in formatted-crates.txt are properly formatted (runs `cargo fmt` cratewise).
cat scripts/formatted-crates.txt | xargs -t -i cargo fmt --check -p {} -- --files-with-diff