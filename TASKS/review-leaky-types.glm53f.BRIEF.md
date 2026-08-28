# Brief: review for leaky enums, traits and structs (report only, NO code)

Read `CLAUDE.md` and `AGENTS.md` in full first. Chris's standing rule from
this week: "each language as its own impl across the board; never make match
arms per lang, they will never have anything to do with each other." Generalise
that eye to every enum, trait and struct in the Rust crates under `v6/`.

## First action
```bash
git merge --ff-only 3dd679c93   # STOP AND REPORT on failure
```

## Scope
Crates: `v6/sprefa-extract`, `v6/sprefa-engine-rs`, `v6/sprefa-store` (Rust only). Read-only review. You write TWO docs and nothing else:
- `plans/2026-08-27-leaky-types-review.PLAN.md` (receipts, `path:line`, counts from commands, for the auditor)
- `plans/2026-08-27-leaky-types-review.PLAN.visual.human.unga.md` (plain words, mermaid, zero citations, for Chris; a plan without it is undelivered)
FORBIDDEN: any edit under `src/`, `tests/`, `Cargo.*`. No refactor is applied.

## What "leaky" means here, each with the command that finds candidates
1. Enum matched outside its defining module (a switch on a closed set that a trait impl should own). `git grep -n 'match .*::' -- 'v6/**/*.rs'` then for each enum: `git grep -c '<Enum>::' -- 'v6/**/*.rs'` grouped by file; an enum with match sites in 3+ files is a candidate. Name the exact enum, every match site, and the trait or impl-per-variant it should become.
2. Trait with methods only one impl uses, or default methods every impl overrides identically: `git grep -n '^pub trait\|^trait' -- 'v6/**/*.rs'` then per trait list impls (`git grep -n 'impl .*<Trait> for'`) and per method who overrides. Candidates: a method used by one impl, or a trait with one impl.
3. Struct with `pub` fields read or written across a crate or module boundary (the field is the API): `git grep -n 'pub [a-z_]*:' -- 'v6/**/src/**/*.rs'`, then for the top 20 structs by pub-field count, `git grep -c '\.<field>' ` outside the defining file. Candidates: a field touched from 3+ foreign files, or a field that is set then read only inside one module (should be private).
4. Stringly-typed kinds: `&'static str` or `String` fields named `kind`, `role`, `family`, `tag` that are compared to literals at call sites (`== "…"`). `git grep -n 'kind: &.static str\|kind: String\|== "' -- 'v6/**/*.rs'`. Candidate when the same literal appears in 2+ files.
5. `pub` that is only used inside its crate: sample with `cargo +nightly rustc -- -W unreachable_pub` if available, else `git grep` per item; report the count and the top 15 by fan-in.
6. Boolean and Option flag threading: functions taking 2+ `bool` parameters, or a struct with 3+ `bool` fields that select behaviour (`git grep -n 'bool,' -- 'v6/**/src/**/*.rs'`). Candidate for an enum or a per-mode impl.

## Output shape
One ranked table, most leaky first: `# | kind (1-6) | item | defined at | leak sites (count, files) | proposed shape (one line) | blast radius (files touched by the fix) | risk (L/M/H)`. Minimum 15 rows, maximum 40. Every row cites `path:line`. Counts come from commands you ran; list every command verbatim in an appendix. Then a 5-row "do first" list where blast radius is small and the leak is in the move/Rehome path or the engine hosts. No prose paragraphs; tables and lists only. Banned words: provenance, substrate, load-bearing, regime, refusal, ground truth. Do NOT propose "write our own" anything; if a fix is a library, name it.

## Delivery
One PR against `origin/main` with the two docs, title `plan: leaky enums, traits, structs review (v6 Rust)`. Do not merge. Hail on post:
`boop beep --no-wait --as <your-lane-name> sprefa-coordinator "<PR#, row count, top 3 items>"`.
