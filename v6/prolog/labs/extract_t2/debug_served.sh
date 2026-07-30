#!/usr/bin/env bash
# scratch: run one program on the served door with the server log visible.
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../../../.." && pwd)"
TSV2="$REPO/v6/tsv2"
export SPREFA_CONFIG=/nonexistent/extract-t2.toml
export DL_NO_DAEMON=1

program="$1"; schedule="$2"
scratch="$(mktemp -d)"
( cd "$TSV2" && node --experimental-transform-types cli/bop.ts serve --port 0 --db ":memory:" >"$scratch/serve.log" 2>&1 ) &
pid=$!
port=""
for _ in $(seq 1 300); do
  port="$(sed -n 's/.*serving on \([0-9]*\).*/\1/p' "$scratch/serve.log" | head -1)"
  [ -n "$port" ] && break
  sleep 0.1
done
echo "PORT=$port"
base="http://127.0.0.1:$port"
echo "--- load ---"
curl -s -X POST --data-binary @"$program" "$base/program"; echo
( curl -sN "$base/ticks" >"$scratch/ticks" ) & capture=$!
sleep 0.5
count="$(python3 -c 'import json,sys; print(len(json.load(open(sys.argv[1]))))' "$schedule")"
for (( i = 0; i < count; i++ )); do
  batch="$(python3 -c 'import json,sys; print(json.dumps(json.load(open(sys.argv[1]))[int(sys.argv[2])]))' "$schedule" "$i")"
  echo "--- arrivals batch $i ---"
  curl -s -X POST -d "{\"batch\":$batch}" "$base/arrivals"; echo
  sleep 0.5
done
sleep 1
kill "$capture" 2>/dev/null; kill "$pid" 2>/dev/null; wait "$pid" 2>/dev/null
echo "--- ticks ---"; cat "$scratch/ticks"
echo "--- serve.log ---"; tail -30 "$scratch/serve.log"
rm -rf "$scratch"
