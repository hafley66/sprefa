#!/usr/bin/env bash
# run.sh -- the rxoracle entry point. `bash run.sh` exits 0 when every case
# matched its declared expectation, nonzero otherwise. `bash run.sh -v` also
# prints each case's two normalized line files and the measured millisecond
# offset of every leg-B event. `bash run.sh <case> [<case> ...]` runs a subset.
#
# The contract, the line format and every normalization live in README.md next
# to this file. What lives HERE is only the mechanism:
#
#   leg A   node runs cases/<name>/leg-a.ts. That file imports rxjs and node
#           builtins and nothing else; it prints shared-format lines on stdout.
#   leg B   this script boots `bop serve` on an EPHEMERAL port (--port 0, port
#           read back off the process's own first stdout line), POSTs the .dl6
#           program, opens `GET /ticks` as an SSE capture through
#           lib/stamp.py, then POSTs one arrival batch per step with a
#           stepMs sleep between them. lib/lines.py turns the capture into
#           shared-format lines. curl is the only client. Nothing imports this
#           codebase from TypeScript.
#
# HERMETIC: SPREFA_CONFIG points at a path that does not exist, DL_NO_DAEMON=1,
# the db is :memory:, the port is ephemeral, and every scratch file lives under
# one mktemp -d that is removed on exit. No daemon is contacted and nothing
# under ~/.local/state is read or written.

set -uo pipefail

RXO_DIR="$(cd "$(dirname "$0")" && pwd)"
TSV2_DIR="$(cd "$RXO_DIR/.." && pwd)"
NODE_RUN=(node --experimental-transform-types)
VERBOSE=0

WORK="$(mktemp -d "${TMPDIR:-/tmp}/rxoracle.XXXXXX")"
SERVER_PID=""
CAPTURE_PID=""

cleanup() {
  [ -n "$CAPTURE_PID" ] && kill -9 "$CAPTURE_PID" 2>/dev/null
  [ -n "$SERVER_PID" ] && kill -9 "$SERVER_PID" 2>/dev/null
  wait 2>/dev/null
  rm -rf "$WORK"
}
trap cleanup EXIT

say() { printf '%s\n' "$*"; }
loud() { printf '\n== %s ==\n' "$*"; }

# ── argument parsing ─────────────────────────────────────────────────────────
SELECTED=()
for argument in "$@"; do
  case "$argument" in
    -v|--verbose) VERBOSE=1 ;;
    -*) say "unknown flag $argument"; exit 2 ;;
    *) SELECTED+=("$argument") ;;
  esac
done
if [ "${#SELECTED[@]}" -eq 0 ]; then
  while IFS= read -r directory; do SELECTED+=("$(basename "$directory")"); done \
    < <(find "$RXO_DIR/cases" -mindepth 1 -maxdepth 1 -type d | sort)
fi

command -v jq >/dev/null || { say "jq is required"; exit 2; }
command -v curl >/dev/null || { say "curl is required"; exit 2; }
command -v python3 >/dev/null || { say "python3 is required"; exit 2; }

# ── leg B: one served process, driven by curl ────────────────────────────────
# Writes the normalized leg-B line file to $2. Returns nonzero on any failure
# of the mechanism itself (server would not boot, program refused, guard fired).
run_leg_b() {
  local case_dir="$1" out_file="$2" scratch="$3"
  local manifest="$case_dir/case.json"
  local step_ms guard_ms drop_del show_internal
  step_ms="$(jq -r '.stepMs // 500' "$manifest")"
  guard_ms="$(jq -r '.guardMs // 100' "$manifest")"
  drop_del="$(jq -r 'if .dropDel == true then "yes" else "no" end' "$manifest")"
  show_internal="$(jq -r '(.showInternal // []) | join(",")' "$manifest")"

  # Host templates read the environment of the SERVED process, so the case's
  # own env goes on before the server starts, plus RXO_MARKS which every host
  # in this corpus appends to (the spawn ledger the cancel-inner case reads).
  local -a env_pairs=()
  while IFS= read -r pair; do [ -n "$pair" ] && env_pairs+=("$pair"); done \
    < <(jq -r '(.env // {}) | to_entries[] | "\(.key)=\(.value)"' "$manifest")
  : >"$scratch/marks"

  ( cd "$TSV2_DIR" && env SPREFA_CONFIG=/nonexistent/rxoracle.toml DL_NO_DAEMON=1 \
      RXO_MARKS="$scratch/marks" "${env_pairs[@]}" \
      "${NODE_RUN[@]}" cli/bop.ts serve --port 0 --db ":memory:" ) \
    >"$scratch/serve.log" 2>&1 &
  SERVER_PID=$!

  local attempt port=""
  for attempt in $(seq 1 150); do
    port="$(sed -n 's/.*serving on \([0-9]*\).*/\1/p' "$scratch/serve.log" | head -1)"
    [ -n "$port" ] && break
    sleep 0.1
  done
  [ -n "$port" ] || { say "  leg B: server never listened"; sed -n 1,20p "$scratch/serve.log"; return 1; }
  local base="http://127.0.0.1:$port"

  local load_body
  load_body="$(curl -s -X POST --data-binary @"$case_dir/leg-b.dl6" "$base/program")"
  case "$load_body" in
    *'"loaded":true'*) : ;;
    *) say "  leg B: program not loaded: $load_body"; return 1 ;;
  esac

  ( curl -sN "$base/ticks" | python3 "$RXO_DIR/lib/stamp.py" >"$scratch/capture" ) &
  CAPTURE_PID=$!
  disown "$CAPTURE_PID" 2>/dev/null
  sleep 0.4

  # The step-0 POST is the step-0 MIDPOINT (README section 2), so the clock
  # origin is that moment minus stepMs/2 and a synchronous tick right after the
  # POST lands mid-step instead of on a boundary. Taken HERE, after the SSE
  # connection is up, because that settle is not part of any step.
  local t0_ms
  t0_ms="$(python3 -c 'import time; print(int(round(time.time()*1000)) - '"$step_ms"'//2)')"

  local step_count index batch label
  step_count="$(jq -r '.steps | length' "$manifest")"
  for (( index = 0; index < step_count; index++ )); do
    batch="$(jq -c ".steps[$index].batch // []" "$manifest")"
    label="$(jq -r ".steps[$index].label // \"step $index\"" "$manifest")"
    if [ "$batch" != "[]" ]; then
      curl -s -X POST -d "{\"batch\":$batch}" "$base/arrivals" >/dev/null \
        || { say "  leg B: arrivals POST failed at step $index ($label)"; return 1; }
    fi
    python3 -c "import time; time.sleep($step_ms/1000.0)"
  done

  # RECEIPTS, taken while the server is still up. These answer questions the
  # line diff structurally cannot: the diff grades relations a program declared,
  # and "did the superseded effect's process still run to completion" and "is
  # the dead inner's answer stored anywhere" live in a spawn ledger and in a
  # compiler-minted relation whose digest columns have no rxjs counterpart to be
  # compared against (README section 3, N4). So they are asserted here, in bash,
  # against the running engine, instead of being forced through a normalization.
  local want_marks want_rel want_rows got_rows
  want_marks="$(jq -r '.receipts.marksLines // ""' "$manifest")"
  if [ -n "$want_marks" ]; then
    local got_marks; got_marks="$(grep -c . "$scratch/marks" 2>/dev/null || echo 0)"
    [ "$got_marks" = "$want_marks" ] \
      || { say "  receipt: expected $want_marks host spawn marks, got $got_marks:"; sed 's/^/    /' "$scratch/marks"; return 1; }
    say "  receipt: host spawn ledger has $got_marks lines as declared"
    sed 's/^/    marks: /' "$scratch/marks"
  fi
  while IFS= read -r pair; do
    [ -n "$pair" ] || continue
    want_rel="${pair%%=*}"; want_rows="${pair##*=}"
    got_rows="$(curl -s "$base/idb/$want_rel" | jq -r '.rows | length')"
    [ "$got_rows" = "$want_rows" ] \
      || { say "  receipt: expected $want_rows rows in $want_rel, got $got_rows"; return 1; }
    say "  receipt: $want_rel holds $got_rows row(s) as declared"
  done < <(jq -r '(.receipts.idbRows // {}) | to_entries[] | "\(.key)=\(.value)"' "$manifest")

  kill -9 "$CAPTURE_PID" 2>/dev/null; CAPTURE_PID=""
  kill -9 "$SERVER_PID" 2>/dev/null; SERVER_PID=""
  wait 2>/dev/null

  # The SSE capture is the only event source, so a capture that missed the
  # first tick is a broken run, never a divergence. Say so rather than diffing.
  grep -q '"tick":1' "$scratch/capture" \
    || { say "  leg B: SSE capture missed tick 1 (connection raced the first batch)"; return 1; }

  local -a line_flags=(--step-ms "$step_ms" --guard-ms "$guard_ms" --t0-ms "$t0_ms"
                       --offsets-file "$scratch/offsets")
  [ "$drop_del" = "yes" ] && line_flags+=(--drop-del)
  [ -n "$show_internal" ] && line_flags+=(--show-internal "$show_internal")
  python3 "$RXO_DIR/lib/lines.py" "${line_flags[@]}" <"$scratch/capture" | sort >"$out_file" || return 1
  return 0
}

# ── one case ─────────────────────────────────────────────────────────────────
FAILURES=0
declare -a TABLE=()

for case_name in "${SELECTED[@]}"; do
  case_dir="$RXO_DIR/cases/$case_name"
  manifest="$case_dir/case.json"
  [ -f "$manifest" ] || { say "no such case: $case_name"; FAILURES=$((FAILURES + 1)); continue; }
  scratch="$WORK/$case_name"; mkdir -p "$scratch"
  expect="$(jq -r '.expect' "$manifest")"
  loud "$case_name  (expect $expect)"
  say "$(jq -r '.summary // ""' "$manifest")"

  # leg A always runs, even for an inexpressible case: what rxjs does is the
  # thing the missing construct would have to reproduce, so it belongs in the
  # receipt either way.
  if ! ( cd "$case_dir" && "${NODE_RUN[@]}" leg-a.ts ) 2>"$scratch/a.err" | sort >"$scratch/a.lines"; then
    say "  leg A failed:"; sed -n 1,20p "$scratch/a.err"; FAILURES=$((FAILURES + 1))
    TABLE+=("$case_name|BROKEN|leg A did not run"); continue
  fi
  if [ -s "$scratch/a.err" ]; then
    grep -v 'ExperimentalWarning\|trace-warnings' "$scratch/a.err" >"$scratch/a.err2" || true
    if [ -s "$scratch/a.err2" ]; then
      say "  leg A reported:"; sed -n 1,20p "$scratch/a.err2"; FAILURES=$((FAILURES + 1))
      TABLE+=("$case_name|BROKEN|leg A guard or error"); continue
    fi
  fi

  if [ "$expect" = "inexpressible" ]; then
    # No leg B run: the point of the case is that the program does not compile.
    # `bop check` is the measurement, and its exit code plus its named refusal
    # are the receipt.
    want_code="$(jq -r '.refusal.exit // 2' "$manifest")"
    want_text="$(jq -r '.refusal.contains' "$manifest")"
    ( cd "$TSV2_DIR" && env SPREFA_CONFIG=/nonexistent/rxoracle.toml DL_NO_DAEMON=1 \
        "${NODE_RUN[@]}" cli/bop.ts check "$case_dir/leg-b.dl6" ) >"$scratch/check.out" 2>&1
    got_code=$?
    if [ "$got_code" != "$want_code" ] || ! grep -q -- "$want_text" "$scratch/check.out"; then
      say "  expected bop check to exit $want_code naming '$want_text'; got exit $got_code:"
      sed -n 1,25p "$scratch/check.out"
      FAILURES=$((FAILURES + 1)); TABLE+=("$case_name|BROKEN|refusal not reproduced"); continue
    fi
    say "  bop check exit $got_code, named refusal '$want_text' -- the construct does not exist"
    say "  rxjs leg, for the record ($(wc -l <"$scratch/a.lines" | tr -d ' ') lines):"
    sed 's/^/    /' "$scratch/a.lines"
    TABLE+=("$case_name|INEXPRESSIBLE|$want_text")
    continue
  fi

  if ! run_leg_b "$case_dir" "$scratch/b.lines" "$scratch"; then
    FAILURES=$((FAILURES + 1)); TABLE+=("$case_name|BROKEN|leg B did not run"); continue
  fi

  applied=""
  [ "$(jq -r 'if .dropDel == true then 1 else 0 end' "$manifest")" = "1" ] && applied="N3"
  if [ "$(jq -r '(.showInternal // []) | length' "$manifest")" != "0" ]; then
    applied="${applied:+$applied,}N4-exception"
  fi

  if [ "$VERBOSE" = "1" ]; then
    say "  leg-B measured step offsets (margin over guard must stay positive):"
    sed 's/^/    /' "$scratch/offsets"
    say "  leg A lines:"; sed 's/^/    /' "$scratch/a.lines"
    say "  leg B lines:"; sed 's/^/    /' "$scratch/b.lines"
  fi

  python3 "$RXO_DIR/lib/report.py" --case "$case_name" --leg-a "$scratch/a.lines" \
    --leg-b "$scratch/b.lines" --expect "$expect" --applied "$applied" >"$scratch/report.txt"
  status=$?
  sed 's/^/  /' "$scratch/report.txt"
  if [ "$status" != "0" ]; then FAILURES=$((FAILURES + 1)); fi

  verdict="$(sed -n 's/^VERDICT  *//p' "$scratch/report.txt")"
  note="$(jq -r '.note // ""' "$manifest")"
  TABLE+=("$case_name|$(tr '[:lower:]' '[:upper:]' <<<"$verdict")${applied:+ (${applied})}|$note")
done

loud "rxoracle table"
printf '%-34s %-28s %s\n' "case" "verdict" "note"
printf '%-34s %-28s %s\n' "----" "-------" "----"
for row in "${TABLE[@]}"; do
  IFS='|' read -r name verdict note <<<"$row"
  printf '%-34s %-28s %s\n' "$name" "$verdict" "$note"
done

if [ "$FAILURES" != "0" ]; then
  printf '\nRXORACLE RED: %d case(s) did not match their declared expectation\n' "$FAILURES"
  exit 1
fi
printf '\nRXORACLE HOLDS: %d case(s), every one as declared\n' "${#TABLE[@]}"
exit 0
