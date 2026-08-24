#!/usr/bin/env bash
# Rust-door gate for ghcache.dl6. Compile, then fold the scripted schedule.
set -uo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../../.." && pwd)"
engine="$root/v6/sprefa-engine-rs"
harness="$engine/target/debug/emit_rust_harness"
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT INT TERM

# The clock checker's path enumeration needs more than the default stack on a
# program this size; docs/failure-modes.md entry 63 carries the measurement.
stack="${GHCACHE_STACK_LIMIT:-12G}"
compile_budget="${GHCACHE_COMPILE_BUDGET:-900}"
fold_budget="${GHCACHE_FOLD_BUDGET:-60}"

cargo build --quiet --manifest-path "$engine/Cargo.toml" --bin emit_rust_harness || exit 1

started=$(date +%s)
timeout "$compile_budget" swipl --stack_limit="$stack" -q \
  -l "$root/v6/prolog/compile.pl" -l "$root/v6/prolog/emit_rust.pl" \
  -g "compile_dl6('$here/ghcache.dl6','$scratch/ghcache.rs',[emitter(emit_rust:emit_program)])" \
  -g halt >"$scratch/compile.log" 2>&1
compile_code=$?
compile_wall=$(( $(date +%s) - started ))

if [ "$compile_code" != 0 ] || [ ! -s "$scratch/ghcache.rs" ]; then
  echo "FAIL ghcache.dl6 did not compile in ${compile_wall}s at stack_limit=$stack"
  grep -m1 '"code"' "$scratch/compile.log" || tail -n 3 "$scratch/compile.log"
  echo "  see v6/dl/ghcache/README.md 'Status' and docs/failure-modes.md entry 63"
  exit 1
fi
printf 'compile: %ss at stack_limit=%s\n' "$compile_wall" "$stack"

started=$(date +%s)
DL_ADAPTERS_DIR="$here" RUST_LOG=sprefa_engine_rs=info \
  timeout "$fold_budget" "$harness" "$scratch/ghcache.rs" \
  "$here/ghcache.schedule.json" --final \
  >"$scratch/out" 2>"$scratch/err"
fold_code=$?
fold_wall=$(( $(date +%s) - started ))

if [ "$fold_code" != 0 ]; then
  echo "FAIL ghcache fold exited $fold_code after ${fold_wall}s"
  { grep -m1 -A1 'panicked at' "$scratch/err" || tail -n 3 "$scratch/err"; }
  exit 1
fi

ticks=$(grep -c '^{"tick"' "$scratch/out")
printf 'fold: %ss, %s ticks\n' "$fold_wall" "$ticks"

# COUNT RECEIPT: a 60s period is ONE minute bucket, four buckets four polls.
# Bucket 3's fresh events answer re-fires pr_due's dirty_repo arm (engine-tick-trace).
due_rows=$(jq -r 'select(.rel == "due") | .rows | length' "$scratch/out" | tail -n 1)
fresh=$(jq -r 'select(.rel == "call_log") | [.rows[] | select(.[3] == 200)] | length' "$scratch/out" | tail -n 1)
cached=$(jq -r 'select(.rel == "call_log") | [.rows[] | select(.[3] == 304)] | length' "$scratch/out" | tail -n 1)
cached_bytes=$(jq -r 'select(.rel == "call_log") | [.rows[] | select(.[3] == 304) | .[6]] | add // 0' "$scratch/out")
remaining=$(jq -r 'select(.rel == "call_log") | [.rows[] | .[4]] | min' "$scratch/out")
transitions=$(jq -r 'select(.rel == "pr_transition") | [.rows[] | select(.[2] == "open" and .[3] == "merged")] | length' "$scratch/out" | tail -n 1)
printf 'due=%s call_log 200=%s 304=%s 304_bytes=%s rate_remaining_min=%s pr_transition_open_merged=%s\n' \
  "$due_rows" "$fresh" "$cached" "$cached_bytes" "$remaining" "$transitions"

fail=0
[ "$due_rows" = 4 ] || { echo "FAIL a 60s period over 4 buckets is 4 polls, got due=$due_rows"; fail=1; }
[ "$fresh" = 5 ] || { echo "FAIL two events 200s plus three graphql 200s is 5, got $fresh"; fail=1; }
# Two due buckets stay cached: the 200 moves the stored tag and
# `page_prev_etag` keeps its bucket reading the tag it asked with.
[ "$cached" = 2 ] || { echo "FAIL every non-fresh events pass is a 304, got $cached"; fail=1; }
[ "$cached_bytes" = 0 ] || { echo "FAIL a 304 moves zero bytes, got $cached_bytes"; fail=1; }
[ "$remaining" = 4996 ] || { echo "FAIL the rate headers decode as ints, got $remaining"; fail=1; }
# The OPEN-only pr_selection never carries a closing PR past its filter; the
# `_recent` alias is what lets pr_transition see it (issues/engine-tick-trace).
[ "$transitions" = 1 ] || { echo "FAIL open -> merged should record exactly once, got $transitions"; fail=1; }
[ "$fail" = 0 ] || exit 1

# RETENTION RECEIPT: three bounded logs reach the emitted program as prune SQL,
# and pr_transition stays keep(all) so it carries none.
for bound in 'ghcache_call_log\\" ORDER BY rowid DESC LIMIT 16500' \
             'ghcache_change_log\\" ORDER BY rowid DESC LIMIT 34000' \
             'ghcache_engine_tick_cost\\" ORDER BY rowid DESC LIMIT 140000'; do
  grep -q "$bound" "$scratch/ghcache.rs" || { echo "FAIL missing prune SQL: $bound"; fail=1; }
done
grep -q 'ghcache_pr_transition\\" ORDER BY rowid DESC LIMIT' "$scratch/ghcache.rs" \
  && { echo "FAIL pr_transition must stay keep(all)"; fail=1; }
[ "$fail" = 0 ] || exit 1
echo "retention: call_log=16500 change_log=34000 engine_tick_cost=140000 pr_transition=all"

# ── leg 2: the account-type split and the 404 backoff ───────────────────────
#
# @comment-ok: the fail-pre-fix numbers, and the one doc site for this leg.
# FAIL PRE FIX, measured in ~/.agent/dl6.db 2026-08-24 over 1427 minute buckets
# (23.8 h): `users/hafley66/events/orgs/hafley66` answered 404 1422 times, one
# per poll cycle, authenticated, decrementing the REST pool, because hafley66 is
# a USER account. `orgs/hafley66/repos` added 24 more, and `not_an_org` /
# `discovered_repo` were BOTH EMPTY: `rest_response` carries a 200 arm and a 304
# arm only, so the org-to-user fallback rule was statically dead. Nothing backed
# a permanently-404 endpoint off: 404, re-demand next bucket, forever.
started=$(date +%s)
DL_ADAPTERS_DIR="$here" RUST_LOG=sprefa_engine_rs=info \
  timeout "$fold_budget" "$harness" "$scratch/ghcache.rs" \
  "$here/ghcache.account.schedule.json" --final \
  >"$scratch/acct" 2>"$scratch/acct.err"
acct_code=$?
acct_wall=$(( $(date +%s) - started ))

if [ "$acct_code" != 0 ]; then
  echo "FAIL ghcache account fold exited $acct_code after ${acct_wall}s"
  { grep -m1 -A1 'panicked at' "$scratch/acct.err" || tail -n 3 "$scratch/acct.err"; }
  exit 1
fi

acct_ticks=$(grep -c '^{"tick"' "$scratch/acct")
calls_for() {
  jq -c 'select(.rel == "call_log")' "$scratch/acct" | tail -n 1 \
    | jq --arg url "$1" '[.rows[] | select(.[2] == $url)] | length'
}
due_for() {
  jq -c 'select(.rel == "due")' "$scratch/acct" | tail -n 1 \
    | jq --arg url "$1" '[.rows[] | select(.[0] == $url)] | length'
}
org_events_calls=$(calls_for "users/hafley66/events/orgs/hafley66")
user_events_calls=$(calls_for "users/hafley66/events")
ghost_calls=$(calls_for "repos/hafley66/ghost/events")
ghost_404=$(jq -c 'select(.rel == "call_log")' "$scratch/acct" | tail -n 1 \
  | jq '[.rows[] | select(.[2] == "repos/hafley66/ghost/events" and .[3] == 404)] | length')
ghost_due=$(due_for "repos/hafley66/ghost/events")
org_repos_due=$(due_for "orgs/hafley66/repos?per_page=100")
user_repos_due=$(due_for "users/hafley66/repos?per_page=100")
printf 'account: %s ticks, org_events=%s user_events=%s ghost_calls=%s ghost_404=%s ghost_due=%s org_repos_due=%s user_repos_due=%s\n' \
  "$acct_ticks" "$org_events_calls" "$user_events_calls" "$ghost_calls" \
  "$ghost_404" "$ghost_due" "$org_repos_due" "$user_repos_due"

# A user account never spells the org events endpoint: 1422 a day becomes zero.
[ "$org_events_calls" = 0 ] || { echo "FAIL the org events endpoint is org-only, got $org_events_calls calls"; fail=1; }
[ "$user_events_calls" = 7 ] || { echo "FAIL a user account polls users/<owner>/events, got $user_events_calls calls"; fail=1; }
# miss_threshold(3): three 404s, then miss_cooloff(60) buckets of silence.
[ "$ghost_calls" = 3 ] || { echo "FAIL the backoff stops a 404 endpoint after 3, got $ghost_calls calls"; fail=1; }
[ "$ghost_404" = 3 ] || { echo "FAIL every ghost call is a 404, got $ghost_404"; fail=1; }
[ "$ghost_due" = 0 ] || { echo "FAIL a cooling endpoint derives no due row, got $ghost_due"; fail=1; }
# not_an_org is sourced off endpoint_miss now, so the fallback actually fires.
[ "$org_repos_due" = 0 ] || { echo "FAIL a user account stops asking /orgs, got $org_repos_due due rows"; fail=1; }
[ "$user_repos_due" = 1 ] || { echo "FAIL the /users repos fallback must appear once, got $user_repos_due"; fail=1; }
[ "$fail" = 0 ] || exit 1

echo "GHCACHE_RUST_DOOR_HOLDS ticks=$ticks account_ticks=$acct_ticks"
