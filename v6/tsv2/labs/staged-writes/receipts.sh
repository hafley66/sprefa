#!/usr/bin/env bash
# receipts.sh -- STAGED WRITES LAB. Everything runs through the real served
# tsv2 engine (v6/tsv2/serve/main.ts), the real compiler (compile_dl6.sh) and
# the real `sh` host path. Nothing is stubbed, nothing is simulated.
#
# Hermetic by construction: SPREFA_CONFIG points at a nonexistent file,
# DL_NO_DAEMON=1, an ephemeral port, a scratch db and a scratch corpus under
# $TMPDIR. NOTHING outside $WORK is ever written -- which matters more here
# than in any other lab, because the subject IS writing files.
#
# Phases
#   0  five .dl6 programs compile
#   1  the staged diff exists as ROWS and the tree is byte-unchanged
#   2  an unarmed program applies nothing; one `armed` row applies it
#   3  N lines = N spawns = N whole-file rewrites (the payload-is-a-relation cost)
#   4  the write does NOT repeat when the disk is reverted behind the engine
#      (content-addressed effect identity is blind to the world it wrote to)
#   5  the tick advances while an effect is in flight (measured, twice)
#   6  kill -9 between the disk write and the answer => the write REPLAYS
#   7  byte-span addressed writing works; span-TYPED host outputs
#   8  a host column named `ordinal` is silently shadowed (found here)
#
# SABOTAGE RECEIPT (run 2026-07-30, reverted): deleting the two file-writing
# lines from `zone.py cmd_put`, so it still answers `{"wrote": 1}` and still
# counts its spawn, turns phase 2 red with exactly
#   FAIL  phase 2: armed program did not change the file
# and nothing else moves. That is the receipt that phase 2 grades THE DISK and
# not the host's answer, which is the whole point of a write lab: a host that
# lies about having written is indistinguishable, at the row level, from one
# that wrote.
set -uo pipefail

LAB="$(cd "$(dirname "$0")" && pwd)"
TSV2="$(cd "$LAB/../.." && pwd)"
ROOT="$(cd "$TSV2/../.." && pwd)"
COMPILE="$ROOT/v6/prolog/compile/scripts/compile_dl6.sh"
SERVE_MAIN="$TSV2/serve/main.ts"

WORK="$(mktemp -d "${TMPDIR:-/tmp}/staged-writes.XXXXXX")"
CORPUS="$WORK/corpus"
mkdir -p "$CORPUS"

export SPREFA_CONFIG=/nonexistent/x.toml
export DL_NO_DAEMON=1

FAILED=0
SERVER_PID=""
say() { printf '%s\n' "$*"; }
fail() { printf 'FAIL  %s\n' "$*"; FAILED=1; }
ok() { printf 'ok    %s\n' "$*"; }

stop_server() {
  if [ -n "$SERVER_PID" ]; then
    kill -9 "$SERVER_PID" 2>/dev/null
    wait "$SERVER_PID" 2>/dev/null
    SERVER_PID=""
  fi
}
cleanup() { stop_server; }
trap cleanup EXIT

free_port() { python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1]);s.close()'; }

# The host command spelled in every .dl6 is `python3 "$LAB_ZONE" ...`. LAB_ZONE
# resolves to a SHIM so the receipts can count spawns without putting counters
# in the program text -- the same split extraction-live.sh makes for the
# extractor binary.
MARKS="$WORK/spawns"
: >"$MARKS"
# The shim is PYTHON, not shell: every .dl6 template spells
# `python3 "$LAB_ZONE"`, so whatever LAB_ZONE names is read by python3.
cat >"$WORK/zone-shim.py" <<SHIM
import runpy, sys
with open("$MARKS", "a") as handle:
    handle.write((sys.argv[1] if len(sys.argv) > 1 else "?") + "\n")
runpy.run_path("$LAB/zone.py", run_name="__main__")
SHIM
export LAB_ZONE="$WORK/zone-shim.py"

marks_of() { local n; n="$(grep -c "^$1\$" "$MARKS" 2>/dev/null)"; printf '%s' "${n:-0}"; }

start_server() {
  local db="$1"
  PORT="$(free_port)"
  BASE="http://127.0.0.1:$PORT"
  TSV2_DB="$db" TSV2_PORT="$PORT" \
    LAB_ZONE="$LAB_ZONE" LAB_TARGET="${LAB_TARGET:-}" LAB_SLEEP="${LAB_SLEEP:-0}" \
    node --experimental-transform-types "$SERVE_MAIN" >>"$WORK/server.log" 2>&1 &
  SERVER_PID=$!
  local tries=0
  while [ "$tries" -lt 100 ]; do
    if [ "$(curl -s -o /dev/null -w '%{http_code}' "$BASE/stats" 2>/dev/null)" != "000" ]; then return 0; fi
    kill -0 "$SERVER_PID" 2>/dev/null || { fail "server died on boot: $(tail -5 "$WORK/server.log")"; return 1; }
    sleep 0.1
    tries=$((tries + 1))
  done
  fail "server never listened on $PORT"
  return 1
}

load() { curl -s -o "$WORK/load.out" -w '%{http_code}' --data-binary "@$1" "$BASE/program"; }
post() { curl -s -X POST -H 'content-type: application/json' -d "$1" "$BASE/edb/events"; }
rows_of() { curl -s "$BASE/idb/$1" | tr -d "\n"; }
row_count() { rows_of "$1" | python3 -c 'import json,sys; d=sys.stdin.read(); print(len(json.loads(d).get("rows",[])) if d.strip() else -1)'; }
digest_of() { python3 -c 'import hashlib,sys;print(hashlib.sha256(open(sys.argv[1],"rb").read()).hexdigest()[:16])' "$1"; }
now_ms() { python3 -c 'import time;print(int(time.time()*1000))'; }

await_rows() {  # rel expected-count timeout-seconds
  local deadline=$(( $(date +%s) + $3 ))
  while [ "$(date +%s)" -lt "$deadline" ]; do
    [ "$(row_count "$1")" = "$2" ] && return 0
    sleep 0.2
  done
  return 1
}

# ── the corpus ──────────────────────────────────────────────────────────────
write_corpus() {
  cat >"$CORPUS/lib.rs" <<'RS'
// a hand-written line that must survive every regeneration
// BEGIN: fnlist
// stale
// END: fnlist

pub fn alpha() {}
pub fn beta() {}
fn gamma() {}
RS
}
write_corpus
FILE_ROW='{"batch":[{"rel":"file","sign":"add","row":["'"$CORPUS"'/lib.rs","d1"]}]}'

say "=== staged-writes lab ==="
say "work: $WORK"

# ── phase 0: the programs compile ───────────────────────────────────────────
for f in 1-stage 2-apply 3-backpressure 4-crash 5-span 6-ordinal; do
  if bash "$COMPILE" "$LAB/$f.dl6" "$WORK/$f.ts" >"$WORK/$f.compile.log" 2>&1; then
    ok "phase 0: $f.dl6 compiles"
  else
    fail "phase 0: $f.dl6 did not compile: $(tail -2 "$WORK/$f.compile.log")"
  fi
done

# ── phase 1: staged diff as rows, tree untouched ────────────────────────────
start_server ":memory:" || exit 1
[ "$(load "$LAB/1-stage.dl6")" = "200" ] || fail "phase 1: /program refused 1-stage.dl6: $(cat "$WORK/load.out")"
before="$(digest_of "$CORPUS/lib.rs")"
post "$FILE_ROW" >/dev/null
await_rows edit_add 3 20 || fail "phase 1: edit_add never reached 3 rows (got $(row_count edit_add))"
add_rows="$(rows_of edit_add)"
del_rows="$(rows_of edit_del)"
after="$(digest_of "$CORPUS/lib.rs")"
[ "$before" = "$after" ] && ok "phase 1: tree byte-unchanged ($before)" || fail "phase 1: the read-only program wrote to disk"
case "$add_rows" in *alpha*beta*gamma*) ok "phase 1: edit_add stages 3 lines: $add_rows" ;; *) fail "phase 1: edit_add is wrong: $add_rows" ;; esac
case "$del_rows" in *stale*) ok "phase 1: edit_del stages the stale line: $del_rows" ;; *) fail "phase 1: edit_del is wrong: $del_rows" ;; esac
stop_server

# ── phase 2 + 3: the second demand applies it; count the spawns ─────────────
write_corpus
: >"$MARKS"
start_server ":memory:" || exit 1
[ "$(load "$LAB/2-apply.dl6")" = "200" ] || fail "phase 2: /program refused 2-apply.dl6: $(cat "$WORK/load.out")"
post "$FILE_ROW" >/dev/null
await_rows edit_add 3 20 || fail "phase 2: edit_add never reached 3 rows"
unarmed="$(digest_of "$CORPUS/lib.rs")"
[ "$(digest_of "$CORPUS/lib.rs")" = "$(digest_of "$CORPUS/lib.rs")" ]
puts_before="$(marks_of put)"
[ "$puts_before" = "0" ] && ok "phase 2: unarmed program spawned zero write commands" || fail "phase 2: unarmed program already wrote ($puts_before put spawns)"
post '{"batch":[{"rel":"armed","sign":"add","row":["fnlist"]}]}' >/dev/null
await_rows applied 3 20 || fail "phase 2: applied never reached 3 rows (got $(row_count applied))"
armed_digest="$(digest_of "$CORPUS/lib.rs")"
if [ "$unarmed" != "$armed_digest" ]; then ok "phase 2: one armed row rewrote the zone ($unarmed -> $armed_digest)"; else fail "phase 2: armed program did not change the file"; fi
grep -q 'hand-written line that must survive' "$CORPUS/lib.rs" && ok "phase 2: hand-written text outside the markers survived" || fail "phase 2: text outside the zone was destroyed"
grep -q '// BEGIN: fnlist' "$CORPUS/lib.rs" && grep -q '// END: fnlist' "$CORPUS/lib.rs" && ok "phase 2: both markers survived" || fail "phase 2: a marker was eaten"
puts="$(marks_of put)"
if [ "$puts" = "3" ]; then
  ok "phase 3: 3 staged lines = $puts spawns = 3 whole-file rewrites (one per row; a host input is a row, a payload is a relation)"
else
  fail "phase 3: expected 3 put spawns, got $puts"
fi
say "      zone after apply:"; sed -n '/BEGIN: fnlist/,/END: fnlist/p' "$CORPUS/lib.rs" | sed 's/^/        /'

# ── phase 4: revert the disk behind the engine; the write does not repeat ───
write_corpus
reverted="$(digest_of "$CORPUS/lib.rs")"
puts_before_retry="$(marks_of put)"
post '{"batch":[{"rel":"armed","sign":"del","row":["fnlist"]}]}' >/dev/null
post '{"batch":[{"rel":"armed","sign":"add","row":["fnlist"]}]}' >/dev/null
sleep 2
puts_after_retry="$(marks_of put)"
now="$(digest_of "$CORPUS/lib.rs")"
if [ "$puts_before_retry" = "$puts_after_retry" ] && [ "$reverted" = "$now" ]; then
  ok "phase 4: demand retracted and re-asserted, ZERO respawns, file stays reverted (content-addressed identity is blind to the disk it wrote)"
else
  fail "phase 4: expected no respawn (put $puts_before_retry -> $puts_after_retry, digest $reverted -> $now)"
fi
stop_server

# ── phase 5: the tick advances while an effect is in flight ─────────────────
start_server ":memory:" || exit 1
[ "$(load "$LAB/3-backpressure.dl6")" = "200" ] || fail "phase 5: /program refused 3-backpressure.dl6"
t0="$(now_ms)"
reply_a="$(post '{"batch":[{"rel":"job","sign":"add","row":["a","3"]}]}')"
t1="$(now_ms)"
reply_b="$(post '{"batch":[{"rel":"job","sign":"add","row":["b","3"]}]}')"
t2="$(now_ms)"
tick_a="$(printf '%s' "$reply_a" | python3 -c 'import json,sys;d=json.load(sys.stdin);print(d["ticks"][-1]["tick"])')"
tick_b="$(printf '%s' "$reply_b" | python3 -c 'import json,sys;d=json.load(sys.stdin);print(d["ticks"][-1]["tick"])')"
elapsed=$((t2 - t0))
if [ "$elapsed" -lt 1500 ] && [ "$tick_b" -gt "$tick_a" ]; then
  ok "phase 5: ticks $tick_a then $tick_b returned in ${elapsed}ms with a 3s effect outstanding -- THE TICK DOES NOT WAIT"
else
  fail "phase 5: expected two ticks well under the 3s effect (ticks $tick_a/$tick_b in ${elapsed}ms)"
fi
await_rows answered 2 30 || fail "phase 5: the effects never answered"
t3="$(now_ms)"
say "      answers landed at +$((t3 - t0))ms; the two POSTs had already returned at +${elapsed}ms"
stop_server

# ── phase 6: kill -9 between the disk write and the answer ──────────────────
export LAB_TARGET="$WORK/appended.txt"
: >"$LAB_TARGET"
CRASH_DB="file:$WORK/crash.sqlite"
LAB_SLEEP=6 start_server "$CRASH_DB" || exit 1
[ "$(load "$LAB/4-crash.dl6")" = "200" ] || fail "phase 6: /program refused 4-crash.dl6"
post '{"batch":[{"rel":"want_append","sign":"add","row":["w1","one true line"]}]}' >/dev/null
deadline=$(( $(date +%s) + 20 ))
while [ "$(date +%s)" -lt "$deadline" ] && [ "$(wc -l <"$LAB_TARGET" | tr -d ' ')" = "0" ]; do sleep 0.2; done
lines_before_kill="$(wc -l <"$LAB_TARGET" | tr -d ' ')"
[ "$lines_before_kill" = "1" ] && ok "phase 6: the disk write is COMMITTED ($lines_before_kill line) while the answer is still in flight" || fail "phase 6: the write never landed before the kill (got $lines_before_kill lines)"
stop_server
answered_after_kill="$(row_count appended 2>/dev/null || echo unreachable)"
say "      killed the process mid-effect; restarting on the same file db"
LAB_SLEEP=0 start_server "$CRASH_DB" || exit 1
[ "$(load "$LAB/4-crash.dl6")" = "200" ] || fail "phase 6: /program refused 4-crash.dl6 on restart"
deadline=$(( $(date +%s) + 20 ))
while [ "$(date +%s)" -lt "$deadline" ] && [ "$(wc -l <"$LAB_TARGET" | tr -d ' ')" = "1" ]; do sleep 0.2; done
lines_after="$(wc -l <"$LAB_TARGET" | tr -d ' ')"
if [ "$lines_after" = "2" ]; then
  ok "phase 6: the write REPLAYED on restart ($lines_before_kill -> $lines_after lines). A write on the durable-witness story is AT-LEAST-ONCE."
else
  fail "phase 6: expected the write to replay to 2 lines, got $lines_after"
fi
stop_server
unset LAB_TARGET

# ── phase 7: byte-span addressed writing ────────────────────────────────────
write_corpus
: >"$MARKS"
start_server ":memory:" || exit 1
[ "$(load "$LAB/5-span.dl6")" = "200" ] || fail "phase 7: /program refused 5-span.dl6"
post "$FILE_ROW" >/dev/null
await_rows zone_span 1 20 || fail "phase 7: zone_span never landed"
span_row="$(rows_of zone_span)"
ok "phase 7: byte span reached the program as flat ints: $span_row"
post '{"batch":[{"rel":"armed","sign":"add","row":["fnlist"]}]}' >/dev/null
await_rows spliced 1 20 || fail "phase 7: spliced never landed"
if grep -q '// written by byte span' "$CORPUS/lib.rs" && grep -q 'hand-written line that must survive' "$CORPUS/lib.rs"; then
  ok "phase 7: the byte range was replaced and the surrounding text survived"
else
  fail "phase 7: span splice did not land correctly"
fi
grep -q '// BEGIN: fnlist' "$CORPUS/lib.rs" && ok "phase 7: markers survived a span-addressed write" || fail "phase 7: span write ate a marker"
stop_server

# span-TYPED host outputs: the named refusal, straight from the compiler
if bash "$COMPILE" "$ROOT/v6/dl/fixtures/flagship-flow.dl6" "$WORK/flagship.ts" >"$WORK/flagship.log" 2>&1; then
  if grep -q 'unsupportedExecution: readonly string\[\] = \[\]' "$WORK/flagship.ts"; then
    ok "phase 7: span-TYPED host outputs now COMPILE (flagship-flow.dl6, zero unsupportedExecution; the span column emits as the struct-plane INTEGER id). The named stop host_struct_output_type in that fixture's own header is STALE."
  else
    say "      NOTE flagship-flow.dl6 compiled but still carries refusals"
  fi
else
  refusal="$(grep -o 'column_type_wrapper([^)]*)' "$WORK/flagship.log" | head -1)"
  if [ -n "$refusal" ]; then
    ok "phase 7: span-TYPED host outputs are still refused: $refusal"
  else
    say "      NOTE flagship-flow.dl6 refused for another reason: $(tail -1 "$WORK/flagship.log")"
  fi
fi

# ── phase 8: the `ordinal` collision this lab found ─────────────────────────
start_server ":memory:" || exit 1
[ "$(load "$LAB/6-ordinal.dl6")" = "200" ] || fail "phase 8: /program refused 6-ordinal.dl6"
post '{"batch":[{"rel":"seed","sign":"add","row":["s1"]}]}' >/dev/null
sleep 3
got_rows="$(rows_of got)"
resp_rows="$(rows_of __host_response_two_rows)"
ddl="$(grep -o 'CREATE TABLE "__host_response_two_rows"[^\`]*' "$WORK/6-ordinal.ts" | head -1)"
if [ "$(row_count got)" = "0" ]; then
  ok "phase 8: the host answered ordinal 7 and 8 and the program derives NOTHING: got=$got_rows"
else
  fail "phase 8: expected the collision to kill the join; got=$got_rows"
fi
case "$resp_rows" in
  *'["",""'*) ok "phase 8: the response row's witness and ordinal are EMPTY: $resp_rows" ;;
  *)          fail "phase 8: expected an empty witness in the response row: $resp_rows" ;;
esac
case "$ddl" in
  *'"col1"'*) ok "phase 8: the compiler renamed its own runtime columns to col1/col2 to dodge the duplicate name, while serve/1_hosts.ts still fills BY LITERAL NAME (\"witness_digest\"/\"ordinal\"). Two halves, one silence." ;;
  *)          fail "phase 8: expected col1/col2 in the emitted DDL: $ddl" ;;
esac
say "      emitted: $ddl"
say "      compare: $(grep -o 'CREATE TABLE \"__host_response_slow\"[^\`]*' "$WORK/3-backpressure.ts" | head -1)"
stop_server

say ""
if [ "$FAILED" = "0" ]; then
  say "STAGED WRITES LAB HOLDS"
  exit 0
fi
say "STAGED WRITES LAB FAILED"
exit 1
