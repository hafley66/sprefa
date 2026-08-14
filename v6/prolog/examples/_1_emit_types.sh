#!/usr/bin/env bash
set -euo pipefail

example_dir="$(cd "$(dirname "$0")" && pwd)"
prolog_dir="$(cd "$example_dir/.." && pwd)"
source_file="$example_dir/_0_emitter_types.dl6"
output_dir="$example_dir/emitted"

mkdir -p "$output_dir"
cd "$prolog_dir"

swipl -q -l compile.pl -l compile/9_emit_type_artifact.pl \
  -g "compile_dl6('$source_file','$output_dir/_0_emitter_types.ts',[emitter(emit_type_artifact:emit_ts_types)])" \
  -t halt

swipl -q -l compile.pl -l compile/9_emit_type_artifact.pl \
  -g "compile_dl6('$source_file','$output_dir/_0_emitter_types.rs',[emitter(emit_type_artifact:emit_rust_types)])" \
  -t halt

swipl -q -l compile.pl -l compile/9_emit_type_artifact.pl \
  -g "compile_dl6('$source_file','$output_dir/_0_emitter_types.schema.json',[emitter(emit_type_artifact:emit_jsonschema)])" \
  -t halt
