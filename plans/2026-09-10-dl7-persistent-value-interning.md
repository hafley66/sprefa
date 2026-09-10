# DL7 layout to persistent value interning

Historical checkpoint, superseded by `2026-09-10-dl6-as-dl7-userland.md`.
The Rust-plan adapter below was proposed and withdrawn. No implementation was
performed under this brief. Backend structures do not define DL7 semantics.

## Context

Chris requested implementation of persistent value interning from the existing
DL7 userland layout. Base is main 8aba008df. The existing layout emitter is
v7/emitters/2_interned_storage.dl7, exercised by
v7/test/15_interned_storage.test.pl. It derives selected type, field, dictionary,
dependency, reverse-projection and identity rows. The documented tested artifact
route uses compiler-closed rows; the unsliced second-evaluation route is incomplete.

The Rust engine already has text_plane.rs, struct_plane.rs, enum_plane.rs,
generated program plans and SQLite persistence. Determine the missing adapter
before designing any new interner. Extract and TypeSpec storage remain unchanged.
Primary checkout has user edits in v6/sprefa-extract/src/types.rs; preserve them.

## Decisions

- User clarification: DL6 is implemented in DL7 USERLAND. If the task needs a
  new Prolog-hosted facility, predicate or binding exposed to DL7, or any DL7
  kernel change, stop immediately and yield to Chris with a concrete contract
  explanation. Existing host facilities may be reused. Moving userland policy
  or semantics into bespoke Prolog/Rust is not an authorized workaround.
- Reuse the existing host runtime and libraries where their contracts fit.
  No second interning implementation without a reviewed evidence-based gap.
- Runtime identity/storage only. No changes to kernel evaluation, product apply,
  graph/type semantics, functional keys or phase boundaries. Any required change
  there stops for explicit user explanation and approval.
- Consume actual generated layout rows. No hand-maintained parallel schema or
  fixture-specific mapping. Preserve ordered fields, constructor identity,
  dictionary domain, sum discriminants, and policy scope.
- Caller owns the transaction. Values intern child-first and return persistent
  integer references. Repeated equal values reuse IDs across reopen. Distinct
  constructors/domains/ordered values must remain distinguishable.
- No per-field SQL calls or insert-then-select loops where batch plans already
  exist. Use runtime-bounded batching and cached statements. Report existing
  string/composite-key constraints before proposing new indexes. Hash equality
  alone must never establish value equality.
- No watchers, effects, deletion/GC policy, automatic migrations, extractor
  output changes, TypeSpec replacement, library upgrades or new dependencies.
- New files follow numbered dependency/reading order. No file moves needed.

## Staffing and first checkpoint

One Claude Opus 5 medium lane through Boop's Claude harness. No OpenCode, Codex
or nested agents. Work only in the lane worktree. No commits, pushes, cleanup,
package installs or broad formatting. Coordinator owns review and integration.
Use a lane-specific CARGO_TARGET_DIR supplied in the environment.

FIRST TURN: bounded read-only adapter design. Read applicable AGENTS.md and
relevant existing implementation. Write only
plans/2026-09-10-dl7-persistent-value-interning.report.md (at most 100 lines).
Do not implement until the coordinator sends the next assignment.

Read these boundaries by symbol, not enormous whole-file dumps:
- v7/emitters/2_interned_storage.dl7 and its test/fixture;
- v6/sprefa-engine-rs/src/{types,program,text_plane,struct_plane,enum_plane,sql}.rs;
- existing DL6 emitters producing the consumed interning plans, found by symbol;
- current v7 artifact-emitter/public CLI JSON serialization contracts.

Report exact reusable signatures and a minimal bridging signature, state/table
ownership, transaction lifetime, input/output example, required field encodings,
sum support, layout compatibility checks and known unsupported shapes. Name the
precise files to change and the smallest executable vertical slice. Identify any
caller semantics the adapter cannot preserve. Send initial hail, inventory hail,
then final checkpoint via `boop beep parent ... --no-wait`; stop after report.

## Intended implementation stages after coordinator review

1. Adapt compiler-derived layout to the existing runtime interning contract,
   with validation before writes and an explicit supported-shape boundary.
2. Execute batched persistent interning plus reverse projection through a
   callable Rust API, exercising a file-backed SQLite database.
3. Add the actual DL7-layout-to-runtime integration test and a reproducible
   documented invocation. Preserve existing callers and generated artifacts.

## Verification and final end state

Exact tests must cover: equal values reuse IDs within a batch and after reopen;
shared child values deduplicate; ordered fields/constructors/dictionaries and sum
variants stay distinct; decode restores the logical values; caller rollback
removes new persistent rows and remains usable; incompatible layout and malformed
values fail before persistent changes; failed batched work cannot publish partial
results. A multi-batch case checks statement counts or equivalent batch evidence.
Use existing dependency versions and targeted tests, one correction pass per
failure before reporting an expanded blocker. No repeated full-suite loops.

End state: a real existing DL7 fixture produces layout data consumed by the Rust
runtime, values intern durably, reopen reuses IDs, rollback and reverse projection
pass. Report exact commands/results, new coverage, limitations and files. A report
alone does not satisfy implementation completion. Stop for the user checkpoint.
