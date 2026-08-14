# BRIEF: the symbol naming layer, and implements as a relationship

Docs only. Zero code. Keep it proportionate: this is one section appended to an
existing doc, not a new research arc. The user's instruction was "dont be too
intense".

## Base
Confirm the base with `git log --oneline -1` before your first commit.

## What the user decided

Two of four SCIP forks are adopted, A and B. Read
`plans/2026-08-12-scip-as-ir.RESEARCH.md` "Deliverable 3" for the full pricing;
do not re-derive it.

- **A. Adopt the symbol/descriptor MODEL, none of the SCIP wire format.** An
  interned symbol table with descriptor columns. Descriptors are nested naming:
  package, then type, then method, then parameter.
- **B. Adopt relationship kinds only.** `is_implementation`,
  `is_type_definition` and friends as typed rows, ignoring everything else SCIP
  carries.

Fork C (narrowed on-demand SCIP for inferred types) and fork D (do nothing) are
NOT adopted. Do not design for them.

## Why these two, so you keep the scope right

A is the collision-free naming layer that codegen needs. The type IR polyglot
plan flagged this exact step as designed and unbuilt at
`plans/2026-08-12-type-ir-polyglot.PLAN.md:120`: "allocate names, module path,
local relation name, field names", with a collision table covering case folding
and reserved words. Descriptors are the principled version of that step.

B closes one specific gap: the type IR has no notion of one type implementing
another, which is the interface work now being designed in
`plans/2026-08-12-typespec-module-ir.RESEARCH.md`. SCIP already has a spelling
for it, so borrow the spelling rather than minting one.

## Deliverable

ONE new section appended to `plans/2026-08-12-typespec-module-ir.RESEARCH.md`,
plus a short matching section in its `.visual.human.unga.md` twin. Answer:

1. what the interned symbol table looks like here. Surrogate INTEGER id, the
   natural key UNIQUE in a dictionary table, descriptor parts as columns. Follow
   the pattern the boop store already uses (`dict_*` tables). Read
   `.claude/skills/sql-relational-design` first.
2. which existing rels currently carry a TEXT symbol and would be re-keyed.
   `type_entity.sym` is one, cited at `src/engine/decls.rs:522`. Find the rest
   and count them.
3. how the descriptor columns feed the naming and collision step at
   `type-ir-polyglot.PLAN.md:120`. This is the point of fork A; make the
   connection concrete with one worked example of two same-named types from
   different modules landing in one emitted TypeScript file.
4. the relationship rel for fork B: its shape, its kinds, and how a `.dl6`
   `is <interface>` conformance clause would produce a row in it.
5. one trap, already measured and stated in the SCIP research: a syntactic
   scanner fills these kinds from `implements`/`extends` clauses and misses the
   inferred-type case. Say plainly what our rows can and cannot be trusted to
   mean, so nobody over-trusts them later.

Prices where you can give them. No fork selection; A and B are already chosen.

## Scope fence
Do NOT design SCIP ingestion. Do NOT propose storing a SCIP index. The wire
format is explicitly out. This is about borrowing two modelling ideas.

## File ownership
YOURS: `plans/2026-08-12-typespec-module-ir.RESEARCH.md` and its
`.visual.human.unga.md` twin. Everything else READ ONLY.

## Style laws
- No em dashes. Banned in prose and identifiers: `provenance`, `substrate`,
  `load-bearing`, `regime`.
- "refusal" is banned in prose; say TODO or not built yet.
- No sycophancy, no negative parallelism ("not X, Y").
- Tables and lists over prose.

## Worktree setup, before your first commit
```
(cd v6/sprefa-extract && cargo build --release --features cli --bin extract)
(cd v6/tsv2 && pnpm install)
(cd v6/sprefa-store/js && pnpm install)
```
