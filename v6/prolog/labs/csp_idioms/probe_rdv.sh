#!/usr/bin/env bash
# probe_rdv.sh : dump both doors' RAW tick logs for rendezvous, side by side.
# Authoring aid for the two-door divergence; not a receipt.
set -u
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../../../.." && pwd)"
TSV2="$REPO/v6/tsv2"
export SPREFA_CONFIG=/nonexistent/csp.toml DL_NO_DAEMON=1

read -r -d '' SHOW <<'PY' || true
import json, sys
for line in sys.stdin:
    line = line.strip()
    if line.startswith("data: "):
        line = line[6:]
    if not line:
        continue
    tick = json.loads(line)
    print(tick["tick"],
          "waiting_receivers=", tick["deltas"].get("waiting_receivers"),
          "met=", tick["deltas"].get("met"))
PY

echo "=== ORACLE ==="
( cd "$REPO/v6/prolog/compile/scripts" && swipl -q -l dl6_oracle.pl \
    -g "oracle('$HERE/rendezvous.dl6','$HERE/rendezvous.schedule.json')" -g halt 2>/dev/null ) \
  | python3 -c "$SHOW"

echo "=== SERVED ==="
scratch="$(mktemp -d)"
( cd "$TSV2" && node --experimental-transform-types cli/bop.ts \
    serve --port 0 --db ':memory:' >"$scratch/serve.log" 2>&1 ) &
pid=$!
port=""
for _ in $(seq 1 200); do
  port="$(sed -n 's/.*serving on \([0-9]*\).*/\1/p' "$scratch/serve.log" | head -1)"
  [ -n "$port" ] && break
  sleep 0.1
done
base="http://127.0.0.1:$port"
curl -s -X POST --data-binary @"$HERE/rendezvous.dl6" "$base/program" >/dev/null
( curl -sN "$base/ticks" >"$scratch/ticks" ) &
capture=$!
sleep 0.5
count="$(python3 -c 'import json,sys; print(len(json.load(open(sys.argv[1]))))' "$HERE/rendezvous.schedule.json")"
for (( i = 0; i < count; i++ )); do
  batch="$(python3 -c 'import json,sys; print(json.dumps(json.load(open(sys.argv[1]))[int(sys.argv[2])]))' "$HERE/rendezvous.schedule.json" "$i")"
  curl -s -X POST -d "{\"batch\":$batch}" "$base/arrivals" >/dev/null
  sleep 0.35
done
sleep 0.8
kill "$capture" 2>/dev/null; kill "$pid" 2>/dev/null; wait "$pid" 2>/dev/null
python3 -c "$SHOW" <"$scratch/ticks"
rm -rf "$scratch"
