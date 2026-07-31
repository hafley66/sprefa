#!/usr/bin/env bash
# files.sh — the FILE-SET FEED's receipt (golden plan phase 2, item 2),
# run against THIS REPOSITORY rather than a toy tree, because the thing being
# proved is ignore semantics at node_modules scale and a temp directory has no
# node_modules to fail on.
#
# Program: ../dl/fixtures/files-hosts.dl6 (two `sh` hosts, zero new
# constructs -- ruling spine_residency: the git/fs spine is hosted IN the
# language, never kernel).
#
# Four assertions:
#
#   1  files('v6/tsv2/**/*.ts') row count EQUALS `git ls-files` for the same
#      pathspec, exactly -- so the host's answer is git's answer, not a walker's
#   2  ZERO node_modules paths, with node_modules PRESENT on disk and holding
#      thousands of .ts files. `git ls-files` reports tracked files only, so the
#      untracked tree is not skipped, it is never walked
#   3  the digests are WORKING-TREE content digests: touching a tracked file's
#      bytes changes its row, and `git hash-object` on the same path agrees
#   4  files_at(HEAD, glob) returns the same paths with git's own BLOB OIDS
#      -- read out of the object database, never hashed from a file, which is
#      what makes the pinned witness cacheable forever
#
# SABOTAGE RECEIPT for the repo-scoped legs (run 2026-07-31, reverted):
# dropping `-C '{repo}'` from repo_files's `ls-files` while leaving it on the
# per-file `hash-object` -- the half-edit a hurried author actually makes --
#
#   FAIL  repo_files produced 606 rows for a two-file repository;
#         git -C is not routing
#
# 606 = this repository's 285 tracked .ts paths under both want_repo clauses
# plus the pinned rows, every one of them carrying the OTHER repository's name
# in its repo column and an EMPTY digest (hash-object in the scratch repo cannot
# see a path that is not in it). Both halves of the sabotage are visible in one
# assertion, which is why the count is asserted against the two-file corpus
# rather than against "more than zero".
#
# SABOTAGE RECEIPT (run 2026-07-29, reverted), and it went red EARLIER than
# predicted: swapping the files template's `git ls-files -- '{glob}'` for
# `find . -name '*.ts'` never reaches assertion 2, because the program stops
# COMPILING -- `template_mismatch(unreferenced_input(glob))`, a 400 on the load.
# The language already refuses a host whose template ignores its own demand
# column, which is a stronger guarantee than this script could assert: a host
# cannot silently answer a question it was not asked.
# BUDGET (timeout-gun lane, 2026-07-31). Measured wall: 9s. Default 300s is
# ~33x that. Whole-script cap, because the cost is a backgrounded node server
# plus the `git ls-files` subprocesses it spawns as hosts and an HTTP poll
# loop; each individual curl also carries FILES_HTTP_BUDGET_S so a poll loop's
# own attempt counter stays meaningful (a request that never returns freezes
# the counter, which is how a bounded loop becomes an unbounded wait).
# Override with FILES_BUDGET_S.
set -uo pipefail
TSV2="$(cd "$(dirname "$0")/.." && pwd)"
REPO="$(cd "$TSV2/../.." && pwd)"

. "$TSV2/../tools/run-capped.sh"
cap_self "${FILES_BUDGET_S:-300}" files "$@"

PORT="${TSV2_FILES_PORT:-17572}"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/tsv2-enum.XXXXXX")"
PROGRAM="$TSV2/../dl/fixtures/files-hosts.dl6"
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
  capped_curl "${FILES_HTTP_BUDGET_S:-30}" -s -o /dev/null "$BASE/ticks" 2>/dev/null && break
  kill -0 "$SERVER_PID" 2>/dev/null || fail "server died on boot: $(tail -5 "$WORK/server.log")"
  sleep 0.2
done

status="$(capped_curl "${FILES_HTTP_BUDGET_S:-30}" -s -o "$WORK/load.json" -w '%{http_code}' -X POST --data-binary @"$PROGRAM" "$BASE/program")"
[ "$status" = "200" ] || fail "program load returned $status: $(cat "$WORK/load.json")"
grep -q 'files_at' "$WORK/load.json" || fail "both files hosts should be declared: $(cat "$WORK/load.json")"
say "PASS  program loaded, hosts: $(sed 's/.*"hosts":\[//; s/\].*//' "$WORK/load.json")"

post_arrival() {
  capped_curl "${FILES_HTTP_BUDGET_S:-30}" -s -o /dev/null -X POST --data-binary "$1" "$BASE/arrivals"
}
rows_json() { capped_curl "${FILES_HTTP_BUDGET_S:-30}" -s "$BASE/idb/$1"; }
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
await_rows file "$expected" || fail "files never produced $expected rows (got $(rows_json file | head -c 200))"
actual="$(rows_json file | python3 -c 'import json,sys; print(len(json.load(sys.stdin)["rows"]))')"
[ "$actual" = "$expected" ] || fail "files produced $actual rows, git ls-files reports $expected"
say "PASS  files('$GLOB') = git ls-files exactly: $actual rows"

# ── 2: node_modules is present on disk and absent from the answer ───────────
on_disk="$(find "$REPO/v6/tsv2/node_modules" -name '*.ts' 2>/dev/null | wc -l | tr -d ' ')"
[ "$on_disk" -gt 100 ] || fail "this receipt needs a populated node_modules to be meaningful (found $on_disk .ts files); run pnpm install"
leaked="$(rows_json file | grep -c node_modules || true)"
[ "$leaked" = "0" ] || fail "files leaked node_modules paths ($leaked) -- ignore semantics are not git's"
say "PASS  $on_disk node_modules .ts files on disk, 0 in the answer (tracked-files-only, never walked)"

# ── 3: the digest is the WORKING TREE's content ────────────────────────────
sample="$(git ls-files -- "$GLOB" | head -1)"
want_digest="$(git hash-object -- "$sample")"
rows_json file | grep -q "\"$sample\",\"$want_digest\"" \
  || fail "files's digest for $sample is not git hash-object's ($want_digest); got $(rows_json file | tr ',' '\n' | grep -A1 "$sample" | head -2)"
say "PASS  digest of $sample = git hash-object $want_digest (working tree, not the index)"

# ── 4: the pinned twin, git own blob oids straight from the object database ─
head_rev="$(git rev-parse HEAD)"
before="$actual"
post_arrival "{\"batch\":[{\"rel\":\"want_at\",\"sign\":\"add\",\"row\":[\"$head_rev\",\"$GLOB\"]}]}"
tree_paths="$(git ls-files --with-tree="$head_rev" -- "$GLOB" | wc -l | tr -d ' ')"
await_rows file "$before" || fail "files_at produced nothing"
sleep 3
pinned_path="$(git ls-files --with-tree="$head_rev" -- "$GLOB" | head -1)"
pinned_oid="$(git rev-parse "$head_rev:$pinned_path")"
rows_json file | grep -q "\"$pinned_path\",\"$pinned_oid\"" \
  || fail "files_at did not report $pinned_path at its blob oid $pinned_oid"
say "PASS  files_at($head_rev) reports git's own blob oids ($tree_paths tracked paths at that rev)"

# ── 5+6: the REPO-SCOPED pair, against a repository that is not the cwd ──────
#
# The point of these two is routing, so the target must be a tree the server
# would answer differently if `git -C` were dropped: a scratch repository with
# exactly two tracked files, none of which exist here. If the repo column were
# ignored the answer would be this repository's thousands of paths, and both
# assertions below would fail on the row count alone.
#
# The pinned leg is sharper than a count. The scratch worktree is EDITED after
# the commit, so the two hosts disagree BY CONSTRUCTION on the same path:
# repo_files reports the working tree's `git hash-object`, repo_files_at reports
# the committed blob oid. Equal digests would mean the rev is not pinning
# anything, which is the failure a paths-only assertion cannot see.
OTHER="$WORK/other-repo"
mkdir -p "$OTHER"
git -C "$OTHER" init -q
git -C "$OTHER" config user.email files-receipt@example.invalid
git -C "$OTHER" config user.name 'files receipt'
printf 'committed alpha\n' >"$OTHER/alpha.ts"
printf 'committed beta\n' >"$OTHER/beta.ts"
git -C "$OTHER" add alpha.ts beta.ts
git -C "$OTHER" commit -qm 'scratch corpus'
other_rev="$(git -C "$OTHER" rev-parse HEAD)"
committed_alpha="$(git -C "$OTHER" rev-parse "$other_rev:alpha.ts")"
printf 'edited alpha\n' >"$OTHER/alpha.ts"
worktree_alpha="$(git -C "$OTHER" hash-object -- alpha.ts)"
[ "$committed_alpha" != "$worktree_alpha" ] \
  || fail "the receipt's own setup is broken: committed and edited alpha.ts hash the same"

post_arrival "{\"batch\":[{\"rel\":\"want_repo\",\"sign\":\"add\",\"row\":[\"$OTHER\",\"*.ts\"]}]}"
await_rows repo_file 2 || fail "repo_files never produced 2 rows (got $(rows_json repo_file | head -c 300))"
scoped="$(rows_json repo_file | python3 -c 'import json,sys; print(len(json.load(sys.stdin)["rows"]))')"
[ "$scoped" = "2" ] || fail "repo_files produced $scoped rows for a two-file repository; git -C is not routing"
rows_json repo_file | grep -q "\"alpha.ts\",\"$worktree_alpha\"" \
  || fail "repo_files did not report the OTHER repository's working-tree digest for alpha.ts"
say "PASS  repo_files('$OTHER') = that repository's 2 tracked files, at its working-tree digests"

post_arrival "{\"batch\":[{\"rel\":\"want_repo_at\",\"sign\":\"add\",\"row\":[\"$OTHER\",\"$other_rev\",\"*.ts\"]}]}"
await_rows repo_file 3 || fail "repo_files_at produced nothing (got $(rows_json repo_file | head -c 300))"
rows_json repo_file | grep -q "\"alpha.ts\",\"$committed_alpha\"" \
  || fail "repo_files_at did not pin alpha.ts to its committed blob oid $committed_alpha"
say "PASS  repo_files_at($other_rev) pins alpha.ts to $committed_alpha, not the edited $worktree_alpha"

stop_server
say "FILES HOSTS HOLD"
