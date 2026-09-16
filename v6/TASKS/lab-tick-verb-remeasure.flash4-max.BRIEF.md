# lab-tick-verb-remeasure (pass 1 of 2; a coordinator design review follows)

You are lane `lab-tick-verb-remeasure`. Coordinator is `sprefa-coordinator`.
Base sha 9e4b468157bb2a189960b8ec69daad10af372862. Branch `lab/tick-verb-remeasure`.
FIRST ACTION: `git merge --ff-only 9e4b468157bb2a189960b8ec69daad10af372862`; on failure STOP and hail.

## Goal
Re-measure the ghcache 14-tick fold and the wide_64 3-tick fold on CURRENT main, per verb, medians of 3, and report where every microsecond of the fold wall goes: SQLite time vs Rust time. Numbers only. No code change to src/**.

## Why
The last per-verb table is from probe 89e3074ee (before PRs #437-#441 landed). It read: fold statements 6738, publish 8714 us / 319 calls, tick-only SQLite 77,814 us, fold wall 91.3 ms. The coordinator needs the same table on main today, plus the Rust-side remainder (wall minus SQLite) per tick.

## Recipe (reuse exactly)
1. `git show 89e3074ee:v6/sprefa-engine-rs/PROBE-delta-read-inmem-carry.md > /tmp/PROBE-recipe.md` and read its "ghcache, 14-tick fold", "wide_64, 3-tick fold" and "Per-verb table" sections. Reproduce the SAME cells with the SAME commands it names. If the doc does not name a command for a cell, derive it from `v6/dl/ghcache/gate.sh` (fold line: `DL_ADAPTERS_DIR=... RUST_LOG=sprefa_engine_rs=info target/debug/emit_rust_harness <program.rs> <schedule.json> --final`) and from `v6/sprefa-engine-rs/tests/trace_summary.rs` (per-verb summary through `sprefa_engine_rs::trace`).
2. Build once: `cd v6/sprefa-engine-rs && cargo build --release --bin emit_rust_harness`. Use `--release` for every timing cell; say so in the report.
3. Run each fold 3 times, interleaved (ghcache, wide_64, ghcache, wide_64, ...). Every run in the background with `timeout 60`. Never foreground-wait over 10 s.
4. Table cells: fold statements, per-verb `us/calls` (all verbs the trace emits), tick-only SQLite us, fold wall ms, and `wall_ms - sqlite_ms` per tick as a separate column.

## Deliverables
- `v6/sprefa-engine-rs/LAB-tick-verb-remeasure.md`: TOC, the two fold tables, the per-verb table, a third table "Rust remainder per tick" (tick, wall us, sqlite us, remainder us), the exact commands, and the three raw run logs referenced by path under the same folder (`LAB-tick-verb-remeasure.runs/`).
- Commit and push the branch. Post a DRAFT PR titled `lab: tick per-verb remeasure on main` with the tables in the body. No src/** changes.

## Yield results over time (mandatory)
Hail at each milestone, one line each:
1. after the first ghcache run: `boop beep hail sprefa-coordinator --from lab-tick-verb-remeasure --body "run1 ghcache: stmts=<n> wall=<ms> sqlite=<us>"`
2. after the first wide_64 run: same shape.
3. after the medians land: the per-verb top 5 by us.
4. done: PR number.
If any run exceeds its timeout or a command from the recipe does not exist on main: STOP, hail the exact error text, do not improvise.

## You own
`v6/sprefa-engine-rs/LAB-tick-verb-remeasure.md`, `v6/sprefa-engine-rs/LAB-tick-verb-remeasure.runs/**`. Forbidden: everything else. Do not edit src/**, tests/**, v6/prolog/**, v6/dl/**.

## Style laws (CLAUDE.md)
No em dashes. Banned words: provenance, substrate, load-bearing, regime, ground truth (say oracle), refusal, support (say refCount), honest, grounded. rxjs/prolog/SQL vocabulary only. Doc opens with a TOC. Tables over prose. No narrative of what you tried.
