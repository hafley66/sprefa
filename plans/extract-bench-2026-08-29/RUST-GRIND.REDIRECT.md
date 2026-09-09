# Redirect (user decision 2026-08-30 ~15:00): rust goes checker-tier

The user's words: scip is the standard for modules/references; calls and types
are syntactically variable; rust is complex, so rust runs WITH the language's
own machinery. Diet (syntax-only) stays the design for go and ts. For rust,
stop syntax-grinding after the class you have open and build the codeql-shaped
leg:

## Arc 2' (replaces arcs 2 and 3): ra_ap-backed rust resolve
- Finish and commit the variant-names class you pinned, then stop syntax work.
- Post what you have as PR 1 (arc 1 codeql row + the variant class), hail.
- Then, on a branch from it (`fix/extract-rust-grind-2`): wire rust-analyzer
  in-process into the rust resolve arm, the way codeql's extractor does:
  `ra_ap_load_cargo` workspace load from `cargo metadata` (NO cargo build),
  `ra_ap_hir`/`ra_ap_ide` for receiver types, trait solving, and def targets.
  The crate deps and a working load already exist in this repo:
  `plans/extract-bench-2026-08-29/ra_ide_probe/` and
  `src/lang/rust_scip_macros.rs` (read both first). Buy, never rebuild:
  no bespoke trait solver, no bespoke inference.
- Seam: same shape as the scip leg. The checker answers ride behind the
  existing resolve seam; when the workspace load fails (no Cargo.toml, broken
  metadata), the arm falls back to the current syntax leg with one
  tracing::info line. `unresolved{reason}` rows stay for what ra cannot name.
- The workspace load is an index-build-class operation (SCIP exception to the
  10-second law): measure and report it separately (load wall, load RSS).
  The per-run resolve after load must still be reported against the 10 s /
  700 MB ceilings; if the load must happen per run, report the honest total
  and name it as the cost of this tier.
- Receipts per commit: ratchet rust rows vs all three oracles
  (ra_ap_ide oracle, scip, codeql). Recall expected to jump toward 88-100%;
  RATCHET_FORCE for the rust rows with the user decision cited in the commit
  body. Wall/RSS table. Gate.
- PR 2 body carries the tier table: syntax leg vs checker leg vs codeql, same
  oracles, plus load cost. Hail after each PR.

Ownership grows by: `src/lang/rust*.rs`, `Cargo.toml` (ra_ap_* deps move from
the probe into the crate), `tests/7N_rust_*`, rust plans, RATCHET rust rows.
Everything else in the original brief stands (one process at a time, nice 15,
timeout, no fmt, laws).
