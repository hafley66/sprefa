# Brief: lower + check A1 (prelude and macrotime library lowered and checked once per process)

Design: `plans/v8/2026-10-06-lower-check-a1.design.md` (read it whole first). Atlas:
`plans/v8/2026-10-06-phase-atlas.md`, gap "lower + check".

Worktree: this directory (`$PWD`), branch `perf/lower-check-a1-omp`, based on main.
Work only here. `CARGO_TARGET_DIR` is this worktree's `target/`.

## Goal

Per `dl8 compile plans/v8/probes/2026-10-05-one-dd-engine/0_hello.dl7` today: 6 `lower_compiler_units`
and 6 `check_datalog` calls (table in the design). Under `DL8_ONE_ENGINE` default (on), make the
prelude and the macrotime library lower and check once per input key per process, with the results
held on `OneEngine` in plain maps keyed by interned `TermId` tuples.

1. Lower: a map on `OneEngine` keyed `(unit, policy, env, imports)` holding the exporter's lowered
   basement and origins. `lower_units_with_exporter` (`src/_2_lower/_12_units.rs:85`) takes the
   exporter result from the map when present instead of calling `lower_one` on it.
   Rows 1/4 vs 2/5 of the design table must hit the same key when inputs are equal; if they do not
   (imports, env differ), report the differing component instead of forcing it.
2. Check: `check_with(base, delta)` per the design. Keep the prelude's `CheckerGraph`, arities,
   resolved rules, seeds, edges. For a program, resolve only non-prelude rules/seeds/edges against
   the union graph; stratify over the union. Runtime guard: any delta edge whose owner is in the
   prelude owner set takes the full `check_datalog` path. Prelude rules hold indexes `0..n_prelude`
   in the merged list (`_12_units.rs:99`, `:364-376`); assert that and fall back to the full path
   if it does not hold.
3. Output must be byte-identical to today: same `Checked` contents and order.

## Receipts (counts only, no wall clock)

- Calls per hello process before/after, per row of the design table: number of `lower_one` runs
  on the prelude and macrotime library, number of full `check_datalog` runs vs `check_with` runs.
  Count with a `tracing` event or a counter in hafley-observe (`sqlite_work`-style); no debugger,
  no lldb.
- Same counts for `dl8 build std/tsi_registry_emit.dl7`.
- Write them into `plans/v8/2026-10-06-lower-check-a1.findings.md`.

## Laws

- Type-check only: `cargo check` / `cargo build --bin dl8`. Do not run test suites; the
  coordinator runs the one e2e check (132-program census + registry cmp).
- Rust only. No Python, no scripts committed.
- rustfmt only the files you touched. No tree-wide formatting.
- No other caching or memoizing beyond the two maps above. No on-disk state.
- Every loop or recursion you add is bounded (repo `CLAUDE.md`).
- `eprintln!` banned in `src/**`; use `tracing`.
- Do not touch `sqlite_ivm/` or `hafley-rs/` (symlinks to other repos).
- Comments: only constraints the code cannot show; no dates or change narrative.
- Commit on `perf/lower-check-a1-omp` with plain subjects. No merges, no pushes.
- If a step turns out impossible as designed, stop and report the throw site / the differing
  key component instead of working around it.

## Report back

Commit shas, the counts table, any key component that differed, and anything that fell back to
the full path on hello or the registry.
