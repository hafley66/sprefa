# LANE catarch: record the catalog arc

## First action, non-negotiable

```bash
cd /Users/chrishafley/projects/sprefa-lanes/catarch
git rev-parse HEAD    # MUST print e3997cecd88322ae029255c5e3cc8402e433d122
```

If it prints anything else, STOP and write REPORT.md saying so.

## Files you own. Touch nothing else.

| file | what you do there |
|---|---|
| `v6/prolog/ARCH.pl` | add two `task/3` rows |
| `plans/2026-08-05-catalog-g1.md` | create it |

Two sibling lanes are editing `v6/prolog/{analyze.pl,lower.pl}` and `v6/tsv2/tests/catalogRows.test.ts` right now. Editing either is a defect.

## Do NOT run these

- `npm install`, `pnpm install`. `git commit`, `git push`, `git merge`, `git rebase`.

## Task 1: two rows in `v6/prolog/ARCH.pl`

Read the last twenty `task(` lines in that file first, around line 890 to 900. Copy their exact shape: `task(Name, Status, Needs).` followed by a `%` comment on the SAME line carrying the receipts. Statuses in use are `done`, `unbuilt`, `labbed`.

Add these two, placed with the other recent rows:

```prolog
task(catalog_g1_producer, unbuilt, []).
task(catalog_g2_oracle_parity, unbuilt, [catalog_g1_producer]).
```

Write each one's trailing comment yourself from the facts below. Keep the register of the neighbouring rows: dense, receipt-bearing, one line, no dates invented.

Facts for `catalog_g1_producer`:
- Decision record is `ruling(catalog_universe, ...)` at `v6/prolog/conformance/rulings.pl:613`: catalog rows describe user rel declarations, come from the compiler's `relplan/5` decl table, and are materialized into the COMPILED PROGRAM database through the same door `__tick` uses. The store-spine home was rejected because the fact plane and a compiled program are separate SQLite databases with no ATTACH anywhere.
- Scaffold landed in `e3997cec`: `catalog_ddl_contract/2` plus two stubs returning `[]` plus the wired call site in `lower_program/2` at `lower.pl`.
- Shape is one table, `__catalog_rel(rel_id, parent_id, ordinal, local_name, kind, type_id)`, `kind` in `{primitive, rel, column}`. A column is a child row of its rel, so it can carry a type and an annotation exactly the way a rel can.
- Bill: 3 DDL statements per catalog-using program (CREATE TABLE, CREATE INDEX, one INSERT OR IGNORE carrying every row). Measured across the 212 emitted modules, catalog rows per module run 7 / 12 / 225 (min / median / max) including the five primitives, and the seed adds 8.4% / 14.6% / 29.4% to the module's existing `ddl` array, which itself runs 714 / 2578 / 80198 bytes.
- Gated on `program_uses_catalog/2`, mirroring `program_uses_tick/2` at `analyze.pl:180`, so all 212 tracked emitted modules stay byte-identical.

Facts for `catalog_g2_oracle_parity`:
- `conformance/ticklog.pl` needs the same seed only once a FIXTURE derives from a catalog row. A DDL-time seed emits no delta at any tick, so g1 alone cannot diverge from the oracle. The first fixture whose rule reads a catalog row emits deltas the oracle never produces.

## Task 2: `plans/2026-08-05-catalog-g1.md`

A plan document for an auditor. Every claim carries a receipt: a `path:line`, a symbol name, or a command with its output. Structure:

1. A real table of contents at the top. Not a bullet list of section names, a table with a column saying what each section answers.
2. The decision, compressed, citing `rulings.pl:613`.
3. The shape, as a table of the six columns with what each means on a rel row and on a column row.
4. The DDL bill, as the numbers given above.
5. The three later steps that add ROWS rather than statements: module nesting via `parent_id`, generic instantiation via `parent_id` pointing at the generic, column types via `type_id`. Plus the one future statement, `__catalog_annotation(target_id, name, value)` for decorators.
6. Prior art, two rows: TypeScript 7's checker gives every type one struct with an id, a kind mask and a variant pointer (`microsoft/typescript-go`, `internal/checker/types.go:673`), and Go seeds its primitives as ordinary objects in the `Universe` scope (`go/types`, `universe.go:78`, `defPredeclaredTypes`). Note that TypeScript's `boolean` is a union of two literal types rather than an intrinsic, in both the JS checker and the Go port.
7. What is deliberately out of g1: dot access over rel names, module nesting, host-fed rows, oracle parity for catalog reads, and generic instantiation.

Tables and diagrams carry the content. Prose is a one-line caption under a diagram, never the medium. Mermaid for any topology. Length is a cost.

## Validation

```bash
cd /Users/chrishafley/projects/sprefa-lanes/catarch/v6/prolog
swipl -g go -t halt ARCH.pl      # expect: exit 0, the two structural claims check
swipl -q -l ARCH.pl -g roadmap -g halt | grep catalog   # expect: both new rows print with status unbuilt
```

`go` proves the build-order graph stays acyclic and total. A cycle means your `Needs` list is wrong.

## Style laws. Violations are defects.

- No em dashes anywhere.
- Banned words in prose and identifiers: provenance, substrate, load-bearing, regime, support, honest, distill, ruling, and "ground" as a verb. Say source, base, critical, mode, refCount, decision, verified.
- No deictic filler: never "here is", "below is", "the following", "as follows". The next words are the location.
- No negative parallelism: never "not X, Y" or "X. Not Y." State the positive claim.
- No one-word sentences.
- Comments in `ARCH.pl` ride on the same line as the fact, matching every neighbouring row.
- Variable names descriptive, never single letters.

## If reality deviates from this brief

STOP and write `REPORT.md` naming the contradiction. In particular, if `swipl -g go -t halt ARCH.pl` fails on the base commit BEFORE your edit, say so and stop; that is a pre-existing break and not yours to fix.

## Deliverable

`REPORT.md` at the worktree root: both validation outputs verbatim, the two task rows you wrote quoted in full, and any deviation. Leave the work uncommitted.
