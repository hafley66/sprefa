#!/usr/bin/env bash
# @comment-ok: the golden driver contract, the single doc site for the gate.
# typegen_golden.sh -- Phase F gate (plans/2026-08-14-phase-f-typegen-dl6.md).
#
# For a pinned fixture set, dump the type plane to JSONL through the new prolog
# door (typegen_export.pl), run the checked-in dl6 renderer (render_ts.dl6) on
# the real tsv2 runtime with those JSONL rows as arrivals, assemble each
# rendered_type rel into a TS file, and diff it against committed goldens.
#
# Exit nonzero on any diff; prints each diff.
# Baseline receipt 2026-08-18: base 7072e4c90 and relation-ID head 49f1019ff
# both return SQLITE_ERROR: no such table: __gen__list_text_df210f232c1299bd
# from POST /program for render_ts.dl6 before the rendered_type query runs.
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
COMPILE_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
PROLOG_DIR="$(cd "$COMPILE_DIR/.." && pwd)"
V6_DIR="$(cd "$PROLOG_DIR/.." && pwd)"
TSV2_DIR="$V6_DIR/tsv2"
RENDERER="$V6_DIR/dl/typegen/render_ts.dl6"
RUST_RENDERER="$V6_DIR/dl/typegen/render_rust.dl6"
GOLDEN_DIR="$SCRIPT_DIR/typegen_golden"
SCHEMA_VALIDATOR="$TSV2_DIR/scripts/0_typegen_schema_validate.mjs"

WORK="$(mktemp -d "${TMPDIR:-/tmp}/typegen-golden.XXXXXX")"
FAILED=0
server_pid=""

cleanup_server() {
  if [ -n "${server_pid:-}" ] && kill -0 "$server_pid" 2>/dev/null; then
    kill "$server_pid" 2>/dev/null || true
    wait "$server_pid" 2>/dev/null || true
  fi
  server_pid=""
}

trap cleanup_server EXIT INT TERM

# name:fixture_file  (fixture files live in v6/prolog/conformance/fixtures)
PINNED=(
  "generic_expansion_end_to_end:0_generic_expand.pl"
  "nested_list_of_text_round_trips:10_list_elements.pl"
  "list_of_json_documents_round_trips:10_list_elements.pl"
  "split_initcap_and_fold_render_pascal_case:15_string_split.pl"
  "anonymous-type-syntax:$V6_DIR/dl/fixtures/anonymous-type-syntax.dl6"
  "rust-associated-outputs:$V6_DIR/dl/fixtures/rust-associated-outputs.dl6"
  "type-annotation-ci:$V6_DIR/dl/fixtures/type-annotation-ci.dl6"
  "type-reflection:$V6_DIR/dl/fixtures/0_type-reflection.dl6"
  "compiler-derived-relation:$V6_DIR/dl/fixtures/0_compiler-derived-relation.dl6"
)

# Constructs the current type-plane door mints for no fixture; rows checked in
# at typegen_golden/<name>.type_rows.jsonl, one golden judged for both doors.
SHAPES=(
  "shape_interface_declaration"
  "shape_generic_rel"
  "shape_module_prefix_collision"
  "shape_list_nesting_depth"
  "shape_option_of_list"
  "shape_option_nested_enum"
  "shape_list_nesting_depth_five"
  "shape_list_nesting_depth_six"
  "shape_float_column"
  "shape_camel_case_module"
  "shape_concrete_dunder_rel"
  "shape_keyword_column"
  "parameterized_enum_two_instantiations"
)

swipl_run() { # goal ; runs from v6/prolog so conformance/... paths resolve
  ( cd "$PROLOG_DIR" && swipl -q -l "$COMPILE_DIR/typegen_export.pl" -g "$1" -g halt 2>/dev/null )
}

dump_rows() { # name fixture_file -> writes $WORK/<name>.jsonl
  local name="$1" fixture_file="$2"
  swipl_run "dump_fixture_rows('conformance/fixtures/$fixture_file', '$name', '$WORK/$name.jsonl')"
}

render_fixture() { # name renderer outfile -> writes $WORK/<outfile>
  local name="$1" renderer="$2" outfile="$3"
  local port="${TYPEGEN_PORT:-0}"
  local base=""

  (
    cd "$V6_DIR"
    exec env TSV2_DB=":memory:" TSV2_PORT="$port" NODE_NO_WARNINGS=1 \
      node --experimental-transform-types "$TSV2_DIR/serve/main.ts"
  ) >"$WORK/$name.server.log" 2>&1 &
  server_pid=$!

  local ready=0
  for _ in $(seq 1 100); do
    local reported_port
    reported_port="$(sed -n 's/^tsv2 serving on \([0-9][0-9]*\).*/\1/p' "$WORK/$name.server.log" | tail -1)"
    if [ -n "$reported_port" ]; then
      base="http://127.0.0.1:$reported_port"
      if curl -sS -o /dev/null --max-time 1 "$base/stats" 2>/dev/null; then ready=1; break; fi
    fi
    kill -0 "$server_pid" 2>/dev/null || break
    sleep 0.05
  done

  local ok=0
  if [ "$ready" = 1 ]; then
    python3 -c "
import json,sys
arrs=[json.loads(l) for l in open('$WORK/$name.jsonl') if l.strip()]
json.dump({'batch':arrs}, open('$WORK/$name.arrivals.json','w'))
"
    local program_status arrivals_status render_status
    program_status="$(curl -sS -o "$WORK/$name.program.response" -w '%{http_code}' -X POST --data-binary @"$renderer" "$base/program" || true)"
    if [ "$program_status" = 200 ]; then
      arrivals_status="$(curl -sS -o "$WORK/$name.arrivals.response" -w '%{http_code}' -X POST --data-binary @"$WORK/$name.arrivals.json" "$base/edb/events" || true)"
      if [ "$arrivals_status" = 200 ]; then
        render_status="$(curl -sS -o "$WORK/$name.rendered.json" -w '%{http_code}' "$base/idb/rendered_type" || true)"
        if [ "$render_status" = 200 ]; then
        python3 -c "
import json
d=json.load(open('$WORK/$name.rendered.json'))
rows=sorted(d['rows'], key=lambda r: (r[1], r[2]))
open('$WORK/$outfile','w').write('\n'.join(r[3] for r in rows))
"
          ok=1
        fi
      fi
    fi
    if [ "$ok" != 1 ]; then
      echo "FAIL  $name: tsv2 request failed (program=${program_status:-not-run}, arrivals=${arrivals_status:-not-run}, rendered_type=${render_status:-not-run})"
      echo "SERVER LOG  $WORK/$name.server.log"
      sed -n '1,160p' "$WORK/$name.server.log"
      for response in "$WORK/$name.program.response" "$WORK/$name.arrivals.response"; do
        if [ -s "$response" ]; then
          echo "RESPONSE  $response"
          sed -n '1,160p' "$response"
        fi
      done
    fi
  else
    echo "FAIL  $name: tsv2 server did not become ready"
    echo "SERVER LOG  $WORK/$name.server.log"
    sed -n '1,160p' "$WORK/$name.server.log"
  fi

  cleanup_server
  [ "$ok" = 1 ]
}

render_prolog() { # name -> writes $WORK/<name>.prolog.ts from the same JSONL
  local name="$1"
  swipl_run "write_prolog_types('$WORK/$name.jsonl', '$WORK/$name.prolog.ts')"
  [ -s "$WORK/$name.prolog.ts" ]
}

render_prolog_schema() { # name -> writes $WORK/<name>.schema.json
  local name="$1"
  ( cd "$PROLOG_DIR" && swipl -q -l "$COMPILE_DIR/typegen_export.pl" \
      -g "typegen_export:read_row_lines('$WORK/$name.jsonl', Rows), emit_jsonschema:jsonschema_text('$name', Rows, Text), open('$WORK/$name.schema.json', write, Stream), format(Stream, '~s', [Text]), close(Stream)" \
      -g halt 2>/dev/null )
  [ -s "$WORK/$name.schema.json" ]
}

compile_ts() {
  local name="$1"
  ( cd "$TSV2_DIR" && pnpm exec tsgo --ignoreConfig --noEmit --strict \
      --skipLibCheck "$WORK/$name.types.ts" )
}

compile_rust() {
  local name="$1"
  local crate="$WORK/rust-$name"
  mkdir -p "$crate/src"
  cp "$WORK/$name.types.rs" "$crate/src/lib.rs"
  cat >"$crate/Cargo.toml" <<'EOF'
[package]
name = "typegen_artifact"
version = "0.0.0"
edition = "2021"

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
EOF
  if [ "$name" = "generic_expansion_end_to_end" ]; then
    cat >>"$crate/src/lib.rs" <<'EOF'

#[cfg(test)]
mod generated_value_tests {
    use super::Person;

    #[test]
    fn product_value_round_trips_through_serde() {
        let value = Person { id: 7, name: "Ada".to_owned() };
        let wire = serde_json::to_string(&value).unwrap();
        assert_eq!(serde_json::from_str::<Person>(&wire).unwrap(), value);
    }
}
EOF
  fi
  cargo test --quiet --manifest-path "$crate/Cargo.toml"
}

validate_schema() {
  local name="$1" kind="$2"
  node "$SCHEMA_VALIDATOR" "$WORK/$name.schema.json" "$kind"
}

runtime_checks() {
  ( cd "$PROLOG_DIR" && swipl -q -l compile/test/plunit_tests.pl \
      -g "run_tests([anonymous_product_values, anonymous_sum_values])" -g halt )
  ( cd "$TSV2_DIR" && node --test --experimental-transform-types \
      tests/enumPlane.test.ts )
  ( cd "$V6_DIR/sprefa-engine-rs" && cargo test --quiet --lib enum_plane )
}

# @comment-ok: 8_emit_rust_types has no JSONL reader; the rows come out of
# typegen_export's reader (unexported, reached by module qualification), then
# rust_types_text/3 renders them, so one golden judges both rust doors.
render_rust_prolog() { # name -> writes $WORK/<name>.prolog.rs
  local name="$1"
  ( cd "$PROLOG_DIR" && swipl -q -l "$COMPILE_DIR/typegen_export.pl" \
      -l "$COMPILE_DIR/8_emit_rust_types.pl" \
      -g "typegen_export:read_row_lines('$WORK/$name.jsonl', Rows), emit_rust_types:rust_types_text(rows, Rows, Text), open('$WORK/$name.prolog.rs', write, Stream), format(Stream, '~s', [Text]), close(Stream)" \
      -g halt 2>/dev/null )
  [ -s "$WORK/$name.prolog.rs" ]
}

judge() { # name -> diffs the dl6 render and the prolog render against one golden
  local name="$1"
  local golden="$GOLDEN_DIR/$name.types.ts"
  if ! render_fixture "$name" "$RENDERER" "$name.types.ts"; then
    echo "FAIL  $name: render_ts.dl6 did not run on tsv2"
    FAILED=1
    return
  fi
  if ! diff -u "$golden" "$WORK/$name.types.ts" >"$WORK/$name.diff" 2>&1; then
    echo "FAIL  $name: dl6 rendered text differs from golden"
    cat "$WORK/$name.diff"
    FAILED=1
    return
  fi
  if ! render_prolog "$name"; then
    echo "FAIL  $name: write_prolog_types failed"
    FAILED=1
    return
  fi
  if ! diff -u "$golden" "$WORK/$name.prolog.ts" >"$WORK/$name.parity.diff" 2>&1; then
    echo "FAIL  $name: prolog emitter text differs from golden"
    cat "$WORK/$name.parity.diff"
    FAILED=1
    return
  fi
  if ! diff -u "$WORK/$name.types.ts" "$WORK/$name.prolog.ts" >"$WORK/$name.door.diff" 2>&1; then
    echo "FAIL  $name: direct Prolog and DL6 renderers differ"
    cat "$WORK/$name.door.diff"
    FAILED=1
    return
  fi
  echo "PASS  $name"
}

judge_rust() { # name -> diffs the rust dl6 render and the rust prolog render against one golden
  local name="$1"
  local golden="$GOLDEN_DIR/$name.types.rs"
  if ! render_fixture "$name" "$RUST_RENDERER" "$name.types.rs"; then
    echo "FAIL  $name: render_rust.dl6 did not run on tsv2"
    FAILED=1
    return
  fi
  if ! diff -u "$golden" "$WORK/$name.types.rs" >"$WORK/$name.rust.diff" 2>&1; then
    echo "FAIL  $name: rust dl6 rendered text differs from golden"
    cat "$WORK/$name.rust.diff"
    FAILED=1
    return
  fi
  if ! render_rust_prolog "$name"; then
    echo "FAIL  $name: rust_types_text failed"
    FAILED=1
    return
  fi
  if ! diff -u "$golden" "$WORK/$name.prolog.rs" >"$WORK/$name.rust.parity.diff" 2>&1; then
    echo "FAIL  $name: rust prolog emitter text differs from golden"
    cat "$WORK/$name.rust.parity.diff"
    FAILED=1
    return
  fi
  if ! diff -u -B "$WORK/$name.types.rs" "$WORK/$name.prolog.rs" >"$WORK/$name.rust.door.diff" 2>&1; then
    echo "FAIL  $name: direct Prolog and DL6 Rust renderers differ"
    cat "$WORK/$name.rust.door.diff"
    FAILED=1
    return
  fi
  echo "PASS  $name (rust)"
}

judge_source() { # name -> parity and target compilation for a real .dl6 source
  local name="$1" kind="plain"
  case "$name" in
    anonymous-type-syntax) kind="sum" ;;
    rust-associated-outputs) kind="product" ;;
  esac
  if ! render_fixture "$name" "$RENDERER" "$name.types.ts"; then
    echo "FAIL  $name: render_ts.dl6 did not run on tsv2"
    FAILED=1
    return
  fi
  if ! render_prolog "$name" || ! diff -u "$WORK/$name.types.ts" "$WORK/$name.prolog.ts" >"$WORK/$name.door.diff" 2>&1; then
    echo "FAIL  $name: direct Prolog and DL6 TS renderers differ"
    cat "$WORK/$name.door.diff" 2>/dev/null || true
    FAILED=1
    return
  fi
  if [ -f "$GOLDEN_DIR/$name.types.ts" ] &&
     ! diff -u "$GOLDEN_DIR/$name.types.ts" "$WORK/$name.types.ts" >"$WORK/$name.ts.diff" 2>&1; then
    echo "FAIL  $name: generated TypeScript differs from golden"
    cat "$WORK/$name.ts.diff"
    FAILED=1
    return
  fi
  if ! compile_ts "$name"; then
    echo "FAIL  $name: generated TypeScript did not compile"
    FAILED=1
    return
  fi
  if ! render_fixture "$name" "$RUST_RENDERER" "$name.types.rs"; then
    echo "FAIL  $name: render_rust.dl6 did not run on tsv2"
    FAILED=1
    return
  fi
  if ! render_rust_prolog "$name" || ! diff -u -B "$WORK/$name.types.rs" "$WORK/$name.prolog.rs" >"$WORK/$name.rust.door.diff" 2>&1; then
    echo "FAIL  $name: direct Prolog and DL6 Rust renderers differ"
    cat "$WORK/$name.rust.door.diff" 2>/dev/null || true
    FAILED=1
    return
  fi
  if [ -f "$GOLDEN_DIR/$name.types.rs" ] &&
     ! diff -u "$GOLDEN_DIR/$name.types.rs" "$WORK/$name.types.rs" >"$WORK/$name.rust.diff" 2>&1; then
    echo "FAIL  $name: generated Rust differs from golden"
    cat "$WORK/$name.rust.diff"
    FAILED=1
    return
  fi
  if ! compile_rust "$name"; then
    echo "FAIL  $name: generated Rust temporary crate did not test"
    FAILED=1
    return
  fi
  if ! render_prolog_schema "$name" || ! validate_schema "$name" "$kind"; then
    echo "FAIL  $name: generated JSON Schema did not validate"
    FAILED=1
    return
  fi
  if [ -f "$GOLDEN_DIR/$name.schema.json" ] &&
     ! diff -u "$GOLDEN_DIR/$name.schema.json" "$WORK/$name.schema.json" >"$WORK/$name.schema.diff" 2>&1; then
    echo "FAIL  $name: generated JSON Schema differs from golden"
    cat "$WORK/$name.schema.diff"
    FAILED=1
    return
  fi
  echo "PASS  $name (real dl6, TS/Rust/schema)"
}

main() {
  mkdir -p "$GOLDEN_DIR"
  for entry in "${PINNED[@]}"; do
    local name="${entry%%:*}"
    local fixture_file="${entry##*:}"
    if [[ "$fixture_file" = "$V6_DIR/dl/fixtures/"* ]]; then
      if ! swipl_run "typegen_export:dump_dl6_rows('$fixture_file', '$name', '$WORK/$name.jsonl')"; then
        echo "FAIL  $name: real dl6 parser/expansion/typegen export failed"
        FAILED=1
        continue
      fi
      judge_source "$name"
      continue
    fi
    if ! dump_rows "$name" "$fixture_file"; then
      echo "FAIL  $name: dump_type_rows failed"
      FAILED=1
      continue
    fi
    judge "$name"
    judge_rust "$name"
  done

  for name in ${SHAPES[@]+"${SHAPES[@]}"}; do
    cp "$GOLDEN_DIR/$name.type_rows.jsonl" "$WORK/$name.jsonl"
    judge "$name"
    judge_rust "$name"
  done

  if ! runtime_checks; then
    echo "FAIL  runtime product/sum checks"
    FAILED=1
  else
    echo "PASS  runtime product/sum checks (Prolog, TSV2, Rust)"
  fi

  if [ "$FAILED" = 1 ]; then
    echo "TYPEGEN GOLDEN: FAIL"
    exit 1
  fi
  echo "TYPEGEN GOLDEN: HOLDS"
}

main
