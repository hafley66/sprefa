# SPEC: plan the catalog, next part

You are one of three planners answering this same spec independently. You will be
graded on how efficiently your output presents its answer. Do not look for the
other two, do not coordinate.

## What is decided already, and is not yours to relitigate

`v6/prolog/conformance/rulings.pl:613`, `ruling(catalog_universe, user_rel_decls_in_program_db, ...)`.
Read it in full before anything else. Its content, compressed:

- Catalog rows describe USER-PROGRAM rel declarations.
- They are produced from the compiler's decl table (`relplan/5`).
- They are materialized into the COMPILED PROGRAM database, through the same door
  the `__tick` table uses.
- The store-spine alternative was rejected: the v6/dl fact plane and a compiled
  program are separate SQLite databases with no ATTACH anywhere, so spine rows are
  unreachable by the user rules the catalog exists to serve.
- Dot access over rels resolves against these rows.
- Hosts may feed catalog rows where a producer outside the compiler is natural.

If your reading of the code contradicts any of that, SAY SO with the receipt. That
is the one thing that outranks the decision.

## Your three questions, in order

1. **How does it work today.** Read the code and state the mechanism as it exists,
   including the DIRECTIONS: which way each piece of data moves, who writes and who
   reads, and where a direction is missing today. At minimum cover the
   compiler-to-program-db direction and the user-rule-read direction. Say plainly
   which parts exist and which are empty slots.
2. **Explain it back.** Your reader is a competent engineer with zero context on
   this file set. One pass, no backtracking, no discovery-order narration.
3. **Plan the next part.** What is the next buildable increment, why that one, what
   it touches, how it is proven, and what it deliberately leaves out.

## Verified anchors (line numbers checked 2026-08-05; re-find by symbol if stale)

| path | what is there |
|---|---|
| `v6/prolog/conformance/rulings.pl:613` | the catalog decision |
| `v6/prolog/emit_ts.pl:660`, `:670` | `rel_columns_entry_line/2`, `rel_column_types_entry_line/2` over `relplan/5` |
| `v6/prolog/lower.pl` `tick_table_ddl/1`, `tick_column_sql/1` | the door the decision points at |
| `v6/tsv2/runtime/scratchStore.ts:1-11` | the separate-databases receipt |
| `plans/2026-08-03-module-catalog-ruling.md` | the earlier catalog decision doc, 145 lines |
| `~/projects/sprefa-lanes/typeirplan/PLAN.md` section 7 | an EXISTING step-g design (`__catalog_rel` / `__catalog_instance`). Read it. Agree or disagree with receipts; do not silently duplicate it |
| `v6/prolog/LANG.md` | the language surface: struct, enum, rel, mods |

Also true and worth checking rather than assuming: dl6 has no import mechanism
today. Across `v6/prolog/compile/dl_view/*.dl6` the only top-level forms are `rel`
and `sh`.

## Deliverables, exactly two files, at your worktree root

1. `PLAN.md` — for an auditor. Every claim carries a receipt: `path:line`, a
   symbol name, or a command with its output. TOC first.
2. `PLAN.visual.human.unga.md` — for Chris. Plain words, diagrams, ZERO citations,
   zero file paths in prose. It must stand alone.

## How you are graded

Efficiency of presentation, above all. Concretely:

- Tables, mermaid diagrams, and a real TOC carry the content. Prose is a one-line
  caption under a diagram, never the medium.
- One focus per section. Priority order, never discovery order.
- Length is a cost. A shorter document that answers all three questions beats a
  longer one that answers them plus decoration.
- A claim without a receipt in `PLAN.md` is a defect.
- A citation in the unga doc is a defect.

## Style laws, non-negotiable

- No em dashes.
- Banned words in prose and identifiers: provenance, substrate, load-bearing,
  regime, support, honest, distill, ground as a verb, ruling. Say source, base,
  critical, mode, refCount, decision.
- No deictic filler: never "here is", "below is", "the following", "as follows".
- No negative parallelism: never "not X, Y" or "X. Not Y."
- No one-word sentences.
- Construct names use rxjs, prolog, or SQL vocabulary only.
- Descriptive variable names in every snippet, never single letters.

## Rules of engagement

- READ ONLY. Change no code, run no migrations, commit nothing, push nothing.
- Running read-only commands to check a claim is expected and encouraged.
- Work only inside your own worktree.
- If reality deviates from this spec, say so in `PLAN.md` under a heading
  `Where the spec was wrong` and continue. Do not stop for it.
- One pass. Do not ask questions back; you have everything you need.
