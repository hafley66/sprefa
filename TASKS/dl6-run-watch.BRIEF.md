# Brief: `dl6 run` and `dl6 watch`, one binary, zero scripts

Issue: file `issuectl --json new --type feature --title "dl6 run and dl6 watch: a program runs once or
watches, from one command" --epic cheap-fast-analysis --reporter chris --assignee chris` first; use its
slug in `Refs-Issue:`. Base sha: printed by the spawner; FIRST ACTION `git merge --ff-only <sha>`;
failure = stop and report. Never spawn subagents. PR against `main` with the receipts below.

## The user's ask (2026-08-21)
"i want to start making dataflow programs that watch or run 1 time." Today that takes
`dead-module-rail.sh`: swipl compile, cargo build of the harness, `emit_rust_harness <rs> --arrive ...
--live-hosts --final-only --final-tsv`, env vars for adapters and the extract binary. It must be:

    dl6 run   prog.dl6 [--arrive rel=v,v ...] [--final-tsv|--final] [--final-rels a,b] [--db <file>]
    dl6 watch prog.dl6 [same flags] [--socket <path>]

`run`: compile (cached by source digest), load the program in-process (no cargo build per run: the
harness already loads an emitted `.rs` text; `dl6 build` stays for the one-binary case), run the hosts
live, print finals, exit 0; exit code = 1 when `--fail-on <query>` names a query with rows (the runner
flag the user chose over new syntax). `watch`: same, then stay up: `bind watch(glob)` rels get arrivals
from soopy's watcher (`~/projects/hafley-rs/crates/soopy/src/_8_watch.rs`, `_8a_watch_core.rs`), `bind
interval(secs)` from a tokio timer, each batch is one tick, finals re-print as TSV deltas (`+`/`-` prefix
per row) or serve over the UDS ring in `src/serve.rs` when `--socket` is given. RSS flat across ticks.

## Laws in force
tsv2 paused; Rust door only. Zero shell in the engine: `swipl` is invoked directly (`Command::new("swipl")`
as `dl6 build` already does), never through `sh`. Banned words in any form: "ground truth" (say oracle).
Banned in prose and identifiers: provenance, substrate, load-bearing, regime, refusal, honest(ly), ground*
as a verb, support. No em dashes. No new dl6 syntax. `tracing` only. Every command wraps `timeout`;
nothing foreground over 10s; `export CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=4` (main carries it in
`.cargo/config.toml`). Nothing seizes the machine: `apply_daemon_budget` on the watch path. Commit
messages imperative ending `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`, last paragraph
`Refs-Issue: @<slug>`.

## Read first
`CLAUDE.md`; `v6/sprefa-engine-rs/src/bin/dl6.rs` (clap, `Build` verb, `Dl6Compiler::emit` spawning
swipl, the build template), `src/bin/emit_rust_harness.rs` (every flag: `--arrive`, `--live-hosts`,
`--final*`, `--socket`; the load path for an emitted program; reuse its functions by moving them into
the library (`src/run.rs`, new) so both bins call one implementation), `src/driver.rs` (`run_schedule_live`,
:126 the one `HostLiveRunner` construction), `src/serve.rs` (READ ONLY: a peer owns it; the UDS ring and
long poll; request changes as diffs), `src/hosts.rs` (executor table; `bind` today: grep `bind_plans`,
`interval`, `watch` in `types.rs`/`program.rs`/`driver.rs` to see what the runtime does with `bind` plans
now), `src/build_template/main.rs` (the built binary's main: it must gain the same `run`/`watch` flags so
`dl6 build` output behaves like `dl6 run`), `v6/dl/deadcode/dead-module-rail.sh` (the script you make
unnecessary; after your change it is three lines: `dl6 run v6/dl/deadcode/dead-module-rail.dl6 --arrive
want=... --arrive cargo_manifest=. --final-tsv`; rewrite it and show the diff of its output is empty),
`v6/dl/dataflow/report-extract.sh`, `v6/prolog/compile/scripts/dl6c.sh` if a peer lane landed it (the
compiler as a saved state: use it when present, fall back to swipl), `.claude/skills/sqlite-costs/SKILL.md`
(the `--db <file>` path: file-backed pragmas already applied in `sql.rs` when `DL_DB_URL` is set).

## Deliverables
1. `src/run.rs`: `pub fn run_once(program: &GenProgram, seeds: Vec<Arrival>, options: RunOptions) ->
   Result<RunOutcome>` and `pub fn watch(program, seeds, options, stop: watch::Receiver<bool>) ->
   Result<()>` with type signatures first in the PR body (planning protocol: signatures, pseudo-code
   under each, lifetimes of every state holder, storage then reads and writes). The harness bin becomes
   a thin caller of `run_once`.
2. `dl6 run` / `dl6 watch` verbs; compile cache under `${XDG_CACHE_HOME:-~/.cache}/sprefa/dl6/<blake3 of
   source + compiler tree digest>.rs` so a second `run` skips swipl (measure: first run wall, second run
   wall, paste); `--fail-on <query>`; adapters sidecar found beside the source as `dl6 build` does;
   `DL_EXTRACT_BIN` no longer needed (in-process executors; if any path still reads it, name it).
3. `bind watch` and `bind interval` wired to soopy's watcher and a timer in `watch`; each batch one tick;
   a 5-minute watch over `~/projects/hafley-rs` with a touch every 30s: paste the tick walls and an RSS
   series sampled every 10s (flat after tick 1, under 5% growth), load average noted.
4. The built binary (`dl6 build`) accepts the same flags (template main updated); `dl6 build` of the
   dead-module rail then `./dead-module-rail run --arrive ... --final-tsv` prints the same TSV as `dl6 run`.
5. Tests: `tests/dl6_run.rs`: run once on `tests/fixtures/query_order_tail.dl6` (exists) and assert TSV;
   `--fail-on` exit code 1 on a non-empty query and 0 on empty; compile cache hit on the second call
   (count swipl spawns through a counting hook or the trace); a watch test with a temp git repo and one
   touched file producing exactly one extra tick (cap 10s).
6. Gates, pasted: `cargo test --release` (114 + yours), `bash grade.sh` (439/335 rc=0), `bash
   shared-frontier-gate.sh` (8/8), `cd v6 && just oracle-rustc && just oracle-knip` (after rewriting the
   rail script they exercise the new path), the dead-module rail 0/16/0 on hafley-rs 3 runs.
7. `docs/dl6-run.md`: one page, TOC, the two commands, flags table, three example programs (one-shot
   report, watch with `bind watch`, interval with `bind interval`), each `.dl6` snippet with its rx
   lowering comment.

## File ownership (peers live: N+1 audit owns `incremental.rs, sql.rs, serve.rs, driver.rs, trace.rs,
program.rs`; ghcacher owns `src/executors/{fetch,env,repos,checkout,toml}.rs`; crosswalk owns
`src/executors/{git_refs,git_history,repo_at,dep_crawl}.rs`)
YOURS: `src/bin/dl6.rs`, `src/bin/emit_rust_harness.rs`, `src/run.rs` (new), `src/lib.rs` (mod line),
`src/build_template/**`, `src/executors/watch.rs` + `interval.rs` (new), ONE hunk in `hosts.rs`
`executor_for`, `tests/dl6_run.rs`, `docs/dl6-run.md`, `v6/dl/deadcode/dead-module-rail.sh` and
`v6/dl/dataflow/report-extract.sh` (rewrites only), `Cargo.toml`/lock, `issues/` your issue. If
`driver.rs` needs a seam, write the diff as a request; if you cannot proceed without it, a minimal
additive function in `driver.rs` is allowed with its diff called out in the PR body.
FORBIDDEN: everything else.

## Report (PR body), tables and lists only
signatures + lifetimes; flag table; compile-cache walls; watch tick walls + RSS series (with load);
gate outputs; the rail script diff; requests.

## Addendum (user, same day): the database is the receipt
"read their sqlite views later to audit complex patterns in source code; hard receipts because I can join
any syntactic/semantic/regexp/fs/git/gh fact into any path analysis." So `--db <file>` is first-class:
8. With `--db prog.db`, the run leaves a plain SQLite file a cold `sqlite3 prog.db` can query. Today the
   decoded-text views are `CREATE TEMP VIEW "__txt_<rel>"` and vanish on close, and the base tables carry
   `__str` ids. Deliver: under `--db`, the `__txt_*` views are created as persistent `CREATE VIEW` (find the
   DDL site in `lower.pl`'s emitted boot; if the change belongs in the emitter, a `--db`-time re-creation
   from the IR's DDL list in `run.rs` is the Rust-side answer; state which and why), plus one persistent
   view per `?` query named after it (`v_<query>`) carrying the `ORDER BY`, plus a `__meta(program, source
   digest, compiler digest, tick, finished_at)` table. Receipt: `sqlite3 prog.db 'SELECT path, defs FROM
   v_rail_unproven_module LIMIT 3'` pasted, run after the process exited. A second `dl6 run --db prog.db`
   on the same program resumes from the stored tick (the engine's restart path; if none exists say so and
   start fresh with a warning). Document in `docs/dl6-run.md`: the table and view naming, the `__str`
   dictionary, three example joins across rels in raw SQL.
