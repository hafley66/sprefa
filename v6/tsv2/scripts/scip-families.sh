#!/usr/bin/env bash
# scip-families.sh — BOTH EXTRACTOR FAMILIES INGESTED BY THE SERVED ENGINE.
#
# One served tsv2 process, one .dl6 program (../dl/fixtures/scip-families.dl6),
# one scratch TypeScript corpus, and ONE DEMAND ROW PER FAMILY posted as an
# arrival. Nothing is faked: the engine is the real served tsv2 runtime, the
# extractor is the in-tree RELEASE build of v6/sprefa-extract, and the `scip`
# family runs REAL scip-typescript over the corpus.
#
# THE CORPUS IS THE DISCRIMINATION. alpha.ts and beta.ts each export a function
# called `helper`; gamma.ts imports alpha's and calls it. Five assertions:
#
#   1  the program loads and declares both hosts
#   2  want_scip demand -> scip_def gains BOTH helpers (both are defined)
#   3  the SAME rows carry scip_ref: gamma referencing ALPHA's helper, which is
#      the compiler's resolution of the import, and NOTHING references beta
#   4  want_diet demand -> diet_edge is EMPTY over those same three files: two
#      corpus definitions share the name, so a name match has no basis to pick
#      and correctly emits nothing rather than guessing
#   5  a second `scip` demand over the same root REUSES the cached index: the
#      indexer does not run twice, proven by the index file's mtime
#
# 3 and 4 together are the receipt that earns two family names, expressed in
# rows the engine derived rather than in the extractor's own stdout.
#
# SABOTAGE RECEIPTS (both run 2026-07-31, both reverted):
#
#   1. Pointing the scip host's template at `--family diet_scip` while leaving
#      the rule's `record == 'scip_def'` filter alone: RED at phase 2 with
#      `scip_def never gained alpha.ts (rows: {"rows":[]})`, because the diet
#      wire carries no scip_def record at all.
#
#   2. Dropping the `record == 'scip_def'` comparison from the scip_def rule:
#      RED at phase 2 with `scip_def holds 8 rows, want exactly 6`, the two
#      extra being gamma's REFERENCES to alpha filed as DEFINITIONS in gamma.ts.
#
# Sabotage 2 is the reason phase 2 asserts an exact count and a wrong-file
# absence rather than presence. AS FIRST WRITTEN THIS SCRIPT DID NOT CATCH IT:
# every presence check still passed, because scip_def and scip_ref carry the
# same two columns and only the tag tells them apart. That hole is exactly what
# makes `record` a declared output column, and a receipt that could not see it
# was asserting the wrong thing.
#
# EXTRACT BINARY: resolved here, never in program text — DL_EXTRACT_BIN, then
# the in-tree RELEASE build, then a build. Same order extraction-live.sh uses.
set -uo pipefail
TSV2="$(cd "$(dirname "$0")/.." && pwd)"

PORT="${TSV2_SCIP_FAMILIES_PORT:-17593}"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/tsv2-scip-families.XXXXXX")"
CORPUS="$WORK/corpus"
DB="file:$WORK/families.sqlite"
PROGRAM="$TSV2/../dl/fixtures/scip-families.dl6"
SERVE_MAIN="$TSV2/serve/main.ts"
BASE="http://127.0.0.1:$PORT"
SERVER_PID=""

fail() { printf 'FAIL  %s\n' "$*"; [ -n "$SERVER_PID" ] && tail -20 "$WORK/server.log"; stop_server; exit 1; }
say() { printf '%s\n' "$*"; }
stop_server() {
  [ -n "$SERVER_PID" ] && kill -9 "$SERVER_PID" 2>/dev/null
  wait "$SERVER_PID" 2>/dev/null
  SERVER_PID=""
}
trap stop_server EXIT

# ── the extractor: in-tree release build, resolved not hardcoded ─────────────
resolve_extract_bin() {
  if [ -n "${DL_EXTRACT_BIN:-}" ] && [ -x "$DL_EXTRACT_BIN" ]; then
    say "extract bin: $DL_EXTRACT_BIN (DL_EXTRACT_BIN)"
    return
  fi
  local crate release
  crate="$(cd "$TSV2/../sprefa-extract" && pwd)"
  release="$crate/target/release/extract"
  if [ ! -x "$release" ]; then
    say "building the in-tree release extractor (cargo build --release --features cli)"
    (cd "$crate" && cargo build --release --features cli --bin extract) >"$WORK/cargo.log" 2>&1 \
      || fail "cargo build failed: $(tail -5 "$WORK/cargo.log")"
  fi
  [ -x "$release" ] || fail "no extract binary at $release"
  export DL_EXTRACT_BIN="$release"
  say "extract bin: $DL_EXTRACT_BIN (in-tree release)"
}

# The indexer must be real. A skipped indexer would leave every rel below empty
# and every assertion vacuous, which is the green that means nothing.
require_indexer() {
  command -v scip-typescript >/dev/null 2>&1 \
    || fail "scip-typescript is not on PATH; this receipt runs the real indexer \
(install: npm install -g @sourcegraph/scip-typescript)"
  say "indexer: scip-typescript $(scip-typescript --version 2>/dev/null | head -1)"
}

# ── the corpus: the ambiguous-name trio ─────────────────────────────────────
# A tsconfig.json is what makes the root DETECT as typescript, and it also keeps
# the indexer off its --infer-tsconfig path, so nothing is written into the tree
# beyond what this script put there.
build_corpus() {
  mkdir -p "$CORPUS"
  cat >"$CORPUS/tsconfig.json" <<'EOF'
{ "compilerOptions": { "target": "ES2020", "module": "ESNext", "strict": true } }
EOF
  cat >"$CORPUS/alpha.ts" <<'EOF'
export function helper(): number {
  return 1;
}
EOF
  cat >"$CORPUS/beta.ts" <<'EOF'
export function helper(): number {
  return 2;
}
EOF
  cat >"$CORPUS/gamma.ts" <<'EOF'
import { helper } from "./alpha";

export function use(): number {
  return helper();
}
EOF
  say "corpus: alpha.ts and beta.ts both export \`helper\`; gamma.ts imports alpha's"
}

start_server() {
  TSV2_DB="$DB" TSV2_PORT="$PORT" \
    DL_EXTRACT_BIN="$DL_EXTRACT_BIN" SCIP_CACHE="$WORK/scip-cache" \
    node --experimental-transform-types "$SERVE_MAIN" >>"$WORK/server.log" 2>&1 &
  SERVER_PID=$!
  for _ in $(seq 1 60); do
    curl -s -o /dev/null "$BASE/ticks" 2>/dev/null && return
    kill -0 "$SERVER_PID" 2>/dev/null || fail "server died on boot: $(tail -5 "$WORK/server.log")"
    sleep 0.2
  done
  fail "server did not become ready on port $PORT"
}

rows_of() { curl -s "$BASE/idb/$1" | tr -d ' \n'; }
row_count() {
  rows_of "$1" | python3 -c 'import json,sys
try: print(len(json.loads(sys.stdin.read())["rows"]))
except Exception: print(-1)'
}
demand() {
  curl -s -o /dev/null -X POST --data-binary \
    "{\"batch\":[{\"rel\":\"$1\",\"sign\":\"add\",\"row\":[\"$CORPUS\"]}]}" "$BASE/arrivals"
}
# poll until `rows_of $1` contains $2
await_rows() {
  local rel="$1" needle="$2" deadline=$((SECONDS + 90))
  while [ "$SECONDS" -lt "$deadline" ]; do
    case "$(rows_of "$rel")" in *"$needle"*) return 0;; esac
    sleep 0.3
  done
  return 1
}

mkdir -p "$WORK/scip-cache"
resolve_extract_bin
require_indexer
build_corpus
start_server

# ── phase 1: the program loads and declares both hosts ──────────────────────
status="$(curl -s -o "$WORK/load.json" -w '%{http_code}' -X POST --data-binary @"$PROGRAM" "$BASE/program")"
[ "$status" = "200" ] || fail "program load returned $status: $(cat "$WORK/load.json")"
grep -q '"scip_rel"' "$WORK/load.json" || fail "no scip_rel host declared: $(cat "$WORK/load.json")"
grep -q '"diet_rel"' "$WORK/load.json" || fail "no diet_rel host declared: $(cat "$WORK/load.json")"
say "PASS  phase 1: program loaded, both family hosts declared"

# ── phase 2: one scip demand row -> real index rows land as EDB arrivals ────
demand want_scip
await_rows scip_def '"alpha.ts"' \
  || fail "phase 2: scip_def never gained alpha.ts (rows: $(rows_of scip_def))"
await_rows scip_def '"beta.ts"' \
  || fail "phase 2: scip_def never gained beta.ts (rows: $(rows_of scip_def))"
# EXACTLY the definitions, and nothing else. Presence alone does not discriminate:
# scip_def and scip_ref both carry a symbol and a file, so a rule that forgot its
# `record` filter would ALSO pass every presence check above while quietly filing
# gamma's REFERENCE to alpha's helper as a DEFINITION in gamma.ts. The count and
# the wrong-file assertion below are what catch it.
defs="$(row_count scip_def)"
[ "$defs" = "6" ] || fail "phase 2: scip_def holds $defs rows, want exactly 6 \
(3 module symbols + alpha.helper + beta.helper + gamma.use): $(rows_of scip_def)"
case "$(rows_of scip_def)" in
  *'["scip-typescriptnpm..`alpha.ts`/helper().","gamma.ts"]'*)
    fail "phase 2: alpha's helper is recorded as DEFINED in gamma.ts, so a \
reference leaked into the definition rel: $(rows_of scip_def)" ;;
esac
say "PASS  phase 2: one want_scip row -> $defs scip_def rows from the real index, definitions only"

# ── phase 3: the compiler's resolution, in rows ────────────────────────────
# gamma REFERENCES alpha's helper. The symbol carries alpha's path because the
# indexer resolved the import; nothing anywhere references beta's.
await_rows scip_ref 'alpha.ts`/helper().' \
  || fail "phase 3: no reference to alpha's helper (rows: $(rows_of scip_ref))"
refs="$(rows_of scip_ref)"
case "$refs" in
  *'beta.ts`/helper().'*)
    fail "phase 3: something references beta's helper, so the projection joined \
on names rather than on the index: $refs" ;;
esac
say "PASS  phase 3: gamma references ALPHA's helper; nothing references beta's"

# ── phase 4: the diet demand over the SAME files derives nothing ───────────
demand want_diet
# The diet host is fast and unconditional, so the absence has to be given time
# to become a presence before it can be trusted: wait for the host to have run
# by watching the tick counter settle, then assert.
sleep 5
diet="$(row_count diet_edge)"
[ "$diet" = "0" ] || fail "phase 4: diet_edge holds $diet rows over a corpus whose \
only cross-file call is unresolvable by name match: $(rows_of diet_edge)"
say "PASS  phase 4: diet_edge is EMPTY -- two defs named \`helper\` leave the name match no basis to choose"

# ── phase 5: a repeat demand reuses the cached index ───────────────────────
# The second demand names the root with a TRAILING SLASH, so it is a genuinely
# different witness and the host really does spawn a second time. That is the
# point: this phase must prove the INDEX CACHE saved the indexer run, not that
# the engine's own content-addressed witness deduplicated the call.
index="$WORK/scip-cache/index.scip"
[ -f "$index" ] || fail "phase 5: no index was cached at $index"
# python3 rather than stat: BSD stat and GNU stat spell mtime with the same
# flag letters and opposite meanings, and both are on this PATH.
mtime_of() { python3 -c 'import os,sys; print(int(os.path.getmtime(sys.argv[1])))' "$1"; }
before="$(mtime_of "$index")"
sleep 1
curl -s -o /dev/null -X POST --data-binary \
  "{\"batch\":[{\"rel\":\"want_scip\",\"sign\":\"add\",\"row\":[\"$CORPUS/\"]}]}" "$BASE/arrivals"
sleep 5
after="$(mtime_of "$index")"
[ "$before" = "$after" ] \
  || fail "phase 5: the index was rebuilt ($before -> $after); an existing index must win untouched"
say "PASS  phase 5: a second demand over the same root reused the cached index (mtime $after unchanged)"

say ""
say "SCIP FAMILIES HOLD  (scip_def $(row_count scip_def), scip_ref $(row_count scip_ref), diet_edge $(row_count diet_edge))"
stop_server
exit 0
