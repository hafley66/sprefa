#!/usr/bin/env bash
set -euo pipefail

LAB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "$LAB_DIR/../../../.." && pwd)"
OUT_DIR="$(mktemp -d)"
trap 'rm -rf "$OUT_DIR"' EXIT

compile_one() {
  local input_path="$1"
  local output_path="$2"
  swipl -q -l "$REPO_DIR/v6/prolog/compile/compile.pl" \
    -g "compile_dl6('$input_path', '$output_path')" \
    -g halt
}

compile_one "$LAB_DIR/1_rel_candidate.dl6" "$OUT_DIR/rel.ts"

if compile_one "$LAB_DIR/0_type_current.dl6" "$OUT_DIR/type.ts" \
    >"$OUT_DIR/type.stdout" 2>"$OUT_DIR/type.stderr"; then
  echo "fail  removed_type_surface_accepted"
  exit 1
fi

if ! grep -q "dl_parse_error(statement" "$OUT_DIR/type.stderr"; then
  echo "fail  removed_type_surface_wrong_refusal"
  sed -n '1,20p' "$OUT_DIR/type.stderr"
  exit 1
fi

echo "PASS  referenced_rel_surface_compiles"
echo "PASS  removed_type_surface_is_rejected"
