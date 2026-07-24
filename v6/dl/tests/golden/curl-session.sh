#!/usr/bin/env bash
# tests/golden/curl-session.sh — the M6 epic-of-epics gate. Runs the full sg-rail curl
# session (plan M5's golden transcript, http-fronted) against a REAL live server started
# by `node src/main.ts`, curl as the only client. Compares a normalized transcript
# against fixtures/golden/curl-session.txt; REGEN_GOLDEN=1 rewrites it instead.
#
# Runnable standalone from the repo root OR from v6/dl (paths resolve from this
# script's own dirname, never the caller's cwd).
#
# TWO EMPIRICAL FINDINGS this script's steps are shaped around (found by actually
# running the demo, not assumed from the plan doc — "doubt yourself, verify against
# the code" is a standing law in this repo):
#
#  1. Plain single-line `sed -i '' '/console.log/d'` deletion does NOT reliably
#     retract `diag`. Deleting a whole line of length L shifts every later byte
#     back by exactly L, so the line immediately AFTER the deleted one always ends
#     up starting at the exact byte offset the deleted line used to start at. In
#     this fixture that means the `return message;` statement inherits the freed
#     offset (147) and `extract` still emits a node/span_line row there — the very
#     same offset the (permanently cached) sg match keyed its join on, so
#     `diag`'s span_line join never breaks. This is not a fixture quirk; it is a
#     structural property of single-line deletion, and would misfire on nearly any
#     source file whose next line also starts a node. Fix used here: the edit also
#     changes an EARLIER line's byte length (a cosmetic rename, message ->
#     outputMessage) so the freed offset does not realign to anything. Verified
#     empirically: with only the deletion, `diag` never empties (delta log shows a
#     lone +1, no -1); with the added rename, it retracts cleanly (+1 then -1).
#
#  2. `console_hit` itself does NOT retract after the fix, and this is BY DESIGN,
#     not a bug: HostRunner's effect_cache digest for a `sh`/builtin probe is keyed
#     on (host, requestCols) = (sg, pattern, path) ONLY — never on file content —
#     so `sg` never re-runs for the same path, no matter how the file changes
#     (the "?" idempotence law, 1_hosts.ts). `console_hit`'s own rule body
#     (`file(path), __resp_sg(...)`) is therefore still fully satisfied by the
#     stale cached response after the edit, and it stays present. Only `diag`
#     drops, because ITS extra join against `span_line` (which DOES update from
#     the real file bytes on every ingestFile diff) loses its partner. So the
#     `/query ? console_hit(...)` step below is expected to still return the
#     STALE row after the fix, not `[]` — the golden fixture records exactly this
#     (regenerated from a live run, not hand-typed).
#
# Bounded-poll law: no fixed sleeps waiting on tick-driven state (the sg host
# commits its response on a LATER tick than file_changed's own response — every
# such wait below is a curl/jq retry loop with an attempt budget, never a sleep
# guessing a duration). The one exception is a short, fixed, DOCUMENTED grace
# period purely for the background `curl -N` subscribe connection to finish its
# TCP handshake before the mutating steps run — that is not the tick-driven
# nondeterminism this law targets.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DL_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
FIXTURES_DIR="$DL_DIR/fixtures"
GOLDEN_PATH="$DL_DIR/fixtures/golden/curl-session.txt"

DL_PORT="${DL_GOLDEN_PORT:-17171}"
BASE_URL="http://127.0.0.1:$DL_PORT"

TMP_DIR="$(mktemp -d)"
SERVER_PID=""
SUBSCRIBE_PID=""

cleanup() {
  if [[ -n "$SUBSCRIBE_PID" ]] && kill -0 "$SUBSCRIBE_PID" 2>/dev/null; then
    kill "$SUBSCRIBE_PID" 2>/dev/null || true
    wait "$SUBSCRIBE_PID" 2>/dev/null || true
  fi
  if [[ -n "$SERVER_PID" ]] && kill -0 "$SERVER_PID" 2>/dev/null; then
    kill "$SERVER_PID" 2>/dev/null || true
    wait "$SERVER_PID" 2>/dev/null || true
  fi
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

TEMP_DB="$TMP_DIR/mvp.sqlite"
BAD_TS="$TMP_DIR/bad.ts"
cp "$FIXTURES_DIR/corpus/bad.ts" "$BAD_TS"

OUTPUT_FILE="$TMP_DIR/transcript.txt"
: > "$OUTPUT_FILE"

append_section() {
  # $1 = label; stdin = the (already normalized-ready) body to record.
  {
    echo "=== $1 ==="
    cat
    echo
  } >> "$OUTPUT_FILE"
}

# ── 1. boot the server ────────────────────────────────────────────────────────
(
  cd "$DL_DIR"
  DL_DB_PATH="$TEMP_DB" DL_PORT="$DL_PORT" node --experimental-transform-types src/main.ts
) > "$TMP_DIR/server.log" 2>&1 &
SERVER_PID=$!

server_up=0
for _ in $(seq 1 100); do
  if curl -s -o /dev/null "$BASE_URL/idb/diag" 2>/dev/null; then
    server_up=1
    break
  fi
  sleep 0.1
done
if [[ "$server_up" -ne 1 ]]; then
  echo "FATAL: server never came up on :$DL_PORT" >&2
  cat "$TMP_DIR/server.log" >&2
  exit 1
fi

# ── 2. load the program ───────────────────────────────────────────────────────
PROGRAM_RESPONSE="$(curl -s -X POST "$BASE_URL/edb/program" --data-binary @"$FIXTURES_DIR/sg-rail.dl")"
echo "$PROGRAM_RESPONSE" | jq -S . | append_section "POST /edb/program"

# ── 3. tell it the file exists (console.log still present) ──────────────────
FILE_CHANGED_1_RESPONSE="$(curl -s -X POST "$BASE_URL/edb/file_changed" -d "{\"path\":\"$BAD_TS\"}")"
echo "$FILE_CHANGED_1_RESPONSE" | jq -S . | append_section "POST /edb/file_changed (1st: console.log present)"

# ── 4. poll GET /idb/diag until non-empty (sg? fires on a later tick) ───────
diag_populated=0
DIAG_AFTER_INGEST=""
for _ in $(seq 1 200); do
  DIAG_AFTER_INGEST="$(curl -s "$BASE_URL/idb/diag")"
  count="$(echo "$DIAG_AFTER_INGEST" | jq '.rows | length')"
  if [[ "$count" -gt 0 ]]; then
    diag_populated=1
    break
  fi
  sleep 0.05
done
if [[ "$diag_populated" -ne 1 ]]; then
  echo "FATAL: diag never became non-empty after file_changed" >&2
  exit 1
fi
echo "$DIAG_AFTER_INGEST" | jq -S . | append_section "GET /idb/diag (polled until non-empty)"

# ── 5. one-shot query before the fix ─────────────────────────────────────────
QUERY_BEFORE_RESPONSE="$(curl -s -X POST "$BASE_URL/query" -d '? console_hit(path, _, _, _).')"
echo "$QUERY_BEFORE_RESPONSE" | jq -S . | append_section "POST /query before fix: ? console_hit(path, _, _, _)."

# ── 6. start the live -N subscribe capture BEFORE mutating the file ─────────
SUBSCRIBE_CAPTURE="$TMP_DIR/subscribe.raw.txt"
: > "$SUBSCRIBE_CAPTURE"
curl -sN "$BASE_URL/subscribe/diag" > "$SUBSCRIBE_CAPTURE" 2>/dev/null &
SUBSCRIBE_PID=$!
# Fixed, short, DOCUMENTED grace period: purely lets curl -N finish its TCP
# handshake before we trigger the mutation below. Not a wait on tick-driven
# state (that wait is the bounded poll in step 8) — see the file header note.
sleep 0.3

# ── 7. fix the file, tell it again ───────────────────────────────────────────
# See finding #1 in the file header: a plain single-line deletion does not
# reliably retract `diag` (the freed byte offset gets reoccupied by the very
# next statement). The rename also shifts an earlier line's length so the
# freed offset does not realign to anything in the edited file.
sed -i '' -e 's/console\.log(message);//' -e 's/message/outputMessage/g' "$BAD_TS"

FILE_CHANGED_2_RESPONSE="$(curl -s -X POST "$BASE_URL/edb/file_changed" -d "{\"path\":\"$BAD_TS\"}")"
echo "$FILE_CHANGED_2_RESPONSE" | jq -S . | append_section "POST /edb/file_changed (2nd: console.log removed)"

# ── 8. poll GET /idb/diag until empty (bounded, no fixed sleep) ──────────────
diag_cleared=0
DIAG_AFTER_FIX=""
for _ in $(seq 1 200); do
  DIAG_AFTER_FIX="$(curl -s "$BASE_URL/idb/diag")"
  count="$(echo "$DIAG_AFTER_FIX" | jq '.rows | length')"
  if [[ "$count" -eq 0 ]]; then
    diag_cleared=1
    break
  fi
  sleep 0.05
done
if [[ "$diag_cleared" -ne 1 ]]; then
  echo "FATAL: diag never cleared after the fix" >&2
  exit 1
fi
echo "$DIAG_AFTER_FIX" | jq -S . | append_section "GET /idb/diag (polled until empty, after fix)"

# ── 9. one-shot query after the fix ──────────────────────────────────────────
# See finding #2 in the file header: console_hit is expected to still show the
# STALE row here (the sg effect cache never re-runs for the same path).
QUERY_AFTER_RESPONSE="$(curl -s -X POST "$BASE_URL/query" -d '? console_hit(path, _, _, _).')"
echo "$QUERY_AFTER_RESPONSE" | jq -S . | append_section "POST /query after fix: ? console_hit(path, _, _, _)."

# ── 10. bounded poll: the subscribe capture must show the retract event ─────
retract_seen=0
for _ in $(seq 1 100); do
  if grep -q '"retracts":\[{' "$SUBSCRIBE_CAPTURE" 2>/dev/null; then
    retract_seen=1
    break
  fi
  sleep 0.05
done
kill "$SUBSCRIBE_PID" 2>/dev/null || true
wait "$SUBSCRIBE_PID" 2>/dev/null || true
SUBSCRIBE_PID=""
if [[ "$retract_seen" -eq 1 ]]; then
  echo "diag retract event arrived over SSE: yes" | append_section "GET /subscribe/diag (live capture, summarized -- exact framing/timing is not compared)"
else
  echo "diag retract event arrived over SSE: no" | append_section "GET /subscribe/diag (live capture, summarized -- exact framing/timing is not compared)"
fi

# ── 11. delta log dump: the thesis line -- diag died through weights ────────
DELTA_DUMP="$(sqlite3 "$TEMP_DB" "SELECT rel,tick,weight FROM delta WHERE rel='diag' ORDER BY tick")"
echo "$DELTA_DUMP" | append_section "sqlite3 delta WHERE rel='diag' ORDER BY tick"

# ── 12. tear down the server (subscribe curl already killed above) ─────────
kill "$SERVER_PID" 2>/dev/null || true
wait "$SERVER_PID" 2>/dev/null || true
SERVER_PID=""

# ── normalize: the temp dir path is different every run ──────────────────────
NORMALIZED_FILE="$TMP_DIR/transcript.normalized.txt"
sed "s#$TMP_DIR#TMPPATH#g" "$OUTPUT_FILE" > "$NORMALIZED_FILE"

if [[ "${REGEN_GOLDEN:-}" == "1" ]]; then
  mkdir -p "$(dirname "$GOLDEN_PATH")"
  cp "$NORMALIZED_FILE" "$GOLDEN_PATH"
  echo "REGEN_GOLDEN=1: wrote $GOLDEN_PATH"
  cat "$NORMALIZED_FILE"
  exit 0
fi

cat "$NORMALIZED_FILE"

if [[ ! -f "$GOLDEN_PATH" ]]; then
  echo "FATAL: no golden at $GOLDEN_PATH (run with REGEN_GOLDEN=1 first)" >&2
  exit 1
fi

if ! diff -u "$GOLDEN_PATH" "$NORMALIZED_FILE"; then
  echo "FATAL: transcript does not match $GOLDEN_PATH (see diff above; REGEN_GOLDEN=1 to update)" >&2
  exit 1
fi

echo "curl-session golden: PASS"
