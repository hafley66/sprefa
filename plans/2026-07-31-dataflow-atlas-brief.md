# The dataflow atlas — brief (opus worktree)

User demand, verbatim spirit: "the largest dataflow diagram you can make
with auto extraction facts and manually joining across gaps that are
bespoke across langs ... the longest dataflow graph we can muster of the
compiler being called from cli ... everything linked or mentioning
sprefa-extract ... in a dl6 file and the graphviz rendered. Prove this
tool is even worth it."

This is a PROOF-OF-WORTH lane. The deliverable is one dl6 program +
one rendered SVG. Graphviz IS installed (`/opt/homebrew/bin/dot`,
version 15.1). The self-map rail (v6/dl/fixtures/self-map.dl6) is the
architectural template: bind watch or one-shot demand rows -> sh fact
hosts -> derived rels -> group_concat document assembly -> write host.
Copy its shape, do not reinvent it.

## The chain to draw (the longest true dataflow path)

`bop run x.dl6` (commander, cli/bop.ts) -> serveTsv2 in-process ->
gen_served compile door -> compile_dl6.sh / compile.pl (swipl) ->
parse_dl.pl -> expansion phases (1_expansion.pl) -> analyze.pl ->
lower.pl -> emit_ts.pl -> the emitted module .ts -> tsv2 runtime
(SqlRunner seam, 3_runtime) -> sqlite statements -> tick log ->
/ticks + stdout. Every arrow that exists in code must come from a
FACT; every arrow that crosses a language is a NAMED manual join rule.

## Fact planes (auto-extracted, one sh host family per language)

1. **TypeScript**: the fixed extractor (`target/release/extract`,
   families cst,type,call,df — the extraction-live pattern, demand rows
   per (path, digest)) over v6/tsv2/cli, v6/tsv2/serve, v6/tsv2/runtime,
   and ONE representative gen_emitted module. Projections: defs, calls,
   imports.
2. **Prolog**: swipl's own cross-referencer. Write a small helper
   (v6/prolog/tools/xref_facts.pl, analogous to self_map_facts.pl) that
   runs library(prolog_xref) over the compile/ tree and emits JSONL:
   `pred_defined(file, name/arity)`, `pred_called(file, caller, callee)`.
   This is the bespoke-per-lang extractor the user asked for; it is a
   FACT SOURCE, wired as an sh host exactly like sm_surface.
3. **Shell**: a regex host over v6/tsv2/scripts/*.sh + compile_dl6.sh +
   justfile recipes in scope: `script_invokes_swipl(script, goal)`,
   `script_invokes_node(script, entry)`.
4. **SQL**: a regex host over the ONE gen_emitted module:
   `stmt_touches(module, verb, table)` (CREATE/INSERT/SELECT/DELETE x
   table name). Cheap and honest — say in the doc it is regex-shaped.

## Manual bridge joins (the cross-language gaps, each one a named rule)

- commander subcommand string -> its action function (TS call facts).
- bop run -> serveTsv2 (TS calls), serve compile door -> the shell
  template that invokes swipl (string match on `compile_dl6`).
- shell `-g goal` text -> prolog predicate (join script_invokes_swipl
  against pred_defined).
- prolog emit_ts predicates -> the emitted module file (emit_ts writes
  it; join on the gen_served/gen_emitted naming convention).
- emitted module imports -> runtime symbols (TS import facts).
- runtime execute seam -> sqlite tables (stmt_touches).
Each bridge rule carries a comment naming WHY the join is bespoke
(what fact neither extractor can see alone).

## The sprefa-extract cluster

Everything that links or mentions the extractor joins in: the bin
itself, its CLI flags (regex over extract.rs's arg parsing or its
--help output via an sh host), every caller (4_ingest.ts, the
extraction hosts' templates, flagship scripts), and the JSONL record
kinds it emits. One graphviz cluster.

## Output

- `v6/dl/fixtures/dataflow-atlas.dl6` derives nodes + edges, assembles
  DOT text (digraph, one cluster per language, the sprefa-extract
  cluster, rankdir=TB — NEVER LR, the user's viewer chokes on
  horizontal strips), writes `v6/DATAFLOW-ATLAS.dot` via a write host,
  and a second sh host runs `dot -Tsvg` -> `v6/DATAFLOW-ATLAS.svg`.
- Longest path: attempt a recursive depth rel (max aggregate over a
  recursive stratum). If the compiler refuses the shape, RECORD the
  named refusal in the atlas doc header and compute the longest path in
  the DOT-emitting rule chain instead (or annotate via `dot -Tplain`).
  A refusal here is a language finding, not a failure of the lane.
- A short `v6/DATAFLOW-ATLAS.md`: node/edge counts per plane, the
  longest path spelled out hop by hop, every bridge rule listed with
  its reason, every named refusal hit.
- justfile recipe `atlas` (regen + byte-stable second run like
  self-map). NOT in green-all yet.

## Receipts

- `just atlas` twice byte-identical (digest the .dot, not the .svg —
  graphviz embeds nothing nondeterministic but do not bet the gate).
- Sabotage: delete one bridge rule in a scratch copy -> the rendered
  graph loses the cross-language edge and the longest path shortens;
  receipt in the script header.
- The battery members your files touch: conformance, TEXT_DOOR,
  roundtrip if fixtures change (the atlas program itself is a rail like
  self-map, not oracle-graded — same trade, state it in the header).
- Counts stated for everything.

## Fences

- Worktree law: first action `git merge --ff-only e3f5064f`; on failure
  STOP AND REPORT.
- Touch: the new .dl6 + tools/xref_facts.pl + scripts/atlas.sh +
  justfile recipe + the two generated artifacts. Do NOT touch:
  bench-cli/**, watch/bind seams (2_binds.ts), the compiler beyond
  read, v5 src/**, sprefa-extract sources (the extractor is FIXED —
  read-only use).
- Host spellings: use `files`/`files_at` (renamed this morning; the old
  enumerate words are gone).
- pnpm install per package; never symlink outer node_modules.
- Style: no em dashes; descriptive variable names; rx/prolog/SQL words
  only; banned words per CLAUDE.md.
- Commit per step `git commit -n`; no push.
