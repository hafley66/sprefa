# BRIEF: recover the v3/v4/v5 write-effect system, then price it as a library

## Base
Confirm the base with `git log --oneline -1` before your first commit. The spawn
printed the sha; that is your base. The ordering is not a gate. If a procedural
line in this brief seems to forbid otherwise-correct work, the work wins: note
the conflict in your report and keep going.

**Docs only. Write ZERO implementation code.** Two plan docs are the whole
deliverable.

## The user's words, verbatim, twice. Read them before anything else.

> "we need fs read/dryable-writes/watch to be standard form across the board.
> yes to batching, its purpose of collect from dl5"

> "go read v3/v4 old code back in time in git, it has a system for this that was
> the mentallity. also its refactoring/codemod system was based in this concept
> from the get i thought. v5 might have it too. i tried to carry it around but
> would rather this kind of logic also be a library from now on im tired of
> optimally welding the chassis to ram everytime"

> "lets consider a cli/lib of rust like sprefa-extract for codemodding/generic
> write batching/system. in v3 i was obsessed that it is just an effect kind / a
> literally yield in language terms of coroutines in sagas/coroutine styles of js"

Three things are being asked for and you must keep them distinct:
1. **Recover** what v3, v4 and v5 actually built, with citations.
2. **Judge** whether it should become a standalone Rust library plus CLI, shaped
   like the existing `sprefa-extract`.
3. **Connect** it to the effect-as-yield framing: a write is an effect the
   program YIELDS, and an interpreter decides to perform, skip, or report it.
   This is the redux-saga / coroutine shape, and the user says it was the
   original intent.

## Where the code is. These paths are measured, they exist.

| tree | path |
|---|---|
| v3 and v4 archive | `~/projects/sprefa-archive-20260701` (contains `v3/`, `v4/`, `v5cozokuzu/`) |
| the original archive | `~/projects/sprefa-archive-20260428` |
| v5, current | `/Users/chrishafley/projects/sprefa/src/` |
| v6 | `/Users/chrishafley/projects/sprefa/v6/` |

Starting points already found in v3, verify each and follow outward:

| what | file:line |
|---|---|
| `WriteApproval` | `v3/crates/pipeline/src/effects.rs:42` |
| `WritePolicy` enum | `v3/crates/pipeline/src/effects.rs:52` |
| `WritePolicy::decide` | `v3/crates/pipeline/src/effects.rs:63` |
| `WriteDecision` enum, with `DryRun` | `v3/crates/pipeline/src/effects.rs:76`, `:86` |
| `FsListFilesEffect` / `FsListFilesBatcher` | `effects.rs:116`, `:152` |
| `ReadBytesEffect` / `ReadBytesBatcher` | `effects.rs:189`, `:263` |
| `ReadBytesBatchEffect` / `read_bytes_batch` | `effects.rs:302`, `:370`, `:381` |
| `PrintEffect` / `PrintSink` | `effects.rs:413`, `:432` |
| `WriteCursorOp` | `v3/crates/pipeline/src/ops/write_cursor.rs:40` |
| the dry-run behaviour, stated | `write_cursor.rs:328` ("dry-run buffers the row with `decision = DryRun` + `Err(\"dry-run\")`") |
| its test | `write_cursor.rs:362-367` |
| the HTTP surface that chose the policy | `v3/crates/server/src/transport_http.rs` |

Note what `write_cursor.rs:328` already shows: the batcher already existed, the
approval policy already existed, and dry-run was a DECISION recorded per row
rather than a branch that skipped the code path. That distinction matters and
should survive into any recommendation.

## Deliverable 1: the archaeology

For EACH of v3, v4, v5, answer with citations:

| question |
|---|
| what effect kinds existed (read, list, write, print, shell, other) |
| how a write was expressed: an op, an effect value, a yielded request? |
| who decided whether it actually happened, and where that decision was recorded |
| what dry-run meant exactly: skipped, buffered, reported, all three? |
| what batching existed and what it batched over |
| what the codemod / refactoring system was, and how it used the above |
| what watch existed: file, folder, glob, and what it emitted |
| why it did not survive into the next version, if you can tell |

A comparison table across the three versions is the centrepiece. Where a
version lacks a row, say "absent" rather than leaving a blank.

Then the same for what v6 has TODAY, which is measured as:

| v6 capability | status | evidence |
|---|---|---|
| `watch` as a declared bind | EXISTS | `v6/prolog/compile/registry.pl:295`, executor at `:298`, `IWatchBindRunner` at `v6/tsv2/runtime/types.ts:817` |
| host effects with a witness cache | EXISTS | `IWitnessCache` and `IHostRunner` in `types.ts:747-777` |
| `sh` shell host decl | EXISTS | grammar `v6/dl/grammar/dl.langium:52-58`, example `dl_view/duplicate_host_name_is_refused.dl6:1` |
| file read from `.dl6` | ABSENT (only the runtime's own digest read at `v6/tsv2/serve/2_binds.ts:232`) | |
| file write from `.dl6` | ABSENT as a builtin; `sh` can shell out, one fork per call | |
| dry-run | ABSENT everywhere | |
| `collect` batching | ABSENT in v6; v5-only, see `examples/gh-cache-batch.dl:8` | |

Verify every one of those rows. If a row is wrong, that is a finding and it goes
at the top of your report.

## Deliverable 2: the effect-as-yield framing, made precise

The user's claim is that a write is "an effect kind, a literal yield in language
terms of coroutines in sagas". Establish whether v3 actually implemented that or
merely resembled it:

- in redux-saga, an effect is an inert DESCRIPTION the generator yields, and a
  separate interpreter performs it. The generator is pure and testable because
  it only ever produces descriptions.
- does `v3`'s `WriteCursorOp` yield an inert description, or does it perform IO
  and record a decision afterward? `write_cursor.rs:328` hints at the second.
  Read enough to say which, with the line.
- what does that difference buy or cost for dry-run, for batching, and for
  testing?

Two repo skills cover this ground and you should read them rather than
re-deriving: `sagas-redux-saga-essence` and `sagas-sprf-effect-runtime` in the
claude-research toolkit. Find them under `~/projects/claude-research/`. Cite
what you take from them.

## Deliverable 3: build-vs-buy. This is a STANDING LAW, not a suggestion.

The user is explicit: "would rather this kind of logic also be a library from
now on, im tired of optimally welding the chassis to ram every time".

Never assert "write our own" for a common-shaped problem without library
research and a written candidate-by-candidate analysis first. No one-line
dismissals of any library. Infra is bought, never built.

Research and price, each with maintenance status, API shape, and what it does
NOT cover:

| problem | candidates to research (there are others, find them) |
|---|---|
| applying a set of file edits atomically with a dry-run mode | `codemod` crates, `rust-analyzer`'s `TextEdit` / `SourceChange` model, `ropey`, `similar` for diffs, `diffy`, `imara-diff` |
| a codemod / refactor driver | `ast-grep` (already used in this repo), `comby`, `jscodeshift`'s model, `rust-analyzer`'s assist framework, `syn` + `prettyplease` |
| batched filesystem reads and directory walking | `ignore` (`WalkParallel`), `walkdir`, `jwalk`, `globset` |
| file and folder watching | `notify`, `notify-debouncer-full`, `watchexec` (which is both a lib and a CLI, and is the closest existing thing to what is being described) |
| effect description + interpreter in Rust | `futures` streams, `genawaiter` / `async-stream` for coroutine shapes, and whether an enum + match beats any of them here |
| atomic file replace | `tempfile` + persist, `atomicwrites`, `cap-std` |

For each: does it fit, what would we still write ourselves, and what is the
dependency weight. A recommendation per problem, with the reasoning visible.

`watchexec` deserves particular attention because it is a maintained library
AND a CLI that already does watch plus debounce plus run, which is a large part
of the ask. Say plainly whether adopting it beats extending the existing v6
`watch` bind, and why.

## Deliverable 4: the shape recommendation

`v6/sprefa-extract` is the model the user named: a Rust crate that is both a
library and a CLI, called by the runtime over a defined wire. Read it, state its
actual shape (crate layout, CLI surface, wire format, who invokes it), and then
say whether a `sprefa-write` or `sprefa-codemod` sibling should exist on the
same pattern, or whether this belongs inside the existing host runner.

Answer these concretely:
- what is the wire between the runtime and this thing, and why that one
- how a `.dl6` program names a write, given `sh` already exists and forks a
  shell per call
- how batching arrives: ported `collect`, or a batch-shaped host decl, or the
  library batching internally
- where the dry-run decision is recorded so it can be reported to a user
- how watch-file and watch-folder differ in the surface, since the user asked
  for both explicitly

Present these as FORKS with prices, not as a decision. The user rules. Your job
is to make the choice cheap for them by having measured it.

## Anti-cheat

| tempting shortcut | why it is a lie |
|---|---|
| summarising v3 from its comments | comments are not the code; cite line numbers you read |
| "v4 was similar to v3" | measure it or say you did not |
| recommending a bespoke build with no candidate table | violates a standing law at every agent level |
| one-line dismissal of a library | banned explicitly |
| picking the fork yourself | the user rules; you price |
| skipping the unga doc | a plan without it is undelivered |

## Deliverables, exactly two files

1. `plans/2026-08-12-fs-effects-recon.RESEARCH.md` — opens with a table of
   contents. Every claim carries a `file:line` or a command and its output.
2. `plans/2026-08-12-fs-effects-recon.RESEARCH.visual.human.unga.md` — the same
   content in plain words for a reader with zero context. Diagrams, no
   citations, no undefined jargon. REQUIRED.

Output form for both: tables, lists, and mermaid diagrams. Prose is a one-line
caption under a diagram, never the medium. Use a mermaid sequenceDiagram for the
yield-then-interpret flow, and a comparison table for v3 against v4 against v5
against v6.

## File ownership
YOURS: the two plan docs only. Everything else in the repo is READ ONLY for this
lane, including the archives.

## Style laws, inline
- No em dashes. Banned in prose AND identifiers: `provenance`, `substrate`,
  `load-bearing`, `regime`. Use source/origin, base layer, critical, mode.
- The word "refusal" is banned in prose; an error for an unbuilt construct is
  "TODO" or "not built yet".
- No sycophancy, no negative parallelism ("not X, Y" / "this isn't X. it's Y").
- Construct names use ONLY rxjs, prolog, or SQL words.
- The 10-second law: any operation over 10s is a defect, not a budget.
- Docs open with a table of contents.
