#!/usr/bin/env bash
# prose-nothing run: boot the tsv2 engine over the dl6 program and report the
# per-class per-side counts. Mode `fixture` feeds the inline probe sentences;
# mode `corpus` (default) walks the full ~/.claude transcript set.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LAB_DIR="$(cd "$SCRIPT_DIR" && pwd)"
TSV2_DIR="$(cd "$LAB_DIR/../../tsv2" && pwd)"
V6_DIR="$(cd "$LAB_DIR/../.." && pwd)"
REPO_ROOT="$(git -C "$V6_DIR" rev-parse --show-toplevel 2>/dev/null || pwd)"

MODE="${1:-corpus}"
PROGRAM="$LAB_DIR/prose-nothing.dl6"
IDLE_MS="${PROSE_NOTHING_IDLE_MS:-700}"

export PROSE_NOTHING_FEED="$LAB_DIR/feed-sentences.sh"

if [ "$MODE" = "fixture" ]; then
  export PROSE_NOTHING_MODE="fixture"
  FIXTURE_FILE="$LAB_DIR/fixture-sentences.json"
  if [ ! -f "$FIXTURE_FILE" ]; then
    printf 'run.sh: fixture mode needs v6/labs/prose_nothing/fixture-sentences.json\n' >&2
    exit 1
  fi
  export PROSE_NOTHING_FIXTURE_FILE="$FIXTURE_FILE"
else
  export PROSE_NOTHING_MODE="feed"
fi

WORK_DIR="$(mktemp -d "${TMPDIR:-/tmp}/prose-nothing.XXXXXX")"
SERVER_PID=""
TICKS_PID=""
cleanup() {
  [ -n "$TICKS_PID" ] && kill -9 "$TICKS_PID" 2>/dev/null
  [ -n "$SERVER_PID" ] && kill -9 "$SERVER_PID" 2>/dev/null
  wait 2>/dev/null
  rm -rf -- "$WORK_DIR"
}
trap cleanup EXIT

PORT="${PROSE_NOTHING_PORT:-0}"
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

TOKEN="$(git -C "$V6_DIR" rev-parse HEAD 2>/dev/null || printf 'prose-nothing')"
if [ "$MODE" = "fixture" ]; then
  CHUNKS=1
  TOTAL_SENTENCES=6
else
  CHUNK_SIZE="${PROSE_NOTHING_CHUNK_SIZE:-5000}"
  TOTAL_SENTENCES="$(PROSE_NOTHING_FEED="$LAB_DIR/feed-sentences.sh" PROSE_NOTHING_MODE=feed \
    "$LAB_DIR/feed-sentences.sh" feed all | wc -l | tr -d ' ')"
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

curl -s "$BASE/idb/evaluative_no_receipt_count" >"$WORK_DIR/eval.json"
curl -s "$BASE/idb/strawman_contrast_count" >"$WORK_DIR/straw.json"
curl -s "$BASE/idb/pure_nothing_count" >"$WORK_DIR/nothing.json"
curl -s "$BASE/idb/sentence_side_count" >"$WORK_DIR/total.json"
curl -s "$BASE/idb/pure_nothing_sentence" >"$WORK_DIR/pure.json"
END_MS="$(python3 -c 'import time; print(int(time.time()*1000))')"

python3 - "$WORK_DIR" "$MODE" "$((END_MS - START_MS))" <<'PY'
import json, os, sys
work, mode, elapsed = sys.argv[1], sys.argv[2], int(sys.argv[3])

def load(name):
    with open(os.path.join(work, name)) as f:
        return json.load(f).get("rows", [])

def dcount(rows):
    return dict(rows) if rows and len(rows[0]) == 2 else {}

total_rows = load("total.json")
side_totals = dcount(total_rows)
all_sides = ["assistant", "user"]

def show(label, rows):
    table = dcount(rows)
    print(f"{label}:")
    running_tot = 0
    for side in all_sides:
        n = table.get(side, 0)
        running_tot += n
        base = side_totals.get(side, 0)
        rate = (n * 10000.0) / base if base else 0.0
        print(f"  {side:<10} {n:>8}   rate {rate:.2f} per 10k")
    print(f"  {'TOTAL':<10} {running_tot:>8}")

print(f"== PROSE-NOTHING (mode={mode}) ==")
print(f"run time: {elapsed} ms")
print(f"sentence sides: {side_totals}")
print()
show("CLASS evaluative_no_receipt", load("eval.json"))
print()
show("CLASS strawman_contrast", load("straw.json"))
print()
show("CLASS pure_nothing", load("nothing.json"))
print()

if mode == "corpus":
    pure = load("pure.json")
    assistant = sorted([r for r in pure if r[0] == "assistant"], key=lambda r: r[1])
    print("TOP 15 assistant pure_nothing offenders (seq):")
    for row in assistant[:15]:
        print(f"  seq {row[1]:>6}  {row[2]}")
PY

exit 0
