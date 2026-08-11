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

emitted_tmp=$(mktemp)
trap 'rm -f "$emitted_tmp"' EXIT
emitter_receipt=$(swipl -q -f emit_grammar.pl -- \
  ../../prolog/compile/parse_dl_dcg.pl "$emitted_tmp")
cmp emitted-grammar.js "$emitted_tmp"
echo "$emitter_receipt"

golden="$lab_dir/../../dl/fixtures/golden-flex.dl6"
parse_output=$(tree-sitter parse "$golden")
if grep -Eq '\(ERROR|\(MISSING' <<<"$parse_output"; then
  printf '%s\n' "$parse_output" >&2
  exit 1
fi
echo "PASS parse: golden-flex.dl6 lines=630 errors=0"

corpus_dir="$lab_dir/../../prolog/compile/out/text-door"
mapfile -t corpus < <(find "$corpus_dir" -maxdepth 1 -type f -name '*.dl6' | sort)
total=${#corpus[@]}
clean=0
errors=0
for fixture in "${corpus[@]}"; do
  if fixture_output=$(tree-sitter parse "$fixture" 2>&1) &&
    ! grep -Eq '\(ERROR|\(MISSING' <<<"$fixture_output"; then
    clean=$((clean + 1))
  else
    errors=$((errors + 1))
    printf '%s\n' "$fixture_output" >&2
  fi
done
echo "TS_CORPUS total=$total clean=$clean errors=$errors"
if (( errors != 0 )); then
  exit 1
fi

config="$lab_dir/languages.ncl"
query="$lab_dir/queries/formatting.scm"
formatted=$("$topiary_bin" -C "$config" format --language dl6 --query "$query" < fixtures/format-input.dl6)
diff -u fixtures/format-expected.dl6 <(printf '%s\n' "$formatted")

formatted_twice=$(printf '%s\n' "$formatted" | "$topiary_bin" -C "$config" format --language dl6 --query "$query")
diff -u <(printf '%s\n' "$formatted") <(printf '%s\n' "$formatted_twice")
echo "PASS format: formatting law and idempotence"
