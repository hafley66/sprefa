# BRIEF: connect prolog to the rust engine. One .dl6 in, rust out, sqlite, graded.

## Base
Confirm the base with `git log --oneline -1` before your first commit. The
spawn printed the sha; that is your base. The ordering is not a gate. If a
procedural line in this brief seems to forbid otherwise-correct work, the work
wins: note the conflict in your report and keep going.

## The one sentence
`emit_ts.pl` turns a `.dl6` into TypeScript that runs on SQLite and is graded
against 392 conformance fixtures; `emit_rust.pl` turns nothing into anything
yet, because it is not wired to any call site. Wire it, then grade it on the
same corpus.

## What is actually true right now, measured

| fact | evidence |
|---|---|
| `emit_rust.pl` exists, 307 lines | `v6/prolog/emit_rust.pl` |
| it is not wired anywhere | grep `emit_rust` over `compile.pl`, `v6/prolog/*.pl`, `v6/justfile` = one hit, a comment at `lower.pl:5325` |
| the rust engine runs | `v6/sprefa-engine-rs/src/*.rs`, 1726 lines |
| its one passing test builds the program BY HAND in rust | `tests/skeleton.rs:16` `fn fixture_program() -> GenProgram`, asserted at `:129` |
| no `.dl6` is compiled in any rust test | same file, no `compile_dl6` anywhere |

So the emitter and the runtime have never met. That meeting is this lane.

## The seam you plug into, and it already exists

`compile.pl` calls the emit phase through a 5-argument seam:

    call(Emitter, Name, Plan, Lowered, BootStatements, Text)

and `compile_dl6/3` takes an `emitter(Module:Pred)` option that defaults to
`emit_ts:emit_program`. Both are ON MAIN as of PR #213: see
`v6/prolog/compile.pl:356` (`emitter_option(Options, Emitter)`) and `:371-372`
(the default). You do NOT need to modify the seam, and you must not. You need
`emit_rust:emit_program/5` to fit it.

A worked call site already exists to copy from: `isolated_compiler_dd` uses the
same seam, and `v6/bench-cli/adapters/dd-diet-rust-sqlite.sh:45` shows the
option being passed from a shell script.

## Deliverables, in order. Commit after each.

1. **`git add v6/prolog/emit_rust.pl`.** It is untracked. Nothing else in this
   brief matters if it is not in the repo.
2. **One `.dl6` compiles to rust through the option.** A test or script that
   runs `compile_dl6('<fixture>.dl6', Out, [emitter(emit_rust:emit_program)])`
   and produces a rust source file. Prove it by compiling that output with
   `cargo build`.
3. **That compiled output runs and matches the oracle.** Same `.dl6`, same
   schedule, oracle tick log vs rust tick log, byte-diffed. Replace the
   hand-built `fixture_program()` in `tests/skeleton.rs` with the compiled one,
   or add a second test that uses it and say why both exist.
4. **A corpus grade, not a single fixture.** A script in the shape of
   `v6/dd-runner/grade.sh`: compile every corpus fixture through
   `emitter(emit_rust:emit_program)`, run each, byte-diff the tick log against
   the oracle's, print one summary line `RUST-GRADE graded=N byte-clean=M`.
   Ratchet it in both directions with a checked-in `graded.tsv` the way
   `dd-grade` does.

## Anti-cheat

| tempting shortcut | why it is a lie |
|---|---|
| keep asserting the hand-built `GenProgram` passes | it proves the runtime, never the emitter |
| grade only the fixtures that pass | the summary line must carry the denominator |
| write the expected tick log by hand | the oracle writes it; a hand-written expectation grades nothing |
| skip `cargo build` on emitted output | rust source that does not compile is not output |
| call a fixture "byte-identical" without diffing bytes | print the diff command in the commit message |

Report `byte-clean=M of N` even when M is small. A real 40/286 is worth more
than a decorated 3/3.

## File ownership. Yours alone:
- `v6/prolog/emit_rust.pl`
- `v6/sprefa-engine-rs/**`
- a new grade script under `v6/sprefa-engine-rs/` or `v6/scripts/`

## Forbidden, owned by other live lanes:
- `v6/prolog/compile.pl` and `v6/prolog/compile/**` (dd branch, merging)
- `v6/prolog/emit_ts.pl` (do not touch the working emitter)
- `v6/prolog/compile/7_emit_ts_types.pl`, `8_emit_rust_types.pl` (type-ir lane)
- `v6/tsv2/scripts/**` (green-cleanup lane)
- anything under `v6/dd-runner/` (read `grade.sh` for its shape, change nothing)

If you need a change inside a forbidden file, STOP and report the exact line
and the reason. Do not work around it.

## KNOWN-RED, do not chase
`just green-all` is red and has been for days. `.github/CI-KNOWN-RED.md`
allowlists the failing legs. Read it before reporting any leg as broken. A leg
that fails and is NOT allowlisted is the real signal.

## Style laws, inline
- No em dashes. No `provenance`, `substrate`, `load-bearing`, `regime` in prose
  or identifiers.
- Comments state only constraints the code cannot show. No change-log narrative,
  no dates, no arc references in source.
- Every new rust type says what it is on first reading.
- dl variable names are descriptive, never single-letter.
- Surrogate keys: stored rels key on INTEGER ids. A composite TEXT PRIMARY KEY
  in emitted DDL is a defect. Read `.claude/skills/sql-relational-design` and
  `.claude/skills/sqlite-costs` before any schema decision.
- The 10-second law: any operation over 10s is a defect to investigate, not a
  budget to accept.

## Worktree setup, before your first commit
The extractor binary and two pnpm installs are absent in a fresh worktree. Run
the repo's prescribed setup before committing; the pre-commit hook needs them.
