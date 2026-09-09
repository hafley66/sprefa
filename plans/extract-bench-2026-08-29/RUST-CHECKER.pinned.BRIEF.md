# Lane `fix-extract-rust-checker` (opus-max): rust resolve gets the checker tier

User decision (2026-08-30 ~15:00, verbatim intent): scip is the standard for
modules/references; calls and types are syntactically variable; rust is
complex, so the rust arm runs WITH the language's own machinery. Diet
(syntax-only) stays the design for go and ts. This lane wires rust-analyzer
in-process into the rust resolve arm, the way codeql's extractor does.

Standing after the grind lane (RATCHET.tsv at 02d162f2e): ra_ap_ide
75.66% recall / 49.77% precision, scip 78.90% / 45.24%, codeql-as-tool
64.96% / 76.15%. The scip experiment read ~88% recall with borrowed checker
answers; this tier's target is at or past that.

## First action
```
git merge --ff-only 02d162f2e84647b3a08a40f04b181422c35a0cea
cd v6/sprefa-extract && nice -n 15 cargo build --release --features cli 2>&1 | tail -1
```
Read: `plans/extract-bench-2026-08-29/RUST-GRIND.REDIRECT.md` (the spec),
`ra_ide_probe/` (a working ra_ap workspace load + call hierarchy),
`src/lang/rust_scip_macros.rs` (ra_ap already linked in-crate),
`plans/extract-crawl-2026-08-29/rust.REPORT.md` sections 20-23,
`src/scip*.rs` for the seam shape to copy.

## Build
- `ra_ap_load_cargo` workspace load from `cargo metadata` (NO cargo build);
  `ra_ap_hir`/`ra_ap_ide` answer receiver types, trait targets, def sites.
  Buy, never rebuild: no bespoke trait solver, no bespoke inference.
- Seam: same shape as the scip leg. Checker answers ride behind the existing
  resolve seam; load failure (no Cargo.toml, broken metadata) falls back to
  the syntax leg with one `tracing::info` line. `unresolved{reason}` rows
  stay for what ra cannot name.
- The workspace load is an index-build-class cost (SCIP exception to the
  10-second law): report load wall + load RSS separately. Per-run resolve
  after load reports against the 10 s / 700 MB ceilings; if load happens per
  run, report the total and name it as this tier's cost.
- Receipts per commit: ratchet rust rows vs all three oracles;
  recall expected to jump toward 88-100% vs ra_ap_ide. `RATCHET_FORCE` on
  rust rows only with this user decision cited in the commit body. Gate
  (`nice -n 15 cargo test --release --features cli`, background, log;
  wall-ratio flakes rerun 3x isolated).

## Rules that end a lane if broken
One heavy process at a time, `nice -n 15`, `timeout 120` on extract runs,
`timeout 900` on workspace loads (background, log). Never edit oracle tsvs.
No `cargo fmt` on files you do not own. No file over 1 MB in git. PRs BASE
MAIN, never stacked on another branch (stacked bases auto-retarget on squash;
it bit twice today).

## Deliver
PR(s) on `fix/extract-rust-checker`, base main. Body: tier table (syntax leg
vs checker leg vs codeql, same oracles), load cost, wall/RSS, gate summary.
Hail after each PR:
`boop beep --no-wait --as fix-extract-rust-checker sprefa-coordinator "rust checker: PR #N, ra r/p a/b, load c s / d MB, resolve e s / f MB, gate x/y"`.

## Ownership
`src/lang/rust*.rs`, `Cargo.toml` (ra_ap deps), rust tests/fixtures, rust
plans, RATCHET rust rows. NOT `go*`, `ts*`, `scip*.rs` beyond the seam
touchpoint, `types.rs` beyond the seam type, `project.rs` beyond the seam
plumbing, `tests/bench/mod.rs` beyond a rust-only helper.
Laws: no em dashes anywhere, no eprintln (tracing only), descriptive names,
comments only for what code cannot show, no words
provenance/substrate/load-bearing/regime/refusal, never "ground truth".
