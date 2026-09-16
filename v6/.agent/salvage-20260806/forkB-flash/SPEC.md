# SPEC: the dotted-head fork

You are one of four planners answering this spec. Two of you answer branch A, two
answer branch B. Do not look for the others, do not coordinate. You are graded on
whether your branch would survive contact with the code, and on how efficiently
your output presents its answer.

Base commit: `31bf4af13cbd791d625558e8c37a11b9795bc8d8`. Verify with
`git rev-parse HEAD` as your FIRST action; anything else means STOP and say so in
`PLAN.md`.

## What is already decided and is not yours to relitigate

`v6/prolog/conformance/rulings.pl:609`, and
`plans/2026-08-03-module-catalog-ruling.md`. Read both in full before anything
else. Compressed:

- A "module" is not a kind. A module is a `rel/0` with children (module-catalog
  rule 7). The catalog knows only `rel` and `column`. v1 nests under `rel/0`
  only; nesting under `rel/N` is the reserved future generalization.
- Children lower to FLAT rels with long mangled names, `a__b__c__<digest>`
  (M5), plus catalog rows relating them. Existing flat rels are root children
  and keep their bare names, zero migration.
- An outer argument a block captures is implicitly distributed into every child
  rel as a leading demand-key column (M1, scalar data-driven becomes leading
  columns, magic set). Scalar static monomorphizes instead.
- The brace surface is sugar over that lowering and arrives in a later wave. A
  FILE is the degenerate first block already.
- Refusals already named and unbuilt: `module_name_collision`,
  `container_and_leaf`, `non_static_rel_arg`, `growing_instantiation_cycle`,
  `unresolvable_path`.

If your reading of the code contradicts any of that, SAY SO with the receipt.
That is the one thing that outranks the decision.

## The fork you are designing

Module-catalog rule 8 says a rule head may be a dotted path, `a.b(x) <- ...`,
contributing rules to a nested rel from outside its block, and that multiple
files contributing to one rel is ordinary datalog union. It leaves ONE thing
open, marked amendable:

- **Branch A, contribute-only.** A dotted head CONTRIBUTES to a rel that the
  path's home block declares. It does not CREATE new paths from outside. A file
  must declare a module's shape before another file can add rules to it.
- **Branch B, create-on-write.** A dotted head CREATES the path if it is absent.
  Any file can grow any module. No declaration is required first.

Your branch is named in `BRANCH.md` at your worktree root. Design THAT branch.
Do not argue for the other one; assume the decision went your way and make your
branch work. Naming a specific cost of your own branch is expected and earns
credit, and a plan that hides one loses it.

## The three questions, in order

1. **What breaks today.** Read the code and state, with receipts, exactly what a
   dotted head does now, where it is refused, and every site that would need to
   change for your branch. Cover at minimum: the dot phase, ref collection,
   table naming, the catalog seed, and the refusal set.
2. **The design.** Type signatures first, then a pseudo-code body as a comment
   under each signature, then storage layout, then the sequence of reads and
   writes, then the uniqueness conditions. Those four layers are allowed to
   disagree with each other; say so where they do.
3. **The proof.** What is the fail-first test, what is the sabotage receipt, and
   which existing gate would catch a regression. Name the exact commands.

## Verified anchors (checked 2026-08-06 by the coordinator; re-find by symbol if stale)

| path | what is there |
|---|---|
| `v6/prolog/0_dot_expand.pl:29` | the comment "There is no module half in scope" |
| `v6/prolog/0_dot_expand.pl:169`, `:176` | the two `unresolvable_member` throws; an ATOM root is refused by construction |
| `v6/prolog/compile.pl:157` | `sort(AllRefs0, AllRefs)`, the ref inventory whose ORDER fixes catalog ids |
| `v6/prolog/compile.pl:175` | `subtract(AllRefs, [CatalogName/CatalogArity \| DerivedRefs], ArrivalTargets)` |
| `v6/prolog/compile.pl:121` | `materialize_reference_target_rels/2`, the existing decl-injection pattern |
| `v6/prolog/compile.pl:131` | `materialize_catalog_rel/2`, the same pattern applied to the catalog |
| `v6/prolog/lower.pl:162` | `table_name(Name/_Arity, Name).` The table name IS the rel name and the ARITY IS DROPPED |
| `v6/prolog/lower.pl:630` | `catalog_ddl_contract/2`, the six catalog columns |
| `v6/prolog/lower.pl:637` | `catalog_table_ddl/1`, the child-walk index |
| `v6/prolog/lower.pl:643` | `catalog_row_ddl/3`, the positional-id seed |
| `v6/prolog/analyze.pl:190` | `program_uses_catalog/2` and its arity-6 gate |
| `v6/prolog/compile/test/plunit_tests.pl` | the `catalog_g1` test group, 6 tests |
| `v6/tsv2/serve/4_http.ts:156` | `ScratchStore.open(config.dbUrl)`, ONE database for the whole server |
| `v6/tsv2/serve/3_engine.ts:229-241` | `bootServedProgram` replays a program's DDL and swallows "already exists" |

## Measurements the coordinator already took. Do not redo them; build on them.

- The catalog shipped at `31bf4af1`. `__catalog_rel(rel_id, parent_id, ordinal,
  local_name, kind, type_id)`, seeded by DDL, queryable by a rule, with
  `EXPLAIN QUERY PLAN` reporting `SEARCH __catalog_rel USING COVERING INDEX
  __catalog_rel_parent`. Every `parent_id` written so far is 0 or a rel's own id
  for its columns; no module nesting is written by anything.
- Across the fixture corpus: `fixtures=302 refs=1074 same_name_two_arities=0`.
  So `lower.pl:162` dropping arity has never fired in practice. It stays a live
  hazard: one program with `edge/2` and `edge/3` emits two
  `CREATE TABLE "edge"` statements with different columns.
- Two programs booted into one server database collide on catalog ids, because
  ids are positional per compile and the server shares one connection. Demoed:
  `rel_id 6` existing twice as `alpha` and `beta`, and a `parent_id = 6` child
  walk returning both programs' columns.
- dl6 has no import mechanism. Across `v6/prolog/compile/dl_view/*.dl6` the only
  top-level forms are `rel` and `sh`.

## Deliverables, exactly three files at your worktree root

1. `PLAN.md` for an auditor. Every claim carries a receipt: a `path:line`, a
   symbol name, or a command with its output. A real table of contents first,
   as a table saying what each section answers.
2. `PLAN.visual.human.unga.md` for Chris. Plain words, ascii or mermaid
   diagrams, ZERO citations, zero file paths in prose. It must stand alone and
   be readable by someone who has never seen this repo.
3. `COST.md`, at most one page: the specific thing your own branch makes worse,
   the case where a user would be surprised, and what you would need to see
   before shipping it.

## Rules of engagement

- READ ONLY. Change no code, run no migrations, commit nothing, push nothing.
  Running read-only commands to check a claim is expected and encouraged.
- `node_modules` is ABSENT in this worktree. Do not run any package manager. No
  JavaScript needs to run for this task.
- `swipl` works. `swipl -q -l <file> -g <goal> -g halt` is the way to check a
  prolog claim, and quoting through a shell `-g` string mangles `<-` operators,
  so write a scratch `.pl` file with `:- op(1150, xfx, <-).` at the top instead.
- Work only inside your own worktree.
- One pass. Do not ask questions back; you have everything you need.
- If reality deviates from this spec, say so in `PLAN.md` under a heading
  `Where the spec was wrong` and continue rather than stopping.

## Style laws, non-negotiable

- No em dashes.
- Banned words in prose and identifiers: provenance, substrate, load-bearing,
  regime, support, honest, distill, ruling, and "ground" as a verb. Say source,
  base, critical, mode, refCount, decision, verified.
- No deictic filler: never "here is", "below is", "the following", "as follows".
- No negative parallelism: never "not X, Y" or "X. Not Y." State the positive claim.
- No one-word sentences.
- Construct names use rxjs, prolog, or SQL vocabulary only.
- Descriptive variable names in every snippet, never single letters.
- Tables and diagrams carry the content. Prose is a one-line caption under a
  diagram. Length is a cost.
