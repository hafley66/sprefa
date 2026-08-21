---
created: 2026-08-21
updated: 2026-08-21
type: feature
reporter: chris
assignee: chris
status: open
priority: normal
epic: cheap-fast-analysis
---

# dl6 run and dl6 watch: a program runs once or watches, from one command

## Description


"i want to start making dataflow programs that watch or run 1 time."

Today that takes `dead-module-rail.sh`: a swipl compile, a cargo build of the
harness, `emit_rust_harness <rs> --arrive ... --live-hosts --final-only
--final-tsv`, and env vars for the adapters and the extract binary. It must be
two commands.

    dl6 run   prog.dl6 [--arrive rel=v,v ...] [--final-tsv] [--fail-on q] [--db f]
    dl6 watch prog.dl6 [same flags] [--root dir]

`run` compiles (cached by source digest), loads the program in-process, runs the
hosts live, prints the finals and exits; exit 1 when `--fail-on <query>` names a
query with rows. `watch` folds, then stays up: `bind watch(glob)` rels take
arrivals from soopy's watcher, `bind interval(secs)` from a monotonic clock, and
the finals re-print as `+`/`-` TSV deltas.

## Addendum: the database is the receipt

"read their sqlite views later to audit complex patterns in source code; hard
receipts because I can join any syntactic/semantic/regexp/fs/git/gh fact into any
path analysis."

`--db <file>` leaves a plain SQLite file a cold `sqlite3` queries: the decoded
`__txt_*` views persistent rather than TEMP, one `v_<query>` view per `?`
carrying its `ORDER BY`, and a `__meta` table naming the program and both
digests.

## Where it landed

`v6/sprefa-engine-rs/src/run.rs` is the one implementation; `dl6 run`, `dl6
watch`, `emit_rust_harness` and every `dl6 build` binary are its callers.
Documented at `docs/dl6-run.md`.
