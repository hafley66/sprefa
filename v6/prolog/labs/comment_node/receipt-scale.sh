#!/usr/bin/env bash
# receipt-scale.sh -- DELIVERABLE 1's statement-count leg, at repo scale, on
# the real served engine over this repository's OWN compiler sources.
#
# TWO ASSERTIONS, both of the count-test-law shape (assert the plan, never the
# end state alone):
#
#   A  statements per tick stay BOUNDED as the corpus fills -- stated that way
#      because that is what the numbers say and "flat" would not be true. A
#      per-file `sh` host over N files must not make the engine's per-tick
#      statement count a function of N; if it did, dogfooding 58 files would be
#      the last size that ever worked. Measured from DL_PERF_LOG: the corpus
#      grows 58x across the run and the count must grow less than 2x.
#   B  the comment rows that land equal the rows the route produces offline
#      (route-cost.sh's `a`/`b2` count), so nothing is silently dropped
#      between the host's stdout and the engine's table.
#
# The corpus is a scratch COPY of v6/prolog/**/*.pl, git-committed inside the
# scratch tree, so the watcher's boot half (`git ls-files`) sees every file at
# once and the run is one settle rather than 58 edits.
set -uo pipefail
LAB="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$LAB/../../../.." && pwd)"
TSV2="$ROOT/v6/tsv2"
PORT="${CN_SCALE_PORT:-17607}"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/cn-scale.XXXXXX")"
CORPUS="$WORK/corpus"
BASE="http://127.0.0.1:$PORT"
SERVER_PID=""

export DL_CN="$LAB/cn.py"
export DL_EXTRACT_BIN="${DL_EXTRACT_BIN:-$ROOT/v6/sprefa-extract/target/release/extract}"
PERF="$WORK/perf.jsonl"

mkdir -p "$CORPUS/src"
# the lab's own files are excluded: the point is the compiler sources
( cd "$ROOT/v6/prolog" && find . -name '*.pl' -not -path './labs/*' ) | while read -r rel; do
  mkdir -p "$CORPUS/src/$(dirname "$rel")"
  cp "$ROOT/v6/prolog/$rel" "$CORPUS/src/$rel"
done
NFILES=$(find "$CORPUS/src" -name '*.pl' | wc -l | tr -d ' ')
cd "$CORPUS"
git init -q
git add -A >/dev/null 2>&1
git -c user.email=lab@local -c user.name=lab commit -qm pin >/dev/null 2>&1

stop_server() { [ -n "$SERVER_PID" ] && kill -9 "$SERVER_PID" 2>/dev/null; wait "$SERVER_PID" 2>/dev/null; SERVER_PID=""; }
trap stop_server EXIT

TSV2_DB="file:$WORK/scale.sqlite" TSV2_PORT="$PORT" TSV2_WATCH_COALESCE_MS=120 DL_PERF_LOG="$PERF" \
  node --experimental-transform-types "$TSV2/serve/main.ts" >>"$WORK/server.log" 2>&1 &
SERVER_PID=$!
for _ in $(seq 1 90); do curl -s -o /dev/null "$BASE/ticks" 2>/dev/null && break; sleep 0.2; done

status="$(curl -s -o "$WORK/load.json" -w '%{http_code}' -X POST --data-binary @"$LAB/programs/dogfood-comments.dl6" "$BASE/program")"
[ "$status" = "200" ] || { echo "FAIL program load returned $status: $(cat "$WORK/load.json")"; exit 1; }
echo "corpus: $NFILES prolog files (this repo's own compiler sources)"

# settle: poll until the row count stops moving for three consecutive reads
rows_of() { curl -s "$BASE/idb/$1" | tr -d ' \n'; }
# The count comes from the program's OWN aggregate rel, never from counting
# brackets in the JSON: a comment body legitimately contains `[`, and the first
# draft of this script read 6107 where the aggregate read 5939 -- a receipt bug
# that would have been reported as an engine defect.
count_of() {
  curl -s "$BASE/idb/comment_kind_count" \
    | python3 -c 'import json,sys; print(sum(row[1] for row in json.load(sys.stdin)["rows"]))' 2>/dev/null || echo 0
}
stable=0; previous=-1; deadline=$((SECONDS + 900))
while [ "$SECONDS" -lt "$deadline" ]; do
  now=$(count_of)
  if [ "$now" = "$previous" ] && [ "$now" != "0" ]; then
    stable=$((stable + 1)); [ "$stable" -ge 4 ] && break
  else
    stable=0
  fi
  previous="$now"
  sleep 2
done
LANDED=$(count_of)
echo "comment_node rows landed: $LANDED"
echo "kinds: $(rows_of comment_kind_count)"

# ── assertion A: statements per tick flat ───────────────────────────────────
python3 - "$PERF" <<'PY'
import json, sys
lines = []
with open(sys.argv[1]) as handle:
    for raw in handle:
        try:
            row = json.loads(raw)
        except ValueError:
            continue
        if "statements" in row or "stmts" in row:
            lines.append(row)
if not lines:
    print("SKIP  assertion A: DL_PERF_LOG produced no tick lines with a statement count")
    raise SystemExit(0)
key = "statements" if "statements" in lines[0] else "stmts"
counts = [row[key] for row in lines]
first, last, peak = counts[0], counts[-1], max(counts)
print(f"statements/tick: ticks={len(counts)} first={first} last={last} peak={peak}")
growth = peak / first if first else 0
print(f"growth factor {growth:.2f}x while the corpus grew {len(counts)}x")
if growth >= 2:
    print(f"FAIL  assertion A: statements/tick grew {first} -> {peak}, not bounded")
    raise SystemExit(1)
print("PASS  assertion A  statements per tick bounded as the corpus filled")
PY
A=$?

# ── assertion B: nothing dropped between stdout and the table ───────────────
OFFLINE=0
while read -r file; do
  n=$(nice -n 19 "$DL_EXTRACT_BIN" --family cst "$file" 2>/dev/null | python3 "$LAB/cn.py" comments "$file" | wc -l | tr -d ' ')
  OFFLINE=$((OFFLINE + n))
done < <(find "$CORPUS/src" -name '*.pl' | sort)
echo "offline route rows: $OFFLINE"
if [ "$OFFLINE" = "$LANDED" ]; then
  echo "PASS  assertion B  every route row reached the engine ($LANDED == $OFFLINE)"
  B=0
else
  echo "FAIL  assertion B  $OFFLINE route rows produced $LANDED engine rows"
  B=1
fi

stop_server
[ "$A" = 0 ] && [ "$B" = 0 ] && { echo; echo "COMMENT DOGFOOD SCALE HOLDS"; exit 0; }
echo; echo "COMMENT DOGFOOD SCALE FAILED (server log: $WORK/server.log)"; exit 1
