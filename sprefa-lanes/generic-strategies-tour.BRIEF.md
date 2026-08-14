# BRIEF: how real compilers handle generic instantiation and interface dispatch

Docs only. Zero code. A survey written to be READ, not to be exhaustive.

## Base
Confirm the base with `git log --oneline -1` before your first commit.

## The question

This project is designing generic rels that MONOMORPHIZE: `pair(int)` and
`pair(text)` become two separate SQLite tables. Measured projection from the
pass 2 research: 6 templates times 8 instantiations times 2 relations per
instance is 96 generated relations.

Every serious language has faced this exact trade and answered it differently.
Survey those answers so the user can choose with the field in view.

**Tour these five, in this order:**

| language | why it is on the list |
|---|---|
| Rust | full monomorphization, the ceiling; pays in compile time and binary size |
| Go | generics arrived in 1.18 with GC shape stenciling plus dictionaries, a deliberate middle path; also has structural interfaces with itab dispatch |
| TypeScript, specifically the **Go port** (`typescript-go` / tsgo) | full erasure at runtime, and the Go rewrite is a live natural experiment in porting a type checker; say what the port changed and what it could not |
| Kotlin | JVM erasure, plus `reified` on inline functions as a targeted escape hatch |
| Python | no runtime generics at all; `typing` is checked externally and erased |

For each, answer plainly:
- what happens to a generic at compile time: instantiated, erased, or something in between
- what the runtime representation is
- how interface or trait dispatch works: static, vtable, itab, duck typing
- the measured or documented COST of the choice, in compile time, artifact size,
  or runtime indirection. Cite real numbers where the language's own team has
  published them.
- what the language's designers said they were buying. Quote them where you can
  find it; Go's generics design documents and Rust's monomorphization discussions
  are both public.

## The part that makes this useful rather than trivia

Our "runtime artifact" is SQLite tables, so the analogy shifts:

| their world | our world |
|---|---|
| generated machine code | generated tables and SQL |
| binary size | table count and schema size |
| compile time | compile time, same |
| vtable indirection | a discriminator column plus a join |

Two limits measured on this machine today, sqlite 3.53.2:

| limit | measured |
|---|---|
| tables in one join | **64, a hard error at 65** |
| tables in a schema | 20,000 created in 7.8 s, schema scan unmeasurably fast |

So the table COUNT is a non-issue and the JOIN WIDTH is the real ceiling. A
monomorphized program that expands one rule into a join across more than 64
generated relations fails outright. Say whether that is reachable in practice
and what the escape looks like.

The Go answer, dictionaries plus shape stenciling, has a direct analogue here: a
shared table with a type discriminator column instead of one table per
instantiation. That is the "runtime type tag" shape the pass 2 doc rejected on
comptime grounds. Present it fairly anyway, with its cost, because the join
limit is an argument for it that did not exist when it was rejected.

## Deliverable

One doc: `plans/2026-08-12-generic-strategies-tour.RESEARCH.md`, opening with a
table of contents, plus a short `.visual.human.unga.md` twin in plain words.

Structure: one section per language, then a comparison table, then a section
mapping each strategy onto tables-and-SQL, then a recommendation on where this
project should sit on the spectrum given the 64-table join limit.

Read `plans/2026-08-12-typespec-module-ir.RESEARCH.md` sections "Pass 2: generic
rel arguments" and "Wrapper prize" first, so you extend that thinking rather
than restating it. It already establishes that `0_generic_expand.pl` (348 lines)
monomorphizes four internal wrapper constructors today.

## Anti-cheat
- cite the compiler's own documentation or source, not blog summaries
- give numbers where the language's team published them; say "not published"
  otherwise rather than inventing a figure
- do not recommend before the survey; the recommendation is the last section
- the unga twin is required

## File ownership
YOURS: the two new plan docs only. Everything else READ ONLY.

## Style laws
- No em dashes. Banned in prose and identifiers: `provenance`, `substrate`,
  `load-bearing`, `regime`.
- "refusal" banned in prose; say TODO or not built yet.
- No sycophancy, no negative parallelism ("not X, Y").
- Tables and lists over prose. Docs open with a table of contents.

## Worktree setup, before your first commit
```
(cd v6/sprefa-extract && cargo build --release --features cli --bin extract)
(cd v6/tsv2 && pnpm install)
(cd v6/sprefa-store/js && pnpm install)
```
