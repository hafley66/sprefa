#!/usr/bin/env bash
set -euo pipefail

lab_dir=$(cd "$(dirname "$0")" && pwd)
cd "$lab_dir"

topiary_bin="$lab_dir/tools/bin/topiary"
if [[ ! -x "$topiary_bin" ]]; then
  cargo install topiary-cli --version 0.7.3 --locked --root "$lab_dir/tools" || {
    echo "BLOCKED: could not install Topiary 0.7.3 inside the lab" >&2
    exit 2
  }
fi

tree-sitter generate
tree-sitter build -o build/dl6.dylib

parse_output=$(tree-sitter parse fixtures/golden-flex-175-236.dl6)
if grep -Eq '\(ERROR|\(MISSING' <<<"$parse_output"; then
  echo "$parse_output" >&2
  exit 1
fi
echo "PASS parse: golden-flex.dl6 lines 175-236 contain zero ERROR/MISSING nodes"

config="$lab_dir/languages.ncl"
query="$lab_dir/queries/formatting.scm"
formatted=$("$topiary_bin" -C "$config" format --language dl6 --query "$query" < fixtures/format-input.dl6)
diff -u fixtures/format-expected.dl6 <(printf '%s\n' "$formatted")

formatted_twice=$(printf '%s\n' "$formatted" | "$topiary_bin" -C "$config" format --language dl6 --query "$query")
diff -u <(printf '%s\n' "$formatted") <(printf '%s\n' "$formatted_twice")
echo "PASS format: formatting law and idempotence"
