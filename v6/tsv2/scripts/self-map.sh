#!/usr/bin/env bash
# self-map.sh: the SELF-MAP production rail. Boots the served tsv2 engine on an
# ephemeral port, loads v6/dl/fixtures/self-map.dl6, waits for the derived rels
# to settle, reads them back over HTTP, and renders v6/ARCH-MAP.md.
#
# Run: cd v6 && just self-map                     one shot, server torn down
#      cd v6 && SELF_MAP_WATCH=1 just self-map    stays up, redraws on change
#
# ── what is dl6 and what is this script ──────────────────────────────────────
#
# The dl6 program owns every DERIVATION: which constructs exist, which axes are
# fully live, which phases are unwired, which tasks are ready vs blocked, which
# rels are sinks, every count. This script owns exactly two things the language
# cannot do:
#
#   1  process lifecycle (start a server, load a program, stop the server)
#   2  STRING ASSEMBLY. dl6 has no string aggregate -- `group_concat` is a
#      named refusal and `concat/1` folds a fixed list of expressions, not N
#      rows -- so folding rows into one mermaid document is a host job. This is
#      the language gap the rail exists to make visible, and it is stated in
#      the .dl6 header too rather than only here.
#
# ── determinism ──────────────────────────────────────────────────────────────
#
# ARCH-MAP.md must be byte-stable across runs so a staleness gate can diff it.
# Row order out of the engine is not a guarantee, so self_map_render.py sorts
# every collection it prints and the prolog fact source sorts before printing.
# Nothing in the output carries a timestamp, a port, a temp path, or a row count
# that depends on scheduling.
#
# ── SABOTAGE RECEIPTS (run 2026-07-30, both reverted; tree clean after) ───────
#
#   (1) registry row. Appended to v6/prolog/compile/registry.pl:
#         surface(fake_rail_probe/2, sabotage, no_refs,
#                 wrapper(rel_atom, lower), reserved).
#       ARCH-MAP.md diff, exactly (lines 95-154):
#         + subgraph ax_sabotage["sabotage"] holding
#           c_fake_rail_probex2f2["fake_rail_probe/2<br/>reserved"]
#         + class ax_sabotage moving          (dashed: the axis is not all-live)
#         + | `sabotage` | 1 | 0 | 0 | 1 |    (live 0, refused 0, reserved 1)
#       The all-live column stayed EMPTY for the new axis, which is the
#       `axis_all_live` antijoin doing its job on a row it had never seen.
#
#   (2) ARCH task. Appended to v6/prolog/ARCH.pl:
#         task(sabotage_frontier_probe, unbuilt, [pushdown_optimizer]).
#       ARCH-MAP.md diff, exactly:
#         166 -> 167 tasks, 98 -> 99 dependency edges
#         + t_sabotage_frontier_probe node, `unbuilt`
#         + t_pushdown_optimizer --> t_sabotage_frontier_probe
#         + class t_sabotage_frontier_probe blocked   (its dep is itself open)
#         unbuilt 37 -> 38, Blocked 25 -> 26, Ready UNCHANGED at 46
#       That last number is the discriminating one: a new task with an
#       unfinished dependency must NOT enter the ready set, and it did not.
#
#   Both reverted with `git checkout`; the re-run diffed byte-identical to the
#   pre-sabotage file.
#
# The discriminating part of both: the change reaches the diagram through the
# engine -- prolog one-shot, `sh` host, EDB arrival, dl6 rules, HTTP read. A
# renderer that read the .pl files itself would have produced the same visible
# diff and proven nothing about the rail.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
TSV2="$(cd "$SCRIPT_DIR/.." && pwd)"
V6="$(cd "$TSV2/.." && pwd)"
ROOT="$(cd "$V6/.." && pwd)"
PROGRAM="$V6/dl/fixtures/self-map.dl6"
OUT="$V6/ARCH-MAP.md"
RENDER="$SCRIPT_DIR/self_map_render.py"
SERVE="$TSV2/serve/main.ts"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/self-map.XXXXXX")"
PORT="${SELF_MAP_PORT:-17711}"
BASE="http://127.0.0.1:$PORT"
SERVER_PID=""

export SELF_MAP_FACTS="$V6/prolog/tools/self_map_facts.pl"

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
[ -f "$RENDER" ] || die "renderer is missing: $RENDER"
[ -f "$SELF_MAP_FACTS" ] || die "fact source is missing: $SELF_MAP_FACTS"
command -v swipl >/dev/null || die "swipl is not on PATH; the fact host cannot run"

# The four watched sources are TRACKED paths read as git pathspecs by the watch
# bind's boot enumeration. An untracked source is invisible to the rail and
# would show up as a silently empty diagram, so it is refused by name here.
for source_path in \
  v6/prolog/compile/registry.pl \
  v6/prolog/1_expansion.pl \
  v6/prolog/ARCH.pl \
  v6/dl/fixtures/self-map.dl6; do
  ( cd "$ROOT" && git ls-files --error-unmatch -- "$source_path" ) >/dev/null 2>&1 \
    || die "watched source is not tracked by git, so the watch bind cannot see it: $source_path"
done

# The engine's cwd is the repo root because the watch bind boot-enumerates with
# `git ls-files` and matches relative paths, and because the `sh` host hands
# `swipl` those same relative paths.
(
  cd "$ROOT"
  TSV2_DB="file:$WORK/self-map.sqlite" TSV2_PORT="$PORT" \
    TSV2_WATCH_ROOT="$ROOT" TSV2_WATCH_COALESCE_MS=60 \
    SELF_MAP_FACTS="$SELF_MAP_FACTS" \
    node --experimental-transform-types "$SERVE"
) >"$WORK/server.log" 2>&1 &
SERVER_PID=$!

for _ in $(seq 1 100); do
  curl -s -o /dev/null "$BASE/ticks" 2>/dev/null && break
  kill -0 "$SERVER_PID" 2>/dev/null || die "server died: $(tail -30 "$WORK/server.log")"
  sleep 0.2
done
curl -s -o /dev/null "$BASE/ticks" 2>/dev/null || die "server did not become ready"

status="$(curl -s -o "$WORK/load.json" -w '%{http_code}' -X POST --data-binary @"$PROGRAM" "$BASE/program")"
[ "$status" = 200 ] || die "program load returned $status: $(cat "$WORK/load.json")"

# The rels the map draws from. Fetched as one document so the renderer sees a
# single consistent read rather than N reads across N ticks.
# Only the rels the document actually prints. The program's intermediates
# (task_done, task_open, task_blocker, phase_later, phase_successor,
# axis_non_live, task_state_count) are not fetched -- they exist to be joined,
# not to be drawn, and fetching an unread rel would be dead weight the renderer
# would then have to explain.
RELS="source phase phase_wired phase_unwired phase_step phase_first phase_last \
construct axis axis_total axis_status_total axis_all_live \
task task_dep task_ready task_blocked \
frontier_edge frontier_node state_total task_state_conflict \
program_rel program_edge program_sink program_fan_in program_fan_out program_negated_edge"

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

# QUIESCENCE, without a wall-clock guess: the hosts are subprocesses and the
# engine ticks until nothing is pending, so the rail polls the whole read until
# it is BOTH complete (all four sources present, every diagram's source rel
# non-empty) and STABLE (two consecutive identical reads). A pure "unchanged
# twice" test would accept the empty pre-boot state; a pure completeness test
# would accept a half-filled mid-tick state.
settle_and_render() {
  local previous="" current settled=0 attempt
  for attempt in $(seq 1 240); do
    fetch_all "$WORK/rows.json" || die "read failed; server log: $(tail -30 "$WORK/server.log")"
    if python3 "$RENDER" --complete-check <"$WORK/rows.json" >/dev/null 2>&1; then
      current="$(cksum <"$WORK/rows.json")"
      if [ "$current" = "$previous" ]; then settled=1; break; fi
      previous="$current"
    fi
    kill -0 "$SERVER_PID" 2>/dev/null || die "server died: $(tail -30 "$WORK/server.log")"
    sleep 0.5
  done
  [ "$settled" = 1 ] || die "rels did not settle in 120s; server log: $(tail -30 "$WORK/server.log")"
  python3 "$RENDER" <"$WORK/rows.json" >"$WORK/ARCH-MAP.md" || die "render failed"
  mv "$WORK/ARCH-MAP.md" "$OUT"
  SETTLED_READ="$(cksum <"$WORK/rows.json")"
}

SETTLED_READ=""
settle_and_render
say "SELF MAP WROTE $OUT"
report_shape() {
  python3 - "$OUT" <<'PY'
import sys
text = open(sys.argv[1]).read()
print(f"  diagrams={text.count('```mermaid')} lines={len(text.splitlines())}")
PY
}
report_shape

# ── LIVE MODE ────────────────────────────────────────────────────────────────
#
# `SELF_MAP_WATCH=1 just self-map` keeps the same server up and re-renders on
# every change to a watched source. Nothing here polls the FILESYSTEM: the
# watch bind already turned a file edit into an arrival, so this loop watches
# the DERIVED ROWS and re-renders when they move. A source edit that changes no
# fact (a comment, a reordering the emitter sorts away) correctly redraws
# nothing.
#
# LIVE RECEIPTS (run 2026-07-30, all reverted, tree clean after; the probe
# scripts were scratch and are not checked in -- this loop is the read they
# used):
#   ADD      appended `surface(live_probe_word/1, live_sabotage, ...)` to
#            registry.pl against a running server; `axis` gained
#            `live_sabotage` and `construct` gained the row, with NO restart
#            and NO program reload.
#   RETRACT  `git checkout` on the same file; `live_sabotage` LEFT `axis`,
#            again with no restart. Both directions ride the watcher's own
#            +add/-del digest pair, so the retraction is the engine's rather
#            than a rebuild.
#   THIS LOOP, end to end: with SELF_MAP_WATCH=1 running, inserted
#            `expansion_phase(35, live_probe_phase, unwired).` into
#            1_expansion.pl. ARCH-MAP.md on disk redrew by itself and diagram
#            1 rewired: `ph_row_spread --> ph_live_probe_phase` and
#            `ph_live_probe_phase --> ph_match` REPLACED `ph_row_spread -->
#            ph_match`. That is the `phase_successor` min-aggregate rule doing
#            the work; a hardcoded arrow list could not have rewired. On
#            `git checkout` it redrew back to bytes identical to the clean
#            file.
#   TRAP FOUND writing these: the first probe checked for absence with a
#            present-check helper and reported a false "retraction gap". The
#            watcher coalesces at TSV2_WATCH_COALESCE_MS and the host is a
#            subprocess, so an absence assertion has to WAIT for absence.
if [ "${SELF_MAP_WATCH:-0}" = 1 ]; then
  say "SELF MAP WATCHING (ctrl-c to stop); sources:"
  say "  v6/prolog/compile/registry.pl v6/prolog/1_expansion.pl v6/prolog/ARCH.pl v6/dl/fixtures/self-map.dl6"
  while kill -0 "$SERVER_PID" 2>/dev/null; do
    sleep 1
    fetch_all "$WORK/rows.json" || continue
    python3 "$RENDER" --complete-check <"$WORK/rows.json" >/dev/null 2>&1 || continue
    [ "$(cksum <"$WORK/rows.json")" = "$SETTLED_READ" ] && continue
    settle_and_render
    say "SELF MAP REDREW $OUT"
    report_shape
  done
  die "server exited: $(tail -30 "$WORK/server.log")"
fi

stop_server
say "SELF MAP HOLDS"
