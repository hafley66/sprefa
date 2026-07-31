#!/usr/bin/env bash
# atlas.sh: the DATAFLOW ATLAS production rail. Boots the served tsv2 engine on
# an ephemeral port, loads v6/dl/fixtures/dataflow-atlas.dl6, waits for the four
# fact planes and the reachability stratum to settle, and waits for the program's
# three output effects (the .dot write, the .md write, the graphviz render).
#
# Run: cd v6 && just atlas
#
# ── what is dl6 and what is this script ──────────────────────────────────────
#
# The dl6 program owns every DERIVATION: which symbols cross a file boundary,
# which predicate calls leave their file, which arrows cross a language, the
# reachability fixpoint, the longest path, every count, and the complete DOT and
# Markdown text. This script owns exactly three things the language cannot do:
#
#   1  process lifecycle (start a server, load a program, stop the server)
#   2  process completion (poll the derived rows until they are stable)
#   3  the two environment names the hosts read: DL_EXTRACT_BIN and
#      ATLAS_XREF_FACTS. Both are shell variables inside the host templates,
#      because `fillTemplate` escapes `$` in any value it splices, so a binary
#      path arriving as a rel column could never expand.
#
# ── determinism ──────────────────────────────────────────────────────────────
#
# The .dot must be byte-stable across runs so a staleness gate can diff it.
# Nothing in it carries a timestamp, a port, a temp path or a scheduling-
# dependent count. Every fold is an ordered aggregate: `group_concat/2` lowers
# to `group_concat(x, sep ORDER BY x)` (lower.pl:2894), so a two-argument fold
# is sorted by its own value, and the three-argument folds order by an int
# ordinal. The RUNS ARE COMPARED ON THE .dot, not the .svg -- graphviz is
# deterministic here but it is not this repository's code and its output is not
# what this rail promises.
#
# ── SABOTAGE RECEIPT (run 2026-07-31, scratch copy, reverted; tree clean) ─────
#
#   BRIDGE 2 DELETED. In a scratch copy of dataflow-atlas.dl6 the
#   `atlas_edge(..., 'bridge_spawn')` rule -- the TypeScript-module-to-prolog-
#   predicate arrow that recovers `swipl -g compile_dl6(...)` from a template
#   literal -- was commented out and nothing else was touched.
#
#   Measured, both runs through this script:
#     clean      NODES=<see the counts block printed below> LONGEST=<n>
#     sabotaged  the same node set MINUS the prolog plane's entire reachable
#                cone from the CLI, and LONGEST drops by the length of that
#                cone. The `cli` -> `typescript` -> `prolog` -> `typescript`
#                -> `sqlite` chain becomes `cli` -> `typescript`, because
#                bridge_spawn is the ONLY arrow from the served process into
#                the compiler's own language.
#   The exact numbers for both runs are printed by this script's counts block
#   and recorded in v6/DATAFLOW-ATLAS.md's own tables, which is why they are
#   not frozen into this comment: they move when the code moves, and a frozen
#   number here would be the stale thing the rail exists to prevent.
#
#   The discriminating part: the sabotage removes a RULE, not a node list. A
#   renderer that hardcoded the pipeline would have drawn the same picture and
#   proven nothing.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
TSV2="$(cd "$SCRIPT_DIR/.." && pwd)"
V6="$(cd "$TSV2/.." && pwd)"
ROOT="$(cd "$V6/.." && pwd)"
PROGRAM="${ATLAS_PROGRAM:-$V6/dl/fixtures/dataflow-atlas.dl6}"
DOT_OUT="$V6/DATAFLOW-ATLAS.dot"
SVG_OUT="$V6/DATAFLOW-ATLAS.svg"
MD_OUT="$V6/DATAFLOW-ATLAS.md"
SERVE="$TSV2/serve/main.ts"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/atlas.XXXXXX")"
PORT="${ATLAS_PORT:-17811}"
BASE="http://127.0.0.1:$PORT"
SERVER_PID=""

export ATLAS_XREF_FACTS="$V6/prolog/tools/xref_facts.pl"

say() { printf '%s\n' "$*"; }
die() { say "FAIL  $*"; exit 1; }

stop_server() {
  if [ -n "$SERVER_PID" ]; then
    kill -9 "$SERVER_PID" 2>/dev/null
    wait "$SERVER_PID" 2>/dev/null
    SERVER_PID=""
  fi
}
trap stop_server EXIT

[ -f "$PROGRAM" ] || die "program is missing: $PROGRAM"
[ -f "$ATLAS_XREF_FACTS" ] || die "prolog fact source is missing: $ATLAS_XREF_FACTS"
command -v swipl >/dev/null || die "swipl is not on PATH; the prolog fact host cannot run"
command -v jq >/dev/null || die "jq is not on PATH; the rail receipt cannot inspect rel rows"
command -v dot >/dev/null || die "graphviz dot is not on PATH; the render host cannot run"

# THE PROGRAM MUST BE TRACKED, and this check is here because its absence cost a
# whole run: the watch bind boot-enumerates through `git ls-files -- <glob>`, so
# an untracked source produces no `seed` row, every `want` row hangs off nothing,
# and the rail settles at exactly zero of everything with no error anywhere.
# self-map.sh refuses the same way for the same reason.
( cd "$ROOT" && git ls-files --error-unmatch -- "${PROGRAM#"$ROOT/"}" ) >/dev/null 2>&1 \
  || die "the atlas program is not tracked by git, so the watch bind cannot see it and nothing will seed: $PROGRAM"

# The extractor is FIXED and read-only. Resolution order matches
# flagship-callgraph.sh and extraction-live.sh: an explicit override, then the
# in-tree release build, then build it.
resolve_extract_bin() {
  if [ -n "${DL_EXTRACT_BIN:-}" ] && [ -x "$DL_EXTRACT_BIN" ]; then
    say "extract bin: $DL_EXTRACT_BIN (DL_EXTRACT_BIN)"
    return
  fi
  local crate release
  crate="$ROOT/v6/sprefa-extract"
  release="$crate/target/release/extract"
  if [ ! -x "$release" ]; then
    say "building the in-tree release extractor (cargo build --release --features cli)"
    ( cd "$crate" && cargo build --release --features cli --bin extract ) >"$WORK/cargo.log" 2>&1 \
      || die "cargo build --bin extract failed: $(tail -5 "$WORK/cargo.log")"
  fi
  [ -x "$release" ] || die "no extract binary at $release"
  export DL_EXTRACT_BIN="$release"
  say "extract bin: $DL_EXTRACT_BIN (in-tree release)"
}
resolve_extract_bin

# The engine's cwd is the repo root: the watch bind boot-enumerates with
# `git ls-files` and matches relative paths, the `files` host hands git those
# same relative paths, and every regex host is given a repo-relative path.
(
  cd "$ROOT"
  TSV2_DB="file:$WORK/atlas.sqlite" TSV2_PORT="$PORT" \
    TSV2_WATCH_ROOT="$ROOT" TSV2_WATCH_COALESCE_MS=60 \
    ATLAS_XREF_FACTS="$ATLAS_XREF_FACTS" DL_EXTRACT_BIN="$DL_EXTRACT_BIN" \
    node --experimental-transform-types "$SERVE"
) >"$WORK/server.log" 2>&1 &
SERVER_PID=$!

for ((attempt = 1; attempt <= 100; attempt++)); do
  curl -s -o /dev/null "$BASE/ticks" 2>/dev/null && break
  kill -0 "$SERVER_PID" 2>/dev/null || die "server died: $(tail -30 "$WORK/server.log")"
  sleep 0.2
done
curl -s -o /dev/null "$BASE/ticks" 2>/dev/null || die "server did not become ready"

status="$(curl -s -o "$WORK/load.json" -w '%{http_code}' -X POST --data-binary @"$PROGRAM" "$BASE/program")"
[ "$status" = 200 ] || die "program load returned $status: $(cat "$WORK/load.json")"

# The readiness witnesses: one per fact plane, the derived graph, the two
# reachability answers, the integrity check, and the three effect receipts.
RELS="ts_def_row pl_call_row goal_mention sql_touch cli_command extract_record"
RELS="$RELS atlas_node atlas_edge edge_without_node cycle_node longest_span"
RELS="$RELS dot_receipt md_receipt render_receipt"

fetch_all() {
  local target="$1" rel first=1
  : >"$target"
  printf '{' >>"$target"
  for rel in $RELS; do
    [ "$first" = 1 ] || printf ',' >>"$target"
    first=0
    printf '"%s":' "$rel" >>"$target"
    curl -s "$BASE/idb/$rel" >>"$target" || return 1
  done
  printf '}' >>"$target"
}

# QUIESCENCE, without a wall-clock guess. The hosts are subprocesses (one
# extractor run and one swipl run per file) and the engine ticks until nothing
# is pending, so the rail polls the whole read until it is BOTH complete (every
# plane non-empty, all three effects answered) and STABLE (two consecutive
# identical reads). A pure "unchanged twice" test would accept the empty
# pre-boot state; a pure completeness test would accept a half-filled mid-tick
# state where the reachability fixpoint has not closed.
COMPLETE='(.ts_def_row.rows | length) > 0
  and (.pl_call_row.rows | length) > 0
  and (.goal_mention.rows | length) > 0
  and (.sql_touch.rows | length) > 0
  and (.cli_command.rows | length) > 0
  and (.extract_record.rows | length) > 0
  and (.atlas_node.rows | length) > 0
  and (.atlas_edge.rows | length) > 0
  and (.longest_span.rows | length) == 1
  and (.dot_receipt.rows | length) == 1
  and (.md_receipt.rows | length) == 1
  and (.render_receipt.rows | length) == 1 and .render_receipt.rows[0][0] == "rendered"'

settle() {
  local previous="" current settled=0 attempt
  for ((attempt = 1; attempt <= 600; attempt++)); do
    fetch_all "$WORK/rows.json" || die "read failed; server log: $(tail -30 "$WORK/server.log")"
    if jq -e "$COMPLETE" "$WORK/rows.json" >/dev/null 2>&1; then
      current="$(cksum <"$WORK/rows.json")"
      if [ "$current" = "$previous" ]; then settled=1; break; fi
      previous="$current"
    fi
    kill -0 "$SERVER_PID" 2>/dev/null || die "server died: $(tail -30 "$WORK/server.log")"
    sleep 0.5
  done
  [ "$settled" = 1 ] || die "rels did not settle in 300s; server log: $(tail -30 "$WORK/server.log")"
}

settle
stop_server

[ -s "$DOT_OUT" ] || die "no .dot was written: $DOT_OUT"
[ -s "$SVG_OUT" ] || die "no .svg was rendered: $SVG_OUT"
[ -s "$MD_OUT" ] || die "no .md was written: $MD_OUT"

# INTEGRITY, checked rather than assumed. `edge_without_node` is derived in the
# program: an edge endpoint carrying no `node_meta` row would draw a naked node
# labelled with a raw id, and this is the only place that can be caught.
orphans="$(jq -r '.edge_without_node.rows | length' "$WORK/rows.json")"
[ "$orphans" = 0 ] || die "$orphans edge endpoint(s) have no node metadata: $(jq -c '.edge_without_node.rows' "$WORK/rows.json")"

# The reachability stratum carries a hop cap so a future cyclic edge family
# fails as a bounded wrong number rather than as a hung rail. A node reaching
# itself means the cap is load-bearing and the longest path is a bound, not an
# answer, so the rail refuses instead of publishing it.
cycles="$(jq -r '.cycle_node.rows | length' "$WORK/rows.json")"
[ "$cycles" = 0 ] || die "$cycles node(s) reach themselves; the longest path is a cap, not an answer: $(jq -c '.cycle_node.rows' "$WORK/rows.json")"

nodes="$(jq -r '.atlas_node.rows | length' "$WORK/rows.json")"
edges="$(jq -r '.atlas_edge.rows | length' "$WORK/rows.json")"
longest="$(jq -r '.longest_span.rows[0][0]' "$WORK/rows.json")"
ts_rows="$(jq -r '.ts_def_row.rows | length' "$WORK/rows.json")"
pl_rows="$(jq -r '.pl_call_row.rows | length' "$WORK/rows.json")"
sql_rows="$(jq -r '.sql_touch.rows | length' "$WORK/rows.json")"
goal_rows="$(jq -r '.goal_mention.rows | length' "$WORK/rows.json")"

say "ATLAS WROTE $DOT_OUT"
say "ATLAS WROTE $SVG_OUT"
say "ATLAS WROTE $MD_OUT"
say "  NODES=$nodes EDGES=$edges LONGEST=$longest CYCLES=$cycles ORPHANS=$orphans"
say "  facts: ts_def=$ts_rows pl_call=$pl_rows sql_touch=$sql_rows goal_mention=$goal_rows"
say "  dot lines=$(wc -l <"$DOT_OUT" | tr -d ' ') svg bytes=$(wc -c <"$SVG_OUT" | tr -d ' ')"

# BYTE STABILITY. `ATLAS_SKIP_STABILITY=1` is for the sabotage runs only, where
# the point is the diff and a second run costs a minute for nothing.
if [ "${ATLAS_SKIP_STABILITY:-0}" != 1 ]; then
  first="$(cksum <"$DOT_OUT")"
  cp "$DOT_OUT" "$WORK/first.dot"
  ATLAS_SKIP_STABILITY=1 ATLAS_PORT="$((PORT + 1))" bash "$0" >"$WORK/second.log" 2>&1 \
    || die "the second run failed: $(tail -20 "$WORK/second.log")"
  second="$(cksum <"$DOT_OUT")"
  if [ "$first" != "$second" ]; then
    diff "$WORK/first.dot" "$DOT_OUT" | head -20
    die "the .dot is not byte-stable across two runs"
  fi
  say "  BYTE STABLE across two runs (cksum $first)"
fi

say "ATLAS HOLDS"
