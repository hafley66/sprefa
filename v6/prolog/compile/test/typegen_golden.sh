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
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
COMPILE_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
PROLOG_DIR="$(cd "$COMPILE_DIR/.." && pwd)"
V6_DIR="$(cd "$PROLOG_DIR/.." && pwd)"
TSV2_DIR="$V6_DIR/tsv2"
RENDERER="$V6_DIR/dl/typegen/render_ts.dl6"
RUST_RENDERER="$V6_DIR/dl/typegen/render_rust.dl6"
GOLDEN_DIR="$SCRIPT_DIR/typegen_golden"

WORK="$(mktemp -d "${TMPDIR:-/tmp}/typegen-golden.XXXXXX")"
FAILED=0

# name:fixture_file  (fixture files live in v6/prolog/conformance/fixtures)
PINNED=(
  "generic_expansion_end_to_end:0_generic_expand.pl"
  "nested_list_of_text_round_trips:10_list_elements.pl"
  "list_of_json_documents_round_trips:10_list_elements.pl"
  "split_initcap_and_fold_render_pascal_case:15_string_split.pl"
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
  local port
  port="$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1]); s.close()')"
  local base="http://127.0.0.1:$port"
  local server_pid=""

  (
    cd "$V6_DIR"
    TSV2_DB=":memory:" TSV2_PORT="$port" NODE_NO_WARNINGS=1 \
      node --experimental-transform-types "$TSV2_DIR/serve/main.ts"
  ) >"$WORK/$name.server.log" 2>&1 &
  server_pid=$!

  local ready=0
  for _ in $(seq 1 100); do
    if curl -s -o /dev/null --max-time 1 "$base/stats" 2>/dev/null; then ready=1; break; fi
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
    if curl -s -o /dev/null -X POST --data-binary @"$renderer" "$base/program"; then
      if curl -s -o /dev/null -X POST --data-binary @"$WORK/$name.arrivals.json" "$base/edb/events"; then
        curl -s "$base/idb/rendered_type" >"$WORK/$name.rendered.json"
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

  kill -9 "$server_pid" 2>/dev/null
  wait "$server_pid" 2>/dev/null
  [ "$ok" = 1 ]
}

render_prolog() { # name -> writes $WORK/<name>.prolog.ts from the same JSONL
  local name="$1"
  swipl_run "write_prolog_types('$WORK/$name.jsonl', '$WORK/$name.prolog.ts')"
  [ -s "$WORK/$name.prolog.ts" ]
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
  echo "PASS  $name (rust)"
}

main() {
  mkdir -p "$GOLDEN_DIR"
  for entry in "${PINNED[@]}"; do
    local name="${entry%%:*}"
    local fixture_file="${entry##*:}"
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

  if [ "$FAILED" = 1 ]; then
    echo "TYPEGEN GOLDEN: FAIL"
    exit 1
  fi
  echo "TYPEGEN GOLDEN: HOLDS"
}

main
