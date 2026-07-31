#!/usr/bin/env bash
# atlas.sh: the DATAFLOW ATLAS production rail. Boots the served tsv2 engine on
# an ephemeral port, loads v6/dl/fixtures/dataflow-atlas.dl6, waits for the four
# fact planes and the reachability stratum to settle, and waits for the program's
# eleven output effects (five .dot writes, the .md write, five graphviz renders).
#
# ── FIVE DRAWINGS, AND THE THREE THINGS THAT MAKES CHECKABLE ─────────────────
#
#   DATAFLOW-ATLAS.dot          TB, clustered by language then by file
#   DATAFLOW-ATLAS-flat-td.dot  TB, no clusters at all
#   DATAFLOW-ATLAS-flat-lr.dot  LR, no clusters at all
#   DATAFLOW-ATLAS-fs-td.dot    TB, nested clusters mirroring the directory tree
#   DATAFLOW-ATLAS-fs-lr.dot    LR, the same nesting
#
# The program derives ONE node line per node and folds it five ways, so three
# claims stop being claims and become assertions below:
#
#   1  every .dot draws the same node id set (sorted ids compared byte for byte
#      against the language view, and that view's line count against the rel)
#   2  every node line in either fs view sits inside the exact chain of cluster
#      labels its own path spells -- checked for ALL of them, with the count
#      printed so an empty check cannot pass
#   3  neither flat view carries a single `subgraph cluster_`
#
# ── WHAT THIS RAIL COSTS NOW, and why ────────────────────────────────────────
#
# ~10 minutes, of which ~8.5 is swipl. The text door has no compile cache, so
# the byte-stability leg pays a second full compile, and this program compiles
# in ~4m16s (30s before the four extra views). The cost is `3_clock_check.pl`'s
# simple-path enumeration against the sixteen-rel filesystem fold; the numbers
# and the 1GB-stack crash that came with them are recorded in the program's own
# header. Nothing about it is this rail's to fix, and it is why `just atlas`
# stays out of green-all.
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
#   literal -- was deleted verbatim and nothing else was touched. Both runs
#   through this script, `ATLAS_PROGRAM` pointing at the copy:
#
#     clean      NODES=421 EDGES=809 LONGEST=19 CYCLES=0 ORPHANS=0
#                `bop run` to a SQLite table: 12 hops
#     sabotaged  NODES=421 EDGES=807 LONGEST=11 CYCLES=0 ORPHANS=0
#                `bop run` to a SQLite table: THE TABLE IS EMPTY
#
#   TWO EDGES. That is the whole cost of the sabotage, and it takes the longest
#   path from 19 hops to 11 and erases the CLI-to-database answer completely,
#   because `bridge_spawn` is the only arrow from the served process into the
#   compiler's own language: without it prolog, the emitted module and every
#   SQLite table become unreachable from the CLI in one stroke.
#
#   The discriminating part: the sabotage removes a RULE, not a node list. A
#   renderer that hardcoded the pipeline would have drawn the same picture and
#   proven nothing. The node count does NOT move, either -- the nodes are still
#   drawn, they are simply no longer reachable -- so an assertion on node count
#   alone would have passed the sabotage. The reachability answer is what
#   discriminates.
#
# ── SABOTAGE RECEIPT 2, the filesystem nesting (2026-07-31, scratch copy) ─────
#
#   ONE CLAUSE POINTED AT THE WRONG FILE. In a scratch copy,
#
#     node_file(node_id, path) <- atlas_node(node_id, 'prolog', path, _, _).
#
#   became
#
#     node_file(node_id, 'v6/tsv2/cli/bop.ts') <- atlas_node(node_id, 'prolog', _, _, _).
#
#   so every prolog node claims to live in a TypeScript file. Nothing else was
#   touched. The run, `ATLAS_PROGRAM` pointing at the copy:
#
#     SAME NODE SET in all 5 .dot files (421 ids each, identical after sort)
#     pl:v6/prolog/0_body_walk.pl#walk_body/3 sits in [v6/tsv2/cli/bop.ts]
#       but its path is [v6/prolog/0_body_walk.pl]
#     ... 265 such lines ...
#     265 misplaced node line(s)
#     FAIL  the fs-td .dot nests a node under the wrong directory
#
#   THE FIRST LINE IS THE POINT. The same-node-set assertion PASSED the
#   sabotage, and so would a cluster count, a node count, an orphan check and
#   the reachability answer: every node is still drawn exactly once, every edge
#   still lands, the graph is unchanged. Only a check that compares each node's
#   ENCLOSING CLUSTER CHAIN against its own path can see a node in the wrong
#   box, which is why the nesting check exists and why it is total rather than
#   a sample.
#
# ── HOW THE ratio TABLE IN THE PROGRAM HEADER WAS MEASURED ───────────────────
#
#   For each of the four new .dot files, with the rail's own output on disk:
#
#     sed 's/^  ratio=.*/  ratio=0.7;/' DATAFLOW-ATLAS-fs-lr.dot > /tmp/t.dot
#     dot -Tsvg /tmp/t.dot -o /tmp/t.svg
#     grep -m1 -o 'width="[0-9]*pt" height="[0-9]*pt"' /tmp/t.svg
#
#   and with `grep -v '^  ratio='` for the unlevered row.

# ── BUDGETS (timeout-gun lane, 2026-07-31) ───────────────────────────────────
#
# Standing law: every compute invocation runs under a budget with a NAMED
# timeout failure. This rail is the law's hardest case, because almost none of
# its cost is a command it waits on: it is a backgrounded node server, one
# extractor subprocess per TypeScript file and one swipl per .pl file that the
# SERVER spawns, five graphviz renders the PROGRAM demands, and an HTTP poll
# loop. Only a process-group cap around the whole script covers all of that,
# which is what `cap_self` installs (v6/tools/run-capped.sh: fork + setpgrp,
# SIGKILL to the group, exit 124).
#
# ATLAS_BUDGET_S, default 2400 (40 min). The measured wall is ~10 min for the
# two runs the byte-stability leg needs, of which ~8.5 min is the text door
# compiling this program TWICE at 4m16s each -- all of it the filed
# `clock_check_path_blowup`, none of it this rail's to fix. 4x the honest wall
# is the headroom, and the honest wall is currently a defect's. When the
# blowup lands this default should come down with it.
#
# ATLAS_DOT_BUDGET_S, default 180, PER RENDER. graphviz is the one leg the
# whole-script cap would report unhelpfully (a hung `dot` and a hung server
# read the same from outside), so the render host in dataflow-atlas.dl6 fires
# its own gun. It reaches the helper through ATLAS_RUN_CAPPED for the reason
# every other path here is an env var: fillTemplate escapes `$` in any value it
# splices, so a path arriving as a rel column could never expand. The five
# renders together are seconds; 180s is the "graphviz has stopped being
# graphviz" line.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
TSV2="$(cd "$SCRIPT_DIR/.." && pwd)"
V6="$(cd "$TSV2/.." && pwd)"
ROOT="$(cd "$V6/.." && pwd)"

. "$V6/tools/run-capped.sh"
cap_self "${ATLAS_BUDGET_S:-2400}" atlas "$@"

export ATLAS_RUN_CAPPED="$V6/tools/run-capped.sh"
export ATLAS_DOT_BUDGET_S="${ATLAS_DOT_BUDGET_S:-180}"
PROGRAM="${ATLAS_PROGRAM:-$V6/dl/fixtures/dataflow-atlas.dl6}"

# THE FIVE DRAWINGS, in the same order `variant/7` declares them. The names are
# repeated here rather than read out of the program because this script has to
# check the files BEFORE it can trust anything the program says about them.
VARIANTS="lang flat_td flat_lr fs_td fs_lr"
DOT_lang="$V6/DATAFLOW-ATLAS.dot";         SVG_lang="$V6/DATAFLOW-ATLAS.svg"
DOT_flat_td="$V6/DATAFLOW-ATLAS-flat-td.dot"; SVG_flat_td="$V6/DATAFLOW-ATLAS-flat-td.svg"
DOT_flat_lr="$V6/DATAFLOW-ATLAS-flat-lr.dot"; SVG_flat_lr="$V6/DATAFLOW-ATLAS-flat-lr.svg"
DOT_fs_td="$V6/DATAFLOW-ATLAS-fs-td.dot";     SVG_fs_td="$V6/DATAFLOW-ATLAS-fs-td.svg"
DOT_fs_lr="$V6/DATAFLOW-ATLAS-fs-lr.dot";     SVG_fs_lr="$V6/DATAFLOW-ATLAS-fs-lr.svg"
dot_of() { eval "printf '%s' \"\$DOT_$1\""; }
svg_of() { eval "printf '%s' \"\$SVG_$1\""; }
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

# THE WATCHED SOURCE MUST BE TRACKED, and this check is here because its absence
# cost a whole run: the watch bind boot-enumerates through `git ls-files --
# <glob>`, so an untracked source produces no `seed` row, every `want` row hangs
# off nothing, and the rail settles at exactly zero of everything with no error
# anywhere. self-map.sh refuses the same way for the same reason.
#
# It is the WATCHED PATH that is checked and not `$PROGRAM`, because those are
# two different things: a sabotage run loads an edited scratch copy through
# `ATLAS_PROGRAM` while the seed still hangs off the tracked fixture.
WATCHED="v6/dl/fixtures/dataflow-atlas.dl6"
( cd "$ROOT" && git ls-files --error-unmatch -- "$WATCHED" ) >/dev/null 2>&1 \
  || die "the watched source is not tracked by git, so the watch bind cannot see it and nothing will seed: $WATCHED"

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
  capped_curl "${ATLAS_HTTP_BUDGET_S:-30}" -s -o /dev/null "$BASE/ticks" 2>/dev/null && break
  kill -0 "$SERVER_PID" 2>/dev/null || die "server died: $(tail -30 "$WORK/server.log")"
  sleep 0.2
done
capped_curl "${ATLAS_HTTP_BUDGET_S:-30}" -s -o /dev/null "$BASE/ticks" 2>/dev/null || die "server did not become ready"

status="$(capped_curl "${ATLAS_HTTP_BUDGET_S:-30}" -s -o "$WORK/load.json" -w '%{http_code}' -X POST --data-binary @"$PROGRAM" "$BASE/program")"
[ "$status" = 200 ] || die "program load returned $status: $(cat "$WORK/load.json")"

# The readiness witnesses: one per fact plane, the derived graph, the two
# reachability answers, the integrity check, and the three effect receipts.
RELS="ts_def_row pl_call_row goal_mention sql_touch cli_command extract_record"
RELS="$RELS atlas_node atlas_edge edge_without_node cycle_node longest_span"
RELS="$RELS path_dir fs_unplaced fs_dir_too_deep variant_clusters variant_canvas"
RELS="$RELS dot_receipt md_receipt render_receipt"

fetch_all() {
  local target="$1" rel first=1
  : >"$target"
  printf '{' >>"$target"
  for rel in $RELS; do
    [ "$first" = 1 ] || printf ',' >>"$target"
    first=0
    printf '"%s":' "$rel" >>"$target"
    capped_curl "${ATLAS_HTTP_BUDGET_S:-30}" -s "$BASE/idb/$rel" >>"$target" || return 1
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
  and (.path_dir.rows | length) > 0
  and (.longest_span.rows | length) == 1
  and (.variant_clusters.rows | length) == 5
  and (.variant_canvas.rows | length) == 5
  and (.dot_receipt.rows | length) == 5
  and (.md_receipt.rows | length) == 1
  and (.render_receipt.rows | length) == 5
  and ([.render_receipt.rows[] | select(.[1] == "rendered")] | length) == 5'

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

for variant in $VARIANTS; do
  [ -s "$(dot_of "$variant")" ] || die "no .dot was written for $variant: $(dot_of "$variant")"
  [ -s "$(svg_of "$variant")" ] || die "no .svg was rendered for $variant: $(svg_of "$variant")"
done
[ -s "$MD_OUT" ] || die "no .md was written: $MD_OUT"

# INTEGRITY, checked rather than assumed. `edge_without_node` is derived in the
# program: an edge endpoint carrying no `node_meta` row would draw a naked node
# labelled with a raw id, and this is the only place that can be caught.
orphans="$(jq -r '.edge_without_node.rows | length' "$WORK/rows.json")"
[ "$orphans" = 0 ] || die "$orphans edge endpoint(s) have no node metadata: $(jq -c '.edge_without_node.rows' "$WORK/rows.json")"

# The reachability stratum carries a hop cap so a future cyclic edge family
# fails as a bounded wrong number rather than as a hung rail. A node reaching
# itself means the cap is what stopped the walk and the longest path is a
# bound, not an answer, so the rail refuses instead of publishing it.
cycles="$(jq -r '.cycle_node.rows | length' "$WORK/rows.json")"
[ "$cycles" = 0 ] || die "$cycles node(s) reach themselves; the longest path is a cap, not an answer: $(jq -c '.cycle_node.rows' "$WORK/rows.json")"

nodes="$(jq -r '.atlas_node.rows | length' "$WORK/rows.json")"
edges="$(jq -r '.atlas_edge.rows | length' "$WORK/rows.json")"
longest="$(jq -r '.longest_span.rows[0][0]' "$WORK/rows.json")"
ts_rows="$(jq -r '.ts_def_row.rows | length' "$WORK/rows.json")"
pl_rows="$(jq -r '.pl_call_row.rows | length' "$WORK/rows.json")"
sql_rows="$(jq -r '.sql_touch.rows | length' "$WORK/rows.json")"
goal_rows="$(jq -r '.goal_mention.rows | length' "$WORK/rows.json")"

# The filesystem view's two integrity rels, both derived in the program. A node
# that lands in no cluster would vanish from two of the five drawings while the
# other three still showed it; a directory deeper than the unrolled fold would
# be dropped without a word.
unplaced="$(jq -r '.fs_unplaced.rows | length' "$WORK/rows.json")"
[ "$unplaced" = 0 ] || die "$unplaced node(s) belong to no filesystem cluster: $(jq -c '.fs_unplaced.rows' "$WORK/rows.json")"
too_deep="$(jq -r '.fs_dir_too_deep.rows | length' "$WORK/rows.json")"
[ "$too_deep" = 0 ] || die "$too_deep director(y|ies) sit below the four unrolled nesting levels; the fold would drop them: $(jq -c '.fs_dir_too_deep.rows' "$WORK/rows.json")"

# ONE GRAPH, FIVE DRAWINGS, and this is where that stops being a claim. A node
# line is `    "id" [label=...` and an edge line is `  "from" -> "to" [color=`,
# so requiring `[label=` immediately after the closing quote separates them.
node_ids_of() { grep -Eo '^[[:space:]]*"[^"]*" \[label=' "$1" | sed -E 's/^[[:space:]]*"(.*)" \[label=$/\1/' | sort; }
node_ids_of "$(dot_of lang)" >"$WORK/ids.lang"
lang_id_count="$(wc -l <"$WORK/ids.lang" | tr -d ' ')"
[ "$lang_id_count" = "$nodes" ] \
  || die "the language .dot draws $lang_id_count node lines but the graph has $nodes nodes"
for variant in $VARIANTS; do
  node_ids_of "$(dot_of "$variant")" >"$WORK/ids.$variant"
  cmp -s "$WORK/ids.lang" "$WORK/ids.$variant" \
    || die "the $variant .dot draws a different node id set than the language .dot: $(diff "$WORK/ids.lang" "$WORK/ids.$variant" | head -5)"
done
say "  SAME NODE SET in all 5 .dot files ($lang_id_count ids each, identical after sort)"

# THE NESTING IS CHECKED AGAINST THE PATHS, not eyeballed. Walk the fs .dot
# keeping a stack of cluster labels; every node line must sit inside the exact
# chain of directory labels its own id spells. Directory labels carry a
# trailing slash and file labels do not, so the stack concatenated with no
# separator IS the path: `v6/` `tsv2/` `serve/` `1_hosts.ts`.
#
# This is total, not a spot check: every node line in the file is compared, and
# the count is printed so a rule that stopped emitting node lines cannot pass
# by checking nothing.
check_fs_nesting() {
  awk -v want_checked="$1" '
    /^[[:space:]]*subgraph cluster_/ { pending = 1; next }
    pending && /^[[:space:]]*label="/ {
      label = $0
      sub(/^[[:space:]]*label="/, "", label)
      sub(/";$/, "", label)
      depth++
      stack[depth] = label
      pending = 0
      next
    }
    /^[[:space:]]*}$/ { if (depth > 0) depth--; next }
    /^[[:space:]]*"[^"]*" \[label=/ {
      id = $0
      sub(/^[[:space:]]*"/, "", id)
      sub(/" \[label=.*$/, "", id)
      here = ""
      for (i = 1; i <= depth; i++) here = here stack[i]
      checked++
      if (id ~ /^(ts|pl):/) {
        want = id
        sub(/^(ts|pl):/, "", want)
        sub(/#.*$/, "", want)
      } else if (id ~ /^sh:/) {
        want = id
        sub(/^sh:/, "", want)
      } else {
        if (here !~ /^\(no file\) /) { bad++; print "  " id " sits in [" here "] but names no file"; }
        next
      }
      if (here != want) { bad++; print "  " id " sits in [" here "] but its path is [" want "]"; }
    }
    END {
      if (checked != want_checked) { print "  checked " checked " node lines, expected " want_checked; exit 1 }
      if (bad > 0) { print "  " bad " misplaced node line(s)"; exit 1 }
      printf "  FS NESTING: %d node lines, every one inside the cluster chain its path spells\n", checked
    }' "$2"
}
check_fs_nesting "$lang_id_count" "$(dot_of fs_td)" || die "the fs-td .dot nests a node under the wrong directory"
check_fs_nesting "$lang_id_count" "$(dot_of fs_lr)" || die "the fs-lr .dot nests a node under the wrong directory"

# The flat views must carry no cluster at all; that is the whole point of them.
for variant in flat_td flat_lr; do
  stray="$(grep -c 'subgraph cluster_' "$(dot_of "$variant")" || true)"
  [ "$stray" = 0 ] || die "the $variant .dot carries $stray cluster(s) and is supposed to carry none"
done

say "ATLAS WROTE $MD_OUT"
say "  NODES=$nodes EDGES=$edges LONGEST=$longest CYCLES=$cycles ORPHANS=$orphans"
say "  facts: ts_def=$ts_rows pl_call=$pl_rows sql_touch=$sql_rows goal_mention=$goal_rows"
for variant in $VARIANTS; do
  dot_file="$(dot_of "$variant")"
  svg_file="$(svg_of "$variant")"
  clusters="$(jq -r --arg v "$variant" '[.variant_clusters.rows[] | select(.[0] == $v) | .[1]][0]' "$WORK/rows.json")"
  canvas="$(jq -r --arg v "$variant" '[.variant_canvas.rows[] | select(.[0] == $v) | "\(.[1]) x \(.[2])"][0]' "$WORK/rows.json")"
  say "ATLAS WROTE $dot_file + $svg_file"
  say "  $variant: dot lines=$(wc -l <"$dot_file" | tr -d ' ') clusters=$clusters canvas=${canvas}pt svg bytes=$(wc -c <"$svg_file" | tr -d ' ')"
done

# BYTE STABILITY, over EVERY .dot rather than the headline one. A variant that
# folded on a set with no ordinal would flip between runs and the other four
# would stay still, so the gate digests all five and names the one that moved.
# `ATLAS_SKIP_STABILITY=1` is for the sabotage runs only, where the point is the
# diff and a second run costs a minute for nothing.
if [ "${ATLAS_SKIP_STABILITY:-0}" != 1 ]; then
  for variant in $VARIANTS; do
    cp "$(dot_of "$variant")" "$WORK/first.$variant.dot"
  done
  ATLAS_SKIP_STABILITY=1 ATLAS_PORT="$((PORT + 1))" bash "$0" >"$WORK/second.log" 2>&1 \
    || die "the second run failed: $(tail -20 "$WORK/second.log")"
  for variant in $VARIANTS; do
    first="$(cksum <"$WORK/first.$variant.dot")"
    second="$(cksum <"$(dot_of "$variant")")"
    if [ "$first" != "$second" ]; then
      diff "$WORK/first.$variant.dot" "$(dot_of "$variant")" | head -20
      die "the $variant .dot is not byte-stable across two runs"
    fi
    say "  BYTE STABLE $variant (cksum $first)"
  done
fi

say "ATLAS HOLDS"
