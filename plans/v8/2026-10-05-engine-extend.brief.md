# Brief: page 4, extend the installed engine (P4a) — design first

User (2026-10-05): P4a. Reason: HMR of import graphs. A module is its own program, and the
module system lives in userland, so adding, editing and removing a module must change the
installed engine without reinstalling it. Comptime generated rules use the same path.

## Today

- `_4_comptime/_2_rounds.rs:282-366`: a round that assembles new rules (`assemble_generated_program`,
  `call_rules`) changes the program; `IrSession::evaluate` (`_9b_ir_eval.rs:436-442`) sees a new
  structural fingerprint and installs a fresh engine (every node rebuilt, cold settle).
- `Engine` trait: `ivm_engine` (`sqlite_ivm/crates/ivm-engine`); impls `ivm-dd` (`2_dd.rs`),
  `ivm-sqlite` (`4_engine.rs`, `3_nodes.rs`). Install = whole `ivm_ir` program.
- Page 2 added `mark/rewind` (sqlite savepoint) — related lifecycle primitive.

## Do — stage 1, design only (stop for user review)

Write `plans/v8/2026-10-05-engine-extend.design.md`, in this order (user planning rule):

1. Type signatures. At least: `Engine::extend(&mut self, fragment: IrFragment) -> FragmentId`,
   `Engine::retire(&mut self, FragmentId)`, what an `IrFragment` holds (its relations, strata,
   which existing relations it reads, which it exports), how a fragment names upstream
   relations. Pseudocode comments for each body, both engines.
2. Instance timelines: engine, fragment, relation, arrangement/table. Who creates, who reads,
   who drops; what happens to downstream fragments when an upstream fragment retires or is
   replaced (HMR edit = retire + extend, or replace in place?). Cross-fragment recursion: a
   module import cycle must become one fragment (SCC of the import graph) — say how that is
   detected and what it costs.
3. Storage: per engine, where a fragment's state lives, the sequence of reads and writes during
   extend / settle / retire, and uniqueness conditions (relation ids, term ids, constructor
   relations shared across fragments).
4. Mapping: dl7 module -> fragment; comptime generated rule -> fragment; macrotime library ->
   base fragment. Which code in `_9b_ir_eval.rs` / `_9a_ir_lower.rs` changes (lowering must
   produce fragments against an existing catalog, not whole programs).
5. Prior art with links/paths: DD dataflows importing arrangements across dataflows (trace
   handles), Materialize dataflow install/drop, DBSP, salsa. Library candidates before any
   bespoke piece.
6. Three dl7 examples: add a module, edit a module (HMR), remove a module; expected rows before
   and after each, both engines.
7. A test plan: incremental (extend/retire sequence) == fresh install of the final program, per
   engine and dd == sqlite.

## Laws

- Stage 1 writes the design doc only; no code. Stop and report.
- Read-only on sqlite_ivm and sprefa source. Paths and line numbers for every claim.
- Commit the design doc on branch `design/engine-extend` in an own worktree from sprefa main.
- Commits end with
  `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>` and
  `Claude-Session: https://claude.ai/code/session_019jL2FmxduyQQ9kpmQYBKj7`.

## Receipt

```
status: done | blocked
branch + sha:
design doc path:
open decisions for the user: <list>
next: <one action>
```
