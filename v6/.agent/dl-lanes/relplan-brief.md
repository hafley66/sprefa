# Lane relplan -- step B2 of plans/2026-08-07-dynamic-loading.md

## First action, non-negotiable
`git merge --ff-only <BASE_SHA>` in your worktree. Failure or a missing tree =
STOP AND REPORT. Do not work around a blocked command (no tar, no --no-verify,
no copying). Never spawn a subagent; fan-out is the coordinator's call.

BASE_SHA: 1ca2f5ff

## What you own (nobody else touches these)
| file | state |
|---|---|
| `v6/tsv2/serve/reloadPlan.ts` | NEW, you create it |
| `v6/tsv2/tests/reloadPlan.test.ts` | NEW, you create it |

Read-only for you, already landed on your base: `v6/tsv2/runtime/types.ts`
(`IRelCatalogRow` at the `IGenProgram` block, `relCatalog` on `IServedProgram`),
`v6/prolog/lower.pl:639-763` (the catalog contract and the four hashes),
`v6/tsv2/gen_emitted/golden-flex.ts` (search `const relCatalog` for a real row
list). Do NOT edit `3_engine.ts` or `4_http.ts`; wiring is step B3, coordinator's.

## The receipts you are building against
- `__rel` has 11 columns, `PRIMARY KEY` over all 11, `WITHOUT ROWID`, plus index
  `__rel_parent (parent_id, local_name)`: `v6/prolog/lower.pl:639` +
  `v6/tsv2/tests/catalogRows.test.ts:57`; the contract itself is `lower.pl:639`.
- Every emitted module now exports `relCatalog`: `v6/prolog/emit_ts.pl:701`
  `rel_catalog_lines/2`, one object per `row/11`.
- `hId` is `rel_h_id/4` = sha256(ParentHash/LocalName/Arity) truncated to 16 hex;
  `hSchema` is `schema_hash/4` over (Columns, ColumnTypes, KeyOrNone); `hRule` is
  `rule_hash/3` over the sorted rule bodies, and it is the EMPTY STRING for a
  source rel with no derivation (`lower.pl:679`).
- Primitives sit at reserved rel ids 1..5 by position (`lower.pl:721-727`), the
  module row is id 6.

## The code to write
```ts
export type RelVerdict = "create" | "recreate" | "refill" | "keep" | "drop";
export interface IReloadPlan {
  readonly verdicts: ReadonlyMap<string, RelVerdict>;  // key = hId
  readonly statements: readonly string[];
  readonly refusals: readonly string[];
}
export interface IReloadPlanner {
  plan(prev: readonly IRelCatalogRow[], next: readonly IRelCatalogRow[], allowDrop: boolean): IReloadPlan;
}
```
Pure function over two arrays. Index BOTH sides by `hId`, over `kind === "rel"`
rows ONLY (skip primitive/module/column rows). Per hId:

| prev | next | verdict | statements |
|---|---|---|---|
| absent | present | `create` | none from you: B3 replays `program.ddl` for CREATE text |
| present | present, `hSchema` differs | `recreate` | `DROP TABLE "<localName>"` |
| present | present, `hSchema` equal, `hRule` differs | `refill` | `DELETE FROM "<localName>"` |
| present | present, both equal | `keep` | none |
| present | absent, `allowDrop` true | `drop` | `DROP TABLE "<localName>"` |
| present | absent, `allowDrop` false | no verdict | refusal `rel_drop_needs_allow_drop(<localName>)` |

`refusals` non-empty means the caller answers 400 and the running program keeps
turning, so `plan` NEVER throws for a refusable case. A no-change reload MUST
give `statements.length === 0`.

## The tests to write (the MIN/MAX matrix, one test name each, exactly these)
`cold boot is all create` (prev `[]`), `an unchanged program keeps everything`
(+ assert `statements.length === 0`), `a column added recreates`,
`a column dropped recreates`, `a type change recreates`, `a key change
recreates`, `a rule body change refills`, `a new rel creates`,
`a drop without allow-drop is refused by name`, `a drop with allow-drop drops`,
`a reshape and a rule change in one load` (one `recreate` AND one `refill`).

Build row fixtures as literal `IRelCatalogRow` objects in the test file: hand-set
`hSchema` / `hRule` strings, do not shell out to swipl. Style: `node:test` +
`node:assert/strict`, the shape `v6/tsv2/tests/catalogRows.test.ts` already uses.
Write a SABOTAGE RECEIPT block in the test header: name the one-line edit to
`reloadPlan.ts` that turns each family red, and RUN it before writing the header.

## Style laws, inline, all mandatory
- Comments state ONLY constraints the code cannot show. Max 2 consecutive
  comment lines in new code (a hook enforces it and will block your edit). No
  change-log narrative, no dates, no restating the next line. TEST-header
  sabotage receipts are the one exception.
- Every new class/important function is interface-bound: declare `IReloadPlanner`
  in the package header types (`v6/tsv2/runtime/types.ts` is coordinator-owned,
  so declare it at the top of `reloadPlan.ts` and the coordinator lifts it), then
  `export const ReloadPlanner: IReloadPlanner = { plan(...) {...} }`. No bare
  `export function`.
- Interfaces carry the `I` prefix. Type names say what the thing is.
- `plan` is SYNCHRONOUS: in-memory list work is plain array code returning
  arrays. No Promise, no async, no Observable, no `.subscribe()`.
- Banned words in prose AND identifiers: provenance, substrate, load-bearing,
  regime. Also banned: "support" (say refCount). No em dashes.
- Surrogate-key law: never propose a composite TEXT primary key. `__rel`'s
  all-11-column PK is existing shipped DDL, not yours to change.

## Your gate, run it and paste the output in your report
```
cd v6 && just typecheck && just tsv2-test
```
Both must be green. `just tsv2-test` expects 156+ pass / 1 skip. If a leg is red
on arrival (before you edit anything), REPORT THAT FIRST and stop.

## Pass 1 of 2, and the rules around the edges
- This is PASS 1 OF 2. A named second pass follows (style/dead-code/receipt
  sweep, then a coordinator design review), so FAVOR PLAIN CODE over clever
  code and do not pre-optimize anything.
- PACKAGE MANAGER IS pnpm. node_modules is ALREADY INSTALLED in your worktree.
  Never run `npm install` (it rewrites the lockfile and un-dedupes types) and
  never run `pnpm install` either; if a package is genuinely missing, STOP AND
  REPORT.
- IF REALITY DEVIATES FROM THIS BRIEF, STOP AND REPORT. Do not improvise, do
  not fix an adjacent thing you noticed, do not widen your file ownership.
  A wrong premise in this brief is the single most useful thing you can find.
- DO NOT COMMIT. Leave the work in the worktree.
- Deliverable contract: `REPORT.md` at your worktree root, in the report format
  below. Write it even if you stopped early; especially then.

## Report format
One table: file, lines added, test names passing. Then the gate output verbatim.
Then any refusal you could not express and why. No prose narrative.
