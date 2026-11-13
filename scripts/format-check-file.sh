#!/usr/bin/env bash
set -uo pipefail

file=$1
if output=$(rustfmt --check --files-with-diff -- "$file"); then
    echo "$file is properly formatted. No further action needed."
else
    if grep -Fxq -- "$file" <<< "$output"; then
        echo "$file is not formatted properly."
        exit 1
    else
        echo "$file is properly formatted, although its children are not. Skipping."
    fi
fi