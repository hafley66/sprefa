#!/usr/bin/env bash
#   bash v6/dl/selfdoc/selfdoc.sh [OUTPUT_DIR]

# Rendering is printf over the `--final-tsv` stream: no python, no jq, no awk.
set -euo pipefail
if [ "${BASH_VERSINFO[0]}" -lt 4 ]; then
  printf 'FAIL  bash 4+ needed for the node-id map; this is %s\n' "$BASH_VERSION" >&2
  exit 1
fi
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
V6="$(cd "$HERE/../.." && pwd)"
ROOT="$(cd "$V6/.." && pwd)"
ENGINE="$V6/sprefa-engine-rs"
OUT="${1:-$ROOT/docs/selfdoc}"
TAB="$(printf '\t')"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/selfdoc.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT
fail() { printf 'FAIL  %s\n' "$*" >&2; exit 1; }

swipl -q -l "$V6/prolog/compile.pl" -l "$V6/prolog/emit_rust.pl" \
  -g "compile_dl6('$HERE/selfdoc.dl6','$WORK/selfdoc.rs',[emitter(emit_rust:emit_program)])" -g halt \
  >"$WORK/compile.log" 2>&1 || fail "compile: $(tail -20 "$WORK/compile.log")"

# Built only when absent: `~/.cargo/.package-cache` is one lock for the machine,
# so a concurrent lane's cargo blocks this one for minutes on an up-to-date tree.
HARNESS="$ENGINE/target/release/emit_rust_harness"
if [ ! -x "$HARNESS" ]; then
  cargo build --release --quiet --manifest-path "$ENGINE/Cargo.toml" --bin emit_rust_harness \
    >"$WORK/build.log" 2>&1 || fail "cargo build: $(tail -5 "$WORK/build.log")"
fi

# One `want` per tree. In a git pathspec `*` crosses `/`, so each reaches every
# depth; `v6/dl` is seeded twice because a pathspec names one extension.
RELS='selfdoc_board,selfdoc_board_edge,selfdoc_module,selfdoc_module_edge,selfdoc_phase_edge,selfdoc_hot_hub,selfdoc_host,selfdoc_orphan_predicate,selfdoc_symbol'
( cd "$ROOT" && DL_ADAPTERS_DIR="$HERE" \
    DL_EXTRACT_BIN="${DL_EXTRACT_BIN:-$V6/sprefa-extract/target/release/extract}" \
    "$HARNESS" "$WORK/selfdoc.rs" \
    --arrive 'want=v6/prolog/*.pl' \
    --arrive 'want=v6/sprefa-extract/src/*.rs' \
    --arrive 'want=v6/dl/*.dl6' \
    --arrive 'want=v6/dl/*.adapters.json' \
    --live-hosts --final-only --final-tsv --final-rels "$RELS" ) \
  >"$WORK/final.tsv" 2>"$WORK/err" || fail "run: $(tail -20 "$WORK/err")"

rows_of() { grep "^$1$TAB" "$WORK/final.tsv" | cut -f2- || true; }
rows_of selfdoc_board            >"$WORK/board"
rows_of selfdoc_board_edge       >"$WORK/board_edge"
rows_of selfdoc_phase_edge       >"$WORK/phase_edge"
rows_of selfdoc_host             >"$WORK/host"
rows_of selfdoc_orphan_predicate >"$WORK/orphan"
rows_of selfdoc_symbol           >"$WORK/symbol"

# The unga shape budget: 24 shapes to a board. Twelve is the cap here so a
# board's edges stay readable; every undrawn module is in the table under it.
NODE_CAP=12
ORPHAN_CAP=40

mkdir -p "$OUT"

# ── one board: a mermaid flowchart of the phase's top modules, then its table ──
# `$1` doc file, `$2` phase, `$3` heading, `$4` one line on what the phase is.
render_board() {
  local doc="$1" phase="$2" heading="$3" caption="$4" draw="${5:-draw}"
  local drawn=0 total=0 cap="$NODE_CAP" line stem path defs sites fan_in fan_out
  declare -A node_id=()
  [ "$draw" = draw ] || cap=0

  while IFS="$TAB" read -r line_phase path stem defs sites fan_in fan_out; do
    [ "$line_phase" = "$phase" ] || continue
    total=$((total + 1))
    if [ "$drawn" -lt "$cap" ]; then
      drawn=$((drawn + 1))
      node_id["$path"]="n$drawn"
    fi
  done <"$WORK/board"

  [ "$total" -gt 0 ] || return 0

  {
    printf '## %s\n\n' "$heading"
    printf '%s\n\n' "$caption"
  } >>"$doc"

  # Three boxes in a row read as a list, so a board under three shapes is one.
  if [ "$drawn" -ge 3 ]; then
    {
      printf '```mermaid\n'
      printf 'flowchart LR\n'
    } >>"$doc"

    while IFS="$TAB" read -r line_phase path stem defs sites fan_in fan_out; do
      [ "$line_phase" = "$phase" ] || continue
      [ -n "${node_id[$path]:-}" ] || continue
      printf '  %s["%s  in%s/out%s"]\n' "${node_id[$path]}" "$stem" "$fan_in" "$fan_out" >>"$doc"
    done <"$WORK/board"

    while IFS="$TAB" read -r line_phase from_path to_path weight; do
      [ "$line_phase" = "$phase" ] || continue
      [ -n "${node_id[$from_path]:-}" ] || continue
      [ -n "${node_id[$to_path]:-}" ] || continue
      printf '  %s -->|%s| %s\n' "${node_id[$from_path]}" "$weight" "${node_id[$to_path]}" >>"$doc"
    done <"$WORK/board_edge"

    printf '```\n\n' >>"$doc"
  fi

  {
    if [ "$total" -gt "$drawn" ] && [ "$drawn" -ge 3 ]; then
      printf '%s of %s modules are drawn, ranked by fan-in; every one is in the table.\n\n' \
        "$drawn" "$total"
    fi
    printf '| module | defs | sites | fan-in | fan-out |\n'
    printf '|---|---:|---:|---:|---:|\n'
  } >>"$doc"

  while IFS="$TAB" read -r line_phase path stem defs sites fan_in fan_out; do
    [ "$line_phase" = "$phase" ] || continue
    printf '| `%s` | %s | %s | %s | %s |\n' "$path" "$defs" "$sites" "$fan_in" "$fan_out" >>"$doc"
  done <"$WORK/board"

  printf '\n' >>"$doc"
}

# The cross-phase board, drawn from selfdoc_phase_edge over one phase set.
# `$1` doc file, `$2..` the phases this document owns.
render_phase_board() {
  local doc="$1"; shift
  local phases=" $* "
  local index=0 phase from_phase to_phase weight
  declare -A phase_id=()

  for phase in $phases; do
    index=$((index + 1))
    phase_id["$phase"]="p$index"
  done

  {
    printf '## The phase graph\n\n'
    printf 'An edge counts the distinct TARGET modules the source phase calls into.\n\n'
    printf '```mermaid\n'
    printf 'flowchart LR\n'
  } >>"$doc"

  for phase in $phases; do
    printf '  %s["%s"]\n' "${phase_id[$phase]}" "$phase" >>"$doc"
  done

  while IFS="$TAB" read -r from_phase to_phase weight; do
    [ -n "${phase_id[$from_phase]:-}" ] || continue
    [ -n "${phase_id[$to_phase]:-}" ] || continue
    [ "$from_phase" = "$to_phase" ] && continue
    printf '  %s -->|%s| %s\n' "${phase_id[$from_phase]}" "$weight" "${phase_id[$to_phase]}" >>"$doc"
  done <"$WORK/phase_edge"

  printf '```\n\n' >>"$doc"
}

render_host_roster() {
  local doc="$1" prefix="$2" program host executor demands
  {
    printf '## The host roster\n\n'
    printf 'Read from the `.adapters.json` sidecars; `demands` counts the rule bodies\n'
    printf 'in the owning program that name the host.\n\n'
    printf '| program | host | executor | demands |\n'
    printf '|---|---|---|---:|\n'
  } >>"$doc"
  while IFS="$TAB" read -r program host executor demands; do
    case "$program" in "$prefix"*) ;; *) continue ;; esac
    printf '| `%s` | `%s` | %s | %s |\n' "$program" "$host" "$executor" "$demands" >>"$doc"
  done <"$WORK/host"
  printf '\n' >>"$doc"
}

render_orphans() {
  local doc="$1" prefix="$2" shown=0 total=0 path name arity
  while IFS="$TAB" read -r path name arity; do
    case "$path" in "$prefix"*) total=$((total + 1)) ;; esac
  done <"$WORK/orphan"
  {
    printf '## Orphan predicates\n\n'
    printf 'A predicate no clause calls, no term mentions, and no `:- module/2` export\n'
    printf 'list names. It OVER-reports in one shape: `maplist(bare_atom, ...)` passes\n'
    printf 'the closure as an arity-0 ATOM, and the call family emits no record for an\n'
    printf 'atom argument, so `path_component_stem/2` reads as an orphan against a live\n'
    printf 'call at `0_anonymous_expand.pl:266`. Read every row before deleting one.\n\n'
    printf 'Total: %s.\n\n' "$total"
    printf '| module | predicate | arity |\n'
    printf '|---|---|---:|\n'
  } >>"$doc"
  while IFS="$TAB" read -r path name arity; do
    case "$path" in "$prefix"*) ;; *) continue ;; esac
    [ "$shown" -lt "$ORPHAN_CAP" ] || continue
    shown=$((shown + 1))
    printf '| `%s` | `%s` | %s |\n' "$path" "$name" "$arity" >>"$doc"
  done <"$WORK/orphan"
  if [ "$total" -gt "$shown" ]; then
    printf '\nThe first %s of %s, by module. Run `just selfdoc` for the whole set.\n' \
      "$shown" "$total" >>"$doc"
  fi
  printf '\n' >>"$doc"
}

render_symbols() {
  local doc="$1" prefix="$2" shown=0 path name fan_in fan_out
  {
    printf '## The twenty hottest symbols\n\n'
    printf '`fan_in` counts the distinct CALLING DEFINITIONS, one per (file, name) pair\n'
    printf 'whose clauses name it. `fan_out` counts the distinct callees of its own.\n\n'
    printf '| module | symbol | fan-in | fan-out |\n'
    printf '|---|---|---:|---:|\n'
  } >>"$doc"
  while IFS="$TAB" read -r path name fan_in fan_out; do
    case "$path" in "$prefix"*) ;; *) continue ;; esac
    [ "$shown" -lt 20 ] || continue
    shown=$((shown + 1))
    printf '| `%s` | `%s` | %s | %s |\n' "$path" "$name" "$fan_in" "$fan_out" >>"$doc"
  done <"$WORK/symbol"
  printf '\n' >>"$doc"
}

# ── compiler.md ─────────────────────────────────────────────────────────────
COMPILER="$OUT/compiler.md"
{
  printf '# The dl6 compiler, read by sprefa-extract\n\n'
  printf 'Generated by `just selfdoc` from `v6/dl/selfdoc/selfdoc.dl6`. Every number\n'
  printf 'is a row that program derived; edit the program, never this file.\n\n'
  printf 'A module edge is a CALL: a callee names a name and never a file, so a name\n'
  printf 'several files define resolves to all of them. The map over-links.\n\n'
  printf '%s\n' '- [The phase graph](#the-phase-graph)'
  printf '%s\n' '- [parse](#parse)'
  printf '%s\n' '- [plan](#plan)'
  printf '%s\n' '- [lower](#lower)'
  printf '%s\n' '- [emit](#emit)'
  printf '%s\n' '- [driver](#driver)'
  printf '%s\n' '- [harness](#harness)'
  printf '%s\n' '- [unphased](#unphased)'
  printf '%s\n' '- [fixture](#fixture)'
  printf '%s\n' '- [The host roster](#the-host-roster)'
  printf '%s\n' '- [Orphan predicates](#orphan-predicates)'
  printf '%s\n\n' '- [The twenty hottest symbols](#the-twenty-hottest-symbols)'
  printf '## The phase rule\n\n'
  printf 'Every file makes one or more ranked phase CLAIMS and the lowest rank wins.\n\n'
  printf '| rank | claim | phase |\n'
  printf '|---:|---|---|\n'
  printf '| 1 | under `v6/prolog/conformance/` | fixture |\n'
  printf '| 2 | stem ends `.test` | harness |\n'
  printf '| 3 | a `phase_driver` row, from compile.pl:657/819/822/836 | that row |\n'
  printf '| 4 | stem contains `emit` | emit |\n'
  printf '| 5 | stem starts `0_` or `1_` | parse |\n'
  printf '| 5 | stem starts `2_`..`9_` | plan |\n'
  printf '| 6 | any other prolog file | unphased |\n\n'
} >"$COMPILER"
render_phase_board "$COMPILER" parse plan lower emit driver harness
render_board "$COMPILER" parse   'parse'    'Source text to an expanded program: the DCG, `expand_uses/8`, and the `0_`/`1_` expansion passes.'
render_board "$COMPILER" plan    'plan'     'The expanded program to a `plan/9` term: analysis, stratification, host expansion, the checks.'
render_board "$COMPILER" lower   'lower'    'The plan to SQL text. One module, and the largest in the tree.'
render_board "$COMPILER" emit    'emit'     'Lowered SQL to a host language: the two doors plus the type and schema artifact emitters.'
render_board "$COMPILER" driver  'driver'   'The entry points and the tables everything reads.'
render_board "$COMPILER" harness 'harness'  'plunit batteries and the graders. Not part of a compile.'
render_board "$COMPILER" unphased 'unphased' 'Prolog files the rule places nowhere: tools, labs, and one-off scripts.' table
render_board "$COMPILER" fixture 'fixture'  'Conformance fixtures. Independent programs, drawn as a table only.' table
render_host_roster "$COMPILER" 'v6/dl/'
render_orphans "$COMPILER" 'v6/prolog/'
render_symbols "$COMPILER" 'v6/prolog/'

# ── extract.md ──────────────────────────────────────────────────────────────
EXTRACT="$OUT/extract.md"
{
  printf '# sprefa-extract, read by itself\n\n'
  printf 'Generated by `just selfdoc` from `v6/dl/selfdoc/selfdoc.dl6`. Every number\n'
  printf 'is a row that program derived; edit the program, never this file.\n\n'
  printf 'A module edge is a CALL, resolved by NAME. A rust `mod` tree edge with no\n'
  printf 'call across it is not drawn.\n\n'
  printf '%s\n' '- [The phase graph](#the-phase-graph)'
  printf '%s\n' '- [cli](#cli)'
  printf '%s\n' '- [dispatch](#dispatch)'
  printf '%s\n' '- [families](#families)'
  printf '%s\n' '- [resolve](#resolve)'
  printf '%s\n' '- [scip](#scip)'
  printf '%s\n\n' '- [The twenty hottest symbols](#the-twenty-hottest-symbols)'
  printf '## The board rule\n\n'
  printf 'Every file makes one or more ranked board CLAIMS and the lowest rank wins.\n\n'
  printf '| rank | claim | board |\n'
  printf '|---:|---|---|\n'
  printf '| 11 | under `src/bin/` | cli |\n'
  printf '| 11 | under `src/lang/` | families |\n'
  printf '| 12 | basename contains `scip` | scip |\n'
  printf '| 13 | stem is `project`, `deps` or `manifests` | resolve |\n'
  printf '| 15 | any other rust file | dispatch |\n\n'
} >"$EXTRACT"
render_phase_board "$EXTRACT" cli dispatch families resolve scip
render_board "$EXTRACT" cli      'cli'      'Argument parsing and the one binary. Every flag lands in a FamilyMask.'
render_board "$EXTRACT" dispatch 'dispatch' 'One file plus a mask to a family output, then the JSONL wire.'
render_board "$EXTRACT" families 'families' 'One module per language front-end. Each projects a tree-sitter parse into the shared record shapes.'
render_board "$EXTRACT" resolve  'resolve'  'Phase 2: names to files, across a supplied project.'
render_board "$EXTRACT" scip     'scip'     'A SCIP index in, raw rows out. The one plane a compiler resolved.'
render_symbols "$EXTRACT" 'v6/sprefa-extract/'

printf 'wrote %s and %s\n' "$COMPILER" "$EXTRACT"
