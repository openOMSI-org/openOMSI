file=$1
if output=$(rustfmt --check --files-with-diff $file); then
    echo "$file is properly formatted. No further action needed."
else
    if [[ "$output" == *"$file"* ]]; then
        echo "$file is not formatted properly."
        exit 1
    else
        echo "$file is properly formatted, although its children are not. Skipping."
    fi
fi