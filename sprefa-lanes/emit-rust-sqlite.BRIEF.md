# BRIEF: emit_rust.pl -- Rust + SQLite, the second language emitter

## Base
- Worktree of `/Users/chrishafley/projects/sprefa`. Base sha `154ae23c`.
- FIRST action: `git log --oneline -1`. Any other base = STOP AND REPORT.

## USER DECISION, 2026-08-12, verbatim
"i only want emitters man no more dd-rust-dd on its own that is retarded, we
have that baseline. i want rust + sqlite emitted now, we can achieve dd
afterwards i want to use this damn thing"

and on shape:

"literally copy the ts into rs with idiomatic tokio and channeling/rx semantics
in rust form with streamext and signals and spawns"

`dd-rust-dd` as a standalone hand-written kernel is CANCELLED. Do not build it,
do not price it, do not mention it as future work.

## One sentence
`emit_ts.pl` is the only language emitter in the tree; write its Rust sibling,
plus the Rust runtime crate it emits against, and grade it by tick-log
byte-diff against the same corpus.

## What you are actually building, in two halves
The emitted TypeScript is mostly DATA. The work is in the runtime.

| half | today | you build |
|---|---|---|
| the emitter | `v6/prolog/emit_ts.pl`, 2814 lines, `emit_program/5` at :2687 | `v6/prolog/emit_rust.pl`, same `emit_program/5` signature |
| the runtime it emits against | `v6/tsv2/runtime/`, 3568 lines TS | `v6/sprefa-engine-rs/`, a new crate |

`v6/tsv2/runtime/` by file, so you can see where the weight is:

| file | lines | what |
|---|---|---|
| `1_incremental.ts` | 1439 | the tick engine. THE port. |
| `types.ts` | 1069 | interfaces; most become Rust structs and traits, cheap |
| `structPlane.ts` | 302 | |
| `ticklog.ts` | 105 | the byte-diff artifact; encoding must match EXACTLY |
| `serveStats.ts`, `rows.ts`, `3_subscribe.ts`, `textPlane.ts`, `tickLoop.ts`, `tickStatements.ts`, `diff.ts`, `2_boot.ts`, `trace.ts`, `scratchStore.ts`, `0_traceSchema.ts` | 23-108 each | |

## The contract you must hit
`IGenProgram`, `v6/tsv2/runtime/types.ts:471-481`:
```ts
readonly name: string;
readonly internMode: IInternMode;
readonly ddl: readonly string[];
readonly rel_columns: Readonly<Record<string, readonly string[]>>;
readonly rel_column_types?: Readonly<Record<string, readonly IRowColumnType[]>>;
readonly arrival_targets: readonly string[];
tick(seam: ISqlSeam, arrivals: IArrivalBatch): Observable<ITickDeltas>;
```
Plus `boot`, an extra field beyond the five pinned names, and `final_select`.
The Rust shape is the same contract with `Observable<ITickDeltas>` becoming a
`Stream<Item = TickDeltas>`. **"extend by adding fields, never renaming"**
applies to your Rust struct exactly as it does to the TS interface.

## BUILD-VS-BUY, MANDATORY, BEFORE ANY RUNTIME CODE
CLAUDE.md, non-negotiable at every agent level:

> never assert "write our own" for a common-shaped problem without library
> research + written candidate analysis first. No one-line dismissals.

The user named tokio, StreamExt, signals, spawns. That is a steer, not a
research substitute. Write a candidate table covering at minimum: `tokio`,
`tokio-stream`, `futures` `StreamExt`, `async-stream`, `tokio::sync` channels
(mpsc/broadcast/watch), and whatever your research turns up for reactive
streams in Rust. For each: what it gives you, what it costs, what it forces on
the architecture, and how it maps to the rxjs operators the emitted TS actually
uses (`concatMap`, `forkJoin`, `map`, `of`, `toArray`).

Same for SQLite: `rusqlite` is already used by `v6/dd-runner` and
`v6/sprefa-store`. Say whether you match them or diverge, and why.

A one-line dismissal of any candidate voids the deliverable. The user's steer
becomes the decision AFTER the table exists, not instead of it.

## The v6 laws that have Rust analogues. State each mapping in the plan doc.
| TS law | your Rust reading |
|---|---|
| exactly ONE manual `.subscribe()` per app | one driver at the root; no scattered `block_on` |
| Promise/async banned above the SqlRunner seam; sync stays sync | async at the SQL seam only; in-memory row work is plain sync `Vec` code |
| SQL building sync, running Observable | SQL strings built sync, executed through the async seam |
| every new class declares its interface in the package's header `types.ts` | every new type declared in the crate's header module; traits, not bare free functions |
| `await someObservable` silently never subscribes | the Rust trap is a `Stream` that is built and never polled. Name it |

## Files you own
| path | permission |
|---|---|
| `v6/prolog/emit_rust.pl` | create |
| `v6/sprefa-engine-rs/**` | create, the whole crate |
| `v6/prolog/compile/test/emit_rust.test.pl` | create |
| `plans/2026-08-12-emit-rust-sqlite.md` | create |
| `plans/2026-08-12-emit-rust-sqlite.visual.human.unga.md` | create |

**Touch nothing else.** Three other lanes are live and these are theirs:
`v6/prolog/compile.pl` and `v6/prolog/compile/6_emit_dd_plan.pl`
(dd-plan-emitter-seam), `v6/prolog/lower.pl` and `v6/prolog/analyze.pl`
(uniform-surrogate-id), `.github/**` (green-all-triage). Also forbidden:
`v6/prolog/emit_ts.pl`, `v6/tsv2/**`, `v6/dd-runner/**`, `chat_log/**`.

READ `emit_ts.pl` and `v6/tsv2/runtime/**` as much as you like. That is the
spec. You may not edit them.

## Wiring, and why you cannot do it yet
`compile_program_phases` already takes the emitter as an argument and calls
`call(Emitter, Name, Plan, Lowered, BootStatements, Text)` at `compile.pl:438`.
A concurrent lane is adding an `emitter(Module:Pred)` option to `compile_dl6/3`
so the text door can select one. **Do not add that option yourself; it is that
lane's file.** Until it lands, drive your emitter directly from your own test
harness:
```prolog
compile:program_plan(Term-Bindings, Options, Plan),
lower:lower_program(Plan, Lowered),
emit_rust:emit_program(Name, Plan, Lowered, BootStatements, Text)
```
Your `emit_program/5` must be substitutable for `emit_ts:emit_program` with no
call-site special case, so the wiring is one argument the day that lane merges.

## Scope. Read this twice.
Do NOT try to cover all 286 fixtures. Land a spine that runs end to end, then
widen. Order:

1. **Candidate table + the plan doc skeleton.** No code.
2. **The crate skeleton**: `IGenProgram`'s Rust equivalent, the SQL seam over
   `rusqlite`, DDL execution, boot statements, and a tick loop that does
   nothing but drain arrivals. Prove it compiles and runs with `cargo test`.
3. **ONE fixture end to end, byte-identical tick log.** Pick the smallest
   program with a level rule. `v6/prolog/compile/out/<name>.oracle.jsonl` is
   the oracle tick log; your Rust arm's stdout must byte-match it. Report the
   fixture name and paste the diff being empty.
4. **Widen by construct class**, one commit per class, with a count each time:
   level rules, then edge rules, then aggregates, then retraction. Report
   `N/286` after each. Stop where you stop and say where.

A spine that byte-matches on 5 fixtures beats a sprawl that matches on none.

## Gates
```bash
cargo test --manifest-path v6/sprefa-engine-rs/Cargo.toml
cargo clippy --manifest-path v6/sprefa-engine-rs/Cargo.toml -- -D warnings
cd v6/prolog && swipl -g go -t halt ARCH.pl
```
And, because you must not regress the tree you are reading:
```bash
cd v6/tsv2 && bash scripts/sweep.sh    # RUN total=286 identical=283 wrong=0, unchanged
cd v6 && just text-door                # unchanged
```
Those two must be IDENTICAL to base. You are adding a new emitter, not
touching the existing one. If either moves, you touched a file you do not own.

## KNOWN RED on main, not yours, do not chase
- `just green-all` is red; `.github/CI-KNOWN-RED.md` allowlists 11 legs and a
  triage lane is rewriting it. Do not read it as truth.
- `roundtrip` fails on `mutual_recursion_matches_oracle`, `fail(not_variant)`.
- 3 bench-cli cells fail because `cases.json` points at the lossy `dl_view/`
  render.
- A concurrent lane is changing the emitted DDL shape to one uniform
  `("__id" INTEGER PRIMARY KEY, <cols>, UNIQUE (<cols>))`. Your emitter
  consumes DDL from `lowered/8`, so you inherit whatever lands. Do NOT hardcode
  today's DDL text anywhere.

## Anti-cheat
| rule | why |
|---|---|
| the tick log is byte-diffed against the oracle jsonl | "looks right" is not a receipt |
| stdout carries the tick log and NOTHING else | that is what gets diffed |
| `sweep` and `text-door` are byte-identical to base | you are adding, not editing |
| no candidate dismissed in one line | the build-vs-buy law |
| every `N/286` comes from a command you ran | no estimates |
| no fixture is widened or special-cased by name | |
| you do not touch `compile.pl`, `lower.pl`, `analyze.pl`, `emit_ts.pl`, `v6/tsv2/**` | four lanes are live |

## Worktree setup, before your first commit
```bash
mkdir -p v6/sprefa-extract/target/release
cp /Users/chrishafley/projects/sprefa/v6/sprefa-extract/target/release/extract \
   v6/sprefa-extract/target/release/extract
(cd v6/tsv2 && pnpm install)
(cd v6/sprefa-store/js && pnpm install)
```
`git commit -n` and `--no-verify` are FORBIDDEN.

## Rails
- Commit after each numbered scope step, with that step's gate output.
- Never spawn a subagent.
- The 10-second law applies to every command you run. `cargo build` on a cold
  crate is the one place you may exceed it; say so with the wall time.

## Style laws, inline
- No em dashes. No `provenance`, `substrate`, `load-bearing`, `regime`.
- "refusal" banned in prose; unbuilt is "TODO" or "not built yet".
- No `here is`, `here's`, `below is`, `the following`, `clearly`, `obviously`.
- Comments state only constraints the code cannot show. No change-log
  narrative, no dates, no restating the next line.
- Variable names descriptive, never single-letter, in Prolog and Rust alike.
- Both plan docs required; the `.visual.human.unga.md` one is plain words,
  ascii or mermaid, ZERO citations. Docs open with a TOC.
