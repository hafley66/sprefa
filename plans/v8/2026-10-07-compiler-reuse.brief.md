# Brief: compiler reuse (user units, registry key miss) and Send engine state

Worktree: this directory (`$PWD`), branch `perf/compiler-reuse`. Work only here.

## User rules for this lane (2026-10-07)

- Write and read code by eye. Do NOT run `cargo check`, `cargo build`, `cargo test`, or any test.
- Rust and dl7 only. No Python. No scripts committed.
- rustfmt never. Format by hand, matching each file's style. Do not reformat lines you did not write.
- Comments: only constraints the code cannot show. No dates, no change narrative.
- `eprintln!` banned in `src/**`; `tracing` only.
- Every loop/recursion you add is bounded (constant with a comment saying what it protects).
- Do not touch `sqlite_ivm/` or `hafley-rs/` (symlinks to other repos).
- Commit as you go, one commit per numbered item, plain subjects. No merges, no pushes.
- Owned paths: `src/_2_lower/**`, `src/_3_check/**`, `src/_4_comptime/**`, `src/_6_eval/**`,
  `src/_8_driver/**`, `src/lib.rs`. Forbidden: `src/_9_runtime/**`, `src/_7_effect.rs`,
  `src/bin/dl8.rs`, `std/http.dl7`, `Cargo.toml` (another lane owns them).

## Read first

- `plans/v8/2026-10-06-lower-check-a1.design.md` and `plans/v8/2026-10-06-lower-check-a1.findings.md`.
- The A1 code: `src/_2_lower/_13_keep.rs`, `src/_3_check/_9_keep.rs`, `src/_8_driver/_7_one.rs`,
  `src/_2_lower/_12_units.rs`, `src/_3_check/_0_api.rs`, `src/_4_comptime/_7_sources.rs`, `src/lib.rs`.

## Items

1. Registry key miss. A1 findings: in `dl8 build std/tsi_registry_emit.dl7` the prelude lower key
   differs on `imports`. `imports` carries rows for every unit; the prelude's lowering reads only
   rows whose importer identity is the prelude. Narrow the key to the imports rows `lower_one`
   actually reads for that unit (find the filter inside `lower_datalog`/`lower_one`; key on the
   filtered list). If `lower_one` reads all imports, report the read site instead of changing it.
2. User units. Extend the A1 lower keep from the exporter to every unit: key
   `(unit, policy, env, filtered imports)`; on a hit skip `lower_one` for that unit, both in the
   first pass and the importer rerun (`_12_units.rs:108-185`). The comptime refreeze
   (`_7_sources.rs:56`) reuses through the same map.
3. Check reuse per rule. Extend `check_with` so a refreeze whose merged program equals an earlier
   one except for a set of rules re-resolves only those rules (key resolved rules by the rule
   `TermId` plus the `TermId`s of the graph entries they read; on a graph change touching those
   owners, re-resolve). Stratify stays over the union.
4. Send engine state. `grep -rn 'Rc<RefCell' src` lists 8 sites (`OneEngine.pool: SharedPool`,
   `Stdlib`, and others). Replace `Rc<RefCell<T>>` with `Arc<Mutex<T>>` (std) so the engine state is
   `Send`; replace `Rc` clones with `Arc` clones. Leave `Universe` single-owner (`&mut`).

## Report back

Commit shas per item, files touched, and every place you were unsure whether the code compiles
(list them; the coordinator will type-check later).
