#!/usr/bin/env bash
# prose-laws run: boot the tsv2 engine over the dl6 program and report the
# per-rule violation counts. Mode `fixture` feeds the inline probe sentences;
# mode `corpus` (default) walks the full ~/.claude transcript set.
#
# Output is the violation(rule_id, seq, sentence) relation aggregated by
# rule_id. The receipt for the fixture is the exact per-rule count: one hit per
# ruled sentence plus a clean sentence contributing zero.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LAB_DIR="$(cd "$SCRIPT_DIR" && pwd)"
TSV2_DIR="$(cd "$LAB_DIR/../../tsv2" && pwd)"
V6_DIR="$(cd "$LAB_DIR/../.." && pwd)"
REPO_ROOT="$(git -C "$V6_DIR" rev-parse --show-toplevel 2>/dev/null || pwd)"

MODE="${1:-corpus}"
PROGRAM="$LAB_DIR/prose-laws.dl6"
IDLE_MS="${PROSE_LAWS_IDLE_MS:-700}"

export PROSE_LAWS_FEED="$LAB_DIR/feed-sentences.sh"

if [ "$MODE" = "fixture" ]; then
  export PROSE_LAWS_MODE="fixture"
  FIXTURE_FILE="$LAB_DIR/fixture-sentences.json"
  if [ ! -f "$FIXTURE_FILE" ]; then
    printf 'run.sh: fixture mode needs v6/labs/prose_laws/fixture-sentences.json\n' >&2
    exit 1
  fi
  export PROSE_LAWS_FIXTURE_FILE="$FIXTURE_FILE"
else
  export PROSE_LAWS_MODE="feed"
fi

WORK_DIR="$(mktemp -d "${TMPDIR:-/tmp}/prose-laws.XXXXXX")"
SERVER_PID=""
TICKS_PID=""
cleanup() {
  [ -n "$TICKS_PID" ] && kill -9 "$TICKS_PID" 2>/dev/null
  [ -n "$SERVER_PID" ] && kill -9 "$SERVER_PID" 2>/dev/null
  wait 2>/dev/null
  rm -rf -- "$WORK_DIR"
}
trap cleanup EXIT

PORT="${PROSE_LAWS_PORT:-0}"
if [ "$PORT" = 0 ]; then
  PORT="$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1]); s.close()')"
fi
BASE="http://127.0.0.1:$PORT"

START_MS="$(python3 -c 'import time; print(int(time.time()*1000))')"

(
  cd "$REPO_ROOT"
  TSV2_DB=":memory:" TSV2_PORT="$PORT" NODE_NO_WARNINGS=1 \
    node --experimental-transform-types "$TSV2_DIR/serve/main.ts"
) >"$WORK_DIR/server.log" 2>&1 &
SERVER_PID=$!

ready=0
for _ in $(seq 1 200); do
  if curl -s -o /dev/null --max-time 1 "$BASE/stats" 2>/dev/null; then ready=1; break; fi
  kill -0 "$SERVER_PID" 2>/dev/null || break
  sleep 0.05
done
if [ "$ready" != 1 ]; then
  printf 'run.sh: server did not start\n' >&2
  tail -20 "$WORK_DIR/server.log" >&2
  exit 1
fi

curl -sN "$BASE/ticks" >"$WORK_DIR/ticks.sse" 2>/dev/null &
TICKS_PID=$!

status="$(curl -s -o "$WORK_DIR/load.json" -w '%{http_code}' -X POST --data-binary @"$PROGRAM" "$BASE/program")"
if [ "$status" != 200 ]; then
  printf 'run.sh: program load returned %s\n' >&2
  cat "$WORK_DIR/load.json" >&2
  exit 1
fi

TOKEN="$(git -C "$V6_DIR" rev-parse HEAD 2>/dev/null || printf 'prose-laws')"

# Sentence total comes from the feed itself in both modes, so the fixture
# receipt is computed, never hardcoded. The feed sub-mode is feed or fixture
# (already set in PROSE_LAWS_MODE above), never the outer corpus label.
SUBMODE="${PROSE_LAWS_MODE:-feed}"
TOTAL_SENTENCES="$(PROSE_LAWS_FEED="$LAB_DIR/feed-sentences.sh" \
  "$LAB_DIR/feed-sentences.sh" "$SUBMODE" all 2>/dev/null | wc -l | tr -d ' ')"
if [ "$MODE" = "fixture" ]; then
  CHUNKS=1
else
  CHUNK_SIZE="${PROSE_LAWS_CHUNK_SIZE:-5000}"
  CHUNKS=$(( (TOTAL_SENTENCES + CHUNK_SIZE - 1) / CHUNK_SIZE ))
fi

python3 - "$WORK_DIR" "$CHUNKS" "$TOKEN" <<'PY'
import json, sys
work, chunks, token = sys.argv[1], int(sys.argv[2]), sys.argv[3]
rows = [{"rel":"probe_row","sign":"add","row":[str(i)]} for i in range(chunks)]
with open(sys.argv[1] + "/arrivals.json", "w") as f:
    json.dump({"batch": rows}, f)
print(f"posting {chunks} probe tokens")
PY

status="$(curl -s -o "$WORK_DIR/arrivals.out" -w '%{http_code}' -X POST --data-binary @"$WORK_DIR/arrivals.json" "$BASE/arrivals")"
if [ "$status" != 200 ]; then
  printf 'run.sh: arrivals returned %s\n' >&2
  cat "$WORK_DIR/arrivals.out" >&2
  exit 1
fi

# The chunked feed hosts each walk the transcripts and can take seconds, so
# idle-based quiescence fires too early. Wait until the accumulated sentence
# count reaches the expected corpus total, then settle the last drain tick.
last_size=-1
idle_slept=0
step_ms=50
deadline=$((SECONDS + 900))
while [ "$SECONDS" -lt "$deadline" ]; do
  totals="$(curl -s "$BASE/idb/sentence_side_count" 2>/dev/null)"
  loaded=$(python3 - "$WORK_DIR" "$totals" <<'PY'
import json, sys
try:
    rows = json.loads(sys.argv[2]).get("rows", [])
except Exception:
    rows = []
print(sum(int(x[1]) for x in rows if len(x) == 2))
PY
)
  [ "$loaded" -ge "$TOTAL_SENTENCES" ] && break
  size="$(wc -c <"$WORK_DIR/ticks.sse" 2>/dev/null | tr -d ' ')"
  if [ "$size" = "$last_size" ]; then
    idle_slept=$((idle_slept + step_ms))
    [ "$idle_slept" -ge "$IDLE_MS" ] && idle_slept=0
  else
    idle_slept=0
    last_size="$size"
  fi
  sleep 0.05
done
[ "$loaded" -ge "$TOTAL_SENTENCES" ] || printf 'run.sh: warning: loaded %s of %s sentences in time\n' "$loaded" "$TOTAL_SENTENCES" >&2
# Settle any final drain tick from the last chunk arrival.
idle_slept=0
while [ "$idle_slept" -lt "$IDLE_MS" ]; do
  size="$(wc -c <"$WORK_DIR/ticks.sse" 2>/dev/null | tr -d ' ')"
  if [ "$size" = "$last_size" ]; then
    idle_slept=$((idle_slept + step_ms))
  else
    idle_slept=0
    last_size="$size"
  fi
  sleep 0.05
done

curl -s "$BASE/idb/violation" >"$WORK_DIR/violation.json"
curl -s "$BASE/idb/sentence_side_count" >"$WORK_DIR/total.json"
END_MS="$(python3 -c 'import time; print(int(time.time()*1000))')"

python3 - "$WORK_DIR" "$MODE" "$((END_MS - START_MS))" "$TOTAL_SENTENCES" <<'PY'
import json, os, sys
work, mode, elapsed, total = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4])

def load(name):
    with open(os.path.join(work, name)) as f:
        return json.load(f).get("rows", [])

def dcount(rows):
    return dict(rows) if rows and len(rows[0]) == 2 else {}

side_totals = dcount(load("total.json"))
all_sides = ["assistant", "user"]

viol = load("violation.json")
by_rule = {}
for rule_id, seq, sentence in viol:
    by_rule.setdefault(rule_id, []).append((seq, sentence))

rule_order = [
    "em-dash", "neg-parallelism", "deictic-filler",
    "one-word-sentence", "banned-stem", "nothing-sentence",
]

print(f"== PROSE-LAWS (mode={mode}) ==")
print(f"run time: {elapsed} ms")
print(f"sentences: {total} {side_totals}")
print()
for rule_id in rule_order:
    hits = sorted(by_rule.get(rule_id, []), key=lambda r: r[0])
    print(f"{rule_id}: {len(hits)}")
    for seq, sentence in hits:
        print(f"    seq {seq}: {sentence}")
print()
unknown = [r for r in by_rule if r not in rule_order]
if unknown:
    print(f"unexpected rule ids: {unknown}")
PY

exit 0
