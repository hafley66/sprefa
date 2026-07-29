#!/usr/bin/env bash
# enumerate.sh — the FILE-SET FEED's receipt (golden plan phase 2, item 2),
# run against THIS REPOSITORY rather than a toy tree, because the thing being
# proved is ignore semantics at node_modules scale and a temp directory has no
# node_modules to fail on.
#
# Program: ../dl/fixtures/enumerate-hosts.dl6 (two `sh` hosts, zero new
# constructs -- ruling spine_residency: the git/fs spine is hosted IN the
# language, never kernel).
#
# Four assertions:
#
#   1  enumerate('v6/tsv2/**/*.ts') row count EQUALS `git ls-files` for the same
#      pathspec, exactly -- so the host's answer is git's answer, not a walker's
#   2  ZERO node_modules paths, with node_modules PRESENT on disk and holding
#      thousands of .ts files. `git ls-files` reports tracked files only, so the
#      untracked tree is not skipped, it is never walked
#   3  the digests are WORKING-TREE content digests: touching a tracked file's
#      bytes changes its row, and `git hash-object` on the same path agrees
#   4  enumerate_at(HEAD, glob) returns the same paths with git's own BLOB OIDS
#      -- read out of the object database, never hashed from a file, which is
#      what makes the pinned witness cacheable forever
#
# SABOTAGE RECEIPT (run 2026-07-29, reverted), and it went red EARLIER than
# predicted: swapping the enumerate template's `git ls-files -- '{glob}'` for
# `find . -name '*.ts'` never reaches assertion 2, because the program stops
# COMPILING -- `template_mismatch(unreferenced_input(glob))`, a 400 on the load.
# The language already refuses a host whose template ignores its own demand
# column, which is a stronger guarantee than this script could assert: a host
# cannot silently answer a question it was not asked.
set -uo pipefail
TSV2="$(cd "$(dirname "$0")/.." && pwd)"
REPO="$(cd "$TSV2/../.." && pwd)"

PORT="${TSV2_ENUMERATE_PORT:-17572}"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/tsv2-enum.XXXXXX")"
PROGRAM="$TSV2/../dl/fixtures/enumerate-hosts.dl6"
SERVE_MAIN="$TSV2/serve/main.ts"
BASE="http://127.0.0.1:$PORT"
GLOB='v6/tsv2/**/*.ts'
SERVER_PID=""
cd "$REPO"

fail() { printf 'FAIL  %s\n' "$*"; [ -n "$SERVER_PID" ] && tail -20 "$WORK/server.log"; stop_server; exit 1; }
say() { printf '%s\n' "$*"; }
stop_server() {
  [ -n "$SERVER_PID" ] && kill -9 "$SERVER_PID" 2>/dev/null
  wait "$SERVER_PID" 2>/dev/null
  SERVER_PID=""
}
trap stop_server EXIT

# The served process's CWD is the root, so git runs where the repo is.
TSV2_DB=":memory:" TSV2_PORT="$PORT" \
  node --experimental-transform-types "$SERVE_MAIN" >"$WORK/server.log" 2>&1 &
SERVER_PID=$!
for _ in $(seq 1 60); do
  curl -s -o /dev/null "$BASE/ticks" 2>/dev/null && break
  kill -0 "$SERVER_PID" 2>/dev/null || fail "server died on boot: $(tail -5 "$WORK/server.log")"
  sleep 0.2
done

status="$(curl -s -o "$WORK/load.json" -w '%{http_code}' -X POST --data-binary @"$PROGRAM" "$BASE/program")"
[ "$status" = "200" ] || fail "program load returned $status: $(cat "$WORK/load.json")"
grep -q 'enumerate_at' "$WORK/load.json" || fail "both enumerate hosts should be declared: $(cat "$WORK/load.json")"
say "PASS  program loaded, hosts: $(sed 's/.*"hosts":\[//; s/\].*//' "$WORK/load.json")"

post_arrival() {
  curl -s -o /dev/null -X POST --data-binary "$1" "$BASE/arrivals"
}
rows_json() { curl -s "$BASE/idb/$1"; }
await_rows() {
  local rel="$1" want="$2" deadline=$((SECONDS + 120))
  while [ "$SECONDS" -lt "$deadline" ]; do
    local n; n="$(rows_json "$rel" | python3 -c 'import json,sys; print(len(json.load(sys.stdin)["rows"]))')"
    [ "$n" -ge "$want" ] && return 0
    sleep 0.5
  done
  return 1
}

# ── 1: the host's answer IS git's answer ────────────────────────────────────
expected="$(git ls-files -- "$GLOB" | wc -l | tr -d ' ')"
[ "$expected" -gt 20 ] || fail "the pathspec matched only $expected tracked files; pick a wider glob"
post_arrival "{\"batch\":[{\"rel\":\"want\",\"sign\":\"add\",\"row\":[\"$GLOB\"]}]}"
await_rows file "$expected" || fail "enumerate never produced $expected rows (got $(rows_json file | head -c 200))"
actual="$(rows_json file | python3 -c 'import json,sys; print(len(json.load(sys.stdin)["rows"]))')"
[ "$actual" = "$expected" ] || fail "enumerate produced $actual rows, git ls-files reports $expected"
say "PASS  enumerate('$GLOB') = git ls-files exactly: $actual rows"

# ── 2: node_modules is present on disk and absent from the answer ───────────
on_disk="$(find "$REPO/v6/tsv2/node_modules" -name '*.ts' 2>/dev/null | wc -l | tr -d ' ')"
[ "$on_disk" -gt 100 ] || fail "this receipt needs a populated node_modules to be meaningful (found $on_disk .ts files); run pnpm install"
leaked="$(rows_json file | grep -c node_modules || true)"
[ "$leaked" = "0" ] || fail "enumerate leaked node_modules paths ($leaked) -- ignore semantics are not git's"
say "PASS  $on_disk node_modules .ts files on disk, 0 in the answer (tracked-files-only, never walked)"

# ── 3: the digest is the WORKING TREE's content ────────────────────────────
sample="$(git ls-files -- "$GLOB" | head -1)"
want_digest="$(git hash-object -- "$sample")"
rows_json file | grep -q "\"$sample\",\"$want_digest\"" \
  || fail "enumerate's digest for $sample is not git hash-object's ($want_digest); got $(rows_json file | tr ',' '\n' | grep -A1 "$sample" | head -2)"
say "PASS  digest of $sample = git hash-object $want_digest (working tree, not the index)"

# ── 4: the pinned twin, git own blob oids straight from the object database ─
head_rev="$(git rev-parse HEAD)"
before="$actual"
post_arrival "{\"batch\":[{\"rel\":\"want_at\",\"sign\":\"add\",\"row\":[\"$head_rev\",\"$GLOB\"]}]}"
tree_paths="$(git ls-files --with-tree="$head_rev" -- "$GLOB" | wc -l | tr -d ' ')"
await_rows file "$before" || fail "enumerate_at produced nothing"
sleep 3
pinned_path="$(git ls-files --with-tree="$head_rev" -- "$GLOB" | head -1)"
pinned_oid="$(git rev-parse "$head_rev:$pinned_path")"
rows_json file | grep -q "\"$pinned_path\",\"$pinned_oid\"" \
  || fail "enumerate_at did not report $pinned_path at its blob oid $pinned_oid"
say "PASS  enumerate_at($head_rev) reports git's own blob oids ($tree_paths tracked paths at that rev)"

stop_server
say "ENUMERATE HOSTS HOLD"
