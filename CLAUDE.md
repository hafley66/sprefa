# sprefa

dl8 is the engine and lives at the repo root: `Cargo.toml` (package `dl8`),
`src/`, `tests/`, `book/`, `oracle/` (frozen goldens), `fixtures/`, `prelude/`,
`std/`, `macrotime/`, `crates/tree-sitter-dl7`, `plans/v8/`. `sqlite_ivm` and
`hafley-rs` are gitignored symlinks to `~/projects/sqlite_ivm` and
`~/projects/hafley-rs` (`sprefa-extract` lives there). `v5/`, `v6/`, `v7/` are
frozen earlier engines and take no new work. Everything v6-era that used to
live in this file is archived verbatim at
`chat_log/20260918.4.claude-md-archive.md`.

Lane doctrine, kernel-change rule, docs human-notes, language decisions table:
`AGENTS.md`. This file does not repeat them.

## THE RULE FOR THIS FILE

No number lives in this file. Every claim is a user decision, a `path:line`, or
a command that prints the current truth.

## Where truth lives

| question | answer |
|---|---|
| does the battery pass | `cargo test` at the root; per-file `cargo test --test _NN_name` |
| which legs are allowed red | `.github/CI-KNOWN-RED.md`, "dl8 cargo battery" section. A failing leg NOT in it is the signal. Read it, then re-measure, before reporting anything broken or green. |
| what the language decided | `AGENTS.md` language-decisions table; Chris edits it, agents do not |
| design in flight | `plans/v8/*.design.md` and addenda; briefs `plans/v8/*.brief.md`; probes `plans/v8/probes/` |
| session history | `chat_log/LATEST.md` names the newest log |
| failures that bit | `docs/failure-modes.md` |

Measure a leg three times, never once, and never from the whole gate under
lane load.

## Standing laws (user-set, every agent at every level)

- Doubt yourself before asserting. Verify against the code; if you lack the
  info, go read it.
- A compiler error for an unbuilt construct is "not built yet" with the throw
  site cited. Never report it as a language limit. "refusal" is banned in prose.
- Comments are not the language. Check the code, the fixture, the oracle.
- Build-vs-buy: no bespoke queue/server/scheduler/parser/telemetry/retry/cache
  without a written candidate-by-candidate library analysis first. The datalog
  engine core is the one legitimately bespoke layer.
- The 10-second law: any single operation over 10s is a defect to investigate
  now (named exception: SCIP indexing). No agent foreground-waits more than 10s;
  batteries run in background with a per-test cap.
- Nothing seizes the machine. A change that can beachball it is a blocking defect.
- Failure ledger: every incident that bites gets a `docs/failure-modes.md` row
  (incident, RCA, fail-pre-fix test, rail).
- `eprintln!` never in `src/**`; `tracing` only. Rare CLI-UX lines carry
  `@eprintln-ok`.
- Lang design happens with Chris in the room. Findings come back as cited forks
  with throw sites; implementation of a decided design is dispatchable.
- Kernel changes need explicit user approval with concrete dl7 examples first
  (`AGENTS.md` "Kernel changes require user participation").

## Dispatch

- Every lane spawn goes through `boop beep lane create --branch <kind>/<name>
  --brief <ABS path> --goal "..." --preset <preset> --base-sha <origin/main sha>`.
  `--dry-run` first for an unfamiliar shape. Never a bare tmux spawn.
- Lane model (user 2026-09-18): sonnet-level tasks run `glm53f-omp` (omp
  harness, `zai-plan/glm-5.3-flash`, z.ai plan); `glm53f-omp-max` for the
  heaviest; opus when trade-offs must be weighed. Never spawn unasked; present
  lane name, preset, base sha, brief path and wait for go.
- Briefs carry every file, receipt, command and style law inline. Concurrent
  lanes get disjoint file ownership with forbidden paths named.
- Every worktree branches from `origin/main`. Before a PR: `git diff --stat
  origin/main...HEAD` lists only owned files.
- A lane's `rc=0` means nothing. Grade its tree yourself: `git log`, `git
  status`, forbidden-path diff, targeted tests, battery, the slow leg alone three
  times.
- Lanes never spawn subagents. PR-per-arc. "always merge fixes": a fix PR graded
  green merges without a per-PR word.
- Reach the coordinator: `boop beep --no-wait --as <lane> sprefa-coordinator
  "<one line>"`. `bash scripts/fleet-doctor.sh` opens every coordinator session.
- Plan lanes ship `PLAN.md` (receipts) and `PLAN.visual.human.unga.md` (plain
  words, diagrams). Labs die on landing; durable output condenses into
  fixtures, plans, AGENTS rows.

## Docs law (user-set 2026-09-15)

Every page under the `writing-documentation` skill; the human notes in
`AGENTS.md` outrank it. Order is ontological then topological. No stray numbers
in sentences. Every claim carries a `path:line`, a fixture, or a command. Fixture
blocks via `{{#include}}`, command output via `mdbook-cmdrun`; a hand-copied
block is a defect. Textbook register: short sentences, present tense, no "we".

## Style laws

- Comment budget: only constraints the code cannot show. No change-log
  narrative, dates, arc references.
- dl8 vocabulary: products and rows, never "rel" (`rel` is `dl6.rel`). `key` is
  `dl6.key`. Namespaces `gh.` `git.` `http.` `fs.` `oai.` `cli.`. Executor trait
  is Tower-named `IService` with `call` and `poll`. Construct names use rxjs,
  prolog, or SQL words; "support" is banned, use refCount.
- Every `.dl7` snippet shown to Chris: descriptive variable names never single
  letters, parens broken across lines, no `^` unless namespacing, JS/TS twin
  beside it (class = product, instance = row, static = `^` body).
- Chris reads one page at a time; plans conversational in chat, pictures are
  row tables and one-line-per-edge trees, never mermaid for language pictures.
- Surrogate INTEGER keys; natural TEXT keys once in a dictionary table.
  `.claude/skills/sql-relational-design` and `sqlite-costs` before any DDL.
- Formerly-quadratic paths get COUNT or EXPLAIN tests, additive only. Never a
  per-row write.
- Recompute guard on every from-scratch re-derive or `// @recompute unguarded`.
- Banned words, prose and identifiers: provenance, substrate, load-bearing,
  regime, "ground truth" (say oracle). No em dashes.
- Colocated consistency: inside a file, follow that file's style.

## User decisions still in force for dl8

- Comptime outputs things and its sqlite_ivm db; first pass is CODE GENERATION
  only, runtime deferred, `dl8 eval` stays the dev interpreter.
- Effects are a type constructor `(effect <input product> <output sum>)`; served
  from rows; kernel keeps only `effect/2`; Rust `IService` answers.
- Generics are rule-body constructors over `intern_snapshot`; no template
  engine, no generic syntax. `Result` and `Option` are prelude constructors.
- Caret is namespacing only, off the demo path.
- Zero shell in the engine. Infra is bought (HTTP, scheduler, queue, logging).
- No v5 anything runs. `v6/tsv2` takes no new work.
- hafley66 is an org, watched whole. One server, one db.
- `str.cons` is arity 3; variadic and interpolation are macrotime, unbuilt.
- No `--openapi`/`--serve`-style flags; `oai.document` is a seed line. CLI
  target library immaterial (`cmd-ts` default); codegen must be proven.
