# brief: the rust checker tier loads every feature and names a file it cannot see

Lane: `fix/rust-checker-all-features`. Base: `origin/main` (coordinator states the sha).
FIRST ACTION: `git merge --ff-only <sha>`. Failure = STOP AND REPORT.
Crate: `v6/sprefa-extract`. Paths relative to it. Build and test with `--features cli,rust-checker` (380s cold; the shared boop target `~/.cache/boop/cargo-target` may already hold it).

## The incident

Hand probe 2026-09-03, binary with `cli,rust-checker`, from the crate root:
`extract --witness --resolve --family type --project-root . --rust-checker src/trail.rs`
emits run 1 `semantic rust-analyzer`, six `partial` coverage rows, zero `fact` rows, zero diagnostics naming the file. `src/lib.rs:46-47` declares `#[cfg(feature = "cli")] pub mod trail;`. `src/lang/rust_checker_ra.rs:44-48` builds `CargoConfig { sysroot, set_test: true, ..Default }`, whose `features` is `CargoFeatures::Selected { features: [], no_default_features: false }` (`ra_ap_project_model` `cargo_workspace.rs:86`). The module is absent from the loaded crate graph, `sema.file_to_module_defs(file_id)` at `rust_checker_ra.rs:377` yields nothing, `files_answered` still counts the file (`:107`), and the tier reports a loaded run that saw nothing.

## Two rows, two commits

### 1. Load with every feature

`rust_checker_ra.rs:44`: `features: CargoFeatures::All` in the `CargoConfig`. Extraction wants the union of what the crate can be; a feature-gated module is code. Say so in one comment. If `All` breaks the workspace load for a fixture (mutually exclusive features in a dependency), fall back per the error to `Selected` with `no_default_features: false` and report the case in the PR body; do not silently keep the default.

### 2. A supplied file that owns no module is a diagnostic

In `run` (`rust_checker_ra.rs:375`), a `WalkFile` whose `file_to_module_defs` is empty is recorded on `CheckerAnswers` (`rust_checker.rs:49` area) as `unmodulated: Vec<String>` (the supplied path). `project.rs:687` `load_rust_checker` turns each into a `DiagnosticOut { run: <the semantic run>, relation: "tier.rust-analyzer", detail: "<path>: owns no module in the loaded crate graph (cfg-gated, or outside every crate root)" }`. The decline plumbing from PR #674 (`project.rs:173` `TierDecline`, `:339`, `:671`) files on run 0 for a tier that never loaded; this row is on the semantic run, because the tier did load. Read `envelope` (`:567`) and put the rows after the tier's coverage rows.

## Receipts

1. Fixture: `tests/fixtures/tsi/rust_probe/Cargo.toml` gains `[features] gated = []`; `src/lib.rs` gains `#[cfg(feature = "gated")] pub mod gated;`; new `src/gated.rs` with one `pub struct Gated { pub id: u64 }` and one `pub fn make() -> Gated`.
2. New `tests/107_rust_checker_features.rs` under `#![cfg(feature = "rust-checker")]`, SABOTAGE RECEIPT header stating the base sha and that on it the run below emits zero facts for `gated.rs` and zero diagnostics naming it.
   - `a_feature_gated_module_is_walked`: `--witness --resolve --family type --project-root tests/fixtures/tsi/rust_probe --rust-checker tests/fixtures/tsi/rust_probe/src/gated.rs` emits `tsi.product` for `Gated` and `tsi.callable` for `make` on the semantic run.
   - `a_file_outside_every_crate_root_is_named`: force the empty-module case some other way (a `.rs` file in the fixture dir not reachable from `lib.rs`, say `src/orphan.rs`, never declared) and assert exactly one `diagnostic` on the semantic run, relation `tier.rust-analyzer`, detail containing `orphan.rs`.
   - `a_walked_file_files_no_diagnostic`: `lib.rs` alone, zero `tier.*` diagnostics.
3. `cargo test --features cli,rust-checker --test 102_rust_semantic_tsi --test 100_tsi_intersection --test 104_tier_decline_diagnostic --test 107_rust_checker_features` green; the `102` pinned sets must not move (the fixture's default crate graph is unchanged by an unused feature; if a set moves, say why).
4. The hand probe above on `src/trail.rs` from the crate root, relation counts of run 1 pasted in the PR body (before: zero).
5. Full battery `cargo test --features cli` in the background, `tail -30` pasted. `git diff --stat origin/main...HEAD` lists no golden.

## Ownership

Owned: `src/lang/rust_checker_ra.rs`, `src/lang/rust_checker.rs`, `src/project.rs` (the diagnostic rows only), `tests/107_rust_checker_features.rs`, `tests/fixtures/tsi/rust_probe/**`.
Forbidden: `src/lang/rust_type_edges.rs`, `tests/99_syntax_tsi_rows.rs`, `tests/106_*`, `tests/fixtures/tsi/probe_graph.rs` (lane `fix/rust-syntax-type-graph` owns them), `src/tsi/**`, `src/wire.rs`, `src/lang/ts*.rs`, `v7/**`, `docs/**`, `v6/prolog/ARCH.pl`.

## Style laws

No em dashes. Comments state only constraints the code cannot show. `tracing` only. Descriptive names. Banned words: provenance, substrate, load-bearing, regime, refusal, "ground truth"; "support" is banned. Commit subjects: `extract: the rust checker tier loads every cargo feature`, `extract: a supplied file owning no module is a diagnostic`.

## Done

Push, PR against `main` with receipts, then:
`boop beep --no-wait --as fix-rust-checker-all-features sprefa-coordinator "rust-features PR #<n>: 107 <n>/<n>, 102 <n>/<n>, trail.rs run 1 facts <n>, battery <pass>/<total>"`.
