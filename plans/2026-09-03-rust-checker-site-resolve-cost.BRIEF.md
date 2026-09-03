# brief: where the rust checker's 138 seconds per file go

Lane: `fix/rust-checker-site-resolve-cost`. Base: `origin/main` (coordinator states the sha).
FIRST ACTION: `git merge --ff-only <sha>`. Failure = STOP AND REPORT.
Crate: `v6/sprefa-extract`. Build and test with `--features cli,rust-checker`.

## The measurement (PR #683, lane fix-rust-checker-walk-by-file, 2026-09-03)

`RUST_LOG=sprefa_extract=info extract --witness --resolve --family type --project-root . --rust-checker src/trace.rs`, one supplied file of 254 lines:

| run | walk_ms | rust.impl |
|---|---|---|
| item walk on, impls by supplied file | 133874, 140017, 141119 | 7 |
| item walk OFF (no --witness) | 138310 | n/a |
| item walk on, whole crate (before #683) | 116880 | 1139 |

So the item walk (types, impls, callables) is not the cost. `walk_ms` is `walk_file` (`src/lang/rust_checker_ra.rs:168`): for EVERY syntax node of the file, `MethodCallExpr` -> `sema.resolve_method_call`, `CallExpr`/`RecordExpr` path -> `sema.resolve_path`, and every other `ast::Path` -> `sema.resolve_path` + `try_to_nav` (`type_ref`, `:245`). 113 method sites, `method_unresolved=0`, `external=259`. About 1.2 s per method site if the method sites own it, which nobody has measured. The 10-second law names this a defect; the workspace LOAD (1.9 s) is the only leg with the SCIP exception.

## Step 1, measure before touching anything (its own commit)

Phase spans exist (`src/trace.rs` `Phase`, `phase_span`, `record_phase`; the trail brief `plans/2026-09-03-extract-trail.BRIEF.md`). Add `Phase::CheckerSite` (or the closest name the enum's style wants) and wrap, separately, with `calls` counted: `resolve_method_call`, `resolve_path` on call/record paths, `resolve_path` on bare paths (`type_ref`), `try_to_nav`. `extract --bench` then prints one row per span with files/calls/ms. Paste that table for `src/trace.rs`, three runs, load average beside each. Also count how many `ast::Path` nodes `walk_file` visits versus how many are in type position (`ast::PathType` ancestor) versus expression position: `descendants()` visits every path in every expression body, and `type_ref` resolves each one.

Then answer, in the PR body, which of these owns the time:
- (a) first-touch inference per function body (`resolve_method_call` and `resolve_path` inside a body both trigger `infer` for that body once; later sites in the same body are cheap): the sign is a few sites paying seconds and the rest paying microseconds. Measure per-site `micros` and paste the top 10.
- (b) `type_ref` over expression-position paths: thousands of `resolve_path` calls that the syntax leg already answers. The sign is `calls` on that span in the thousands.
- (c) trait solving under `CargoFeatures::All` + `set_test: true` (PR #680 widened the crate graph; `external` rose 77 -> 259).
- (d) `try_to_nav` (it re-parses the destination file per call).

## Step 2, the fix the measurement names (second commit, only what (a)-(d) proves)

- (b): resolve a bare `ast::Path` only when it sits under `ast::PathType` (type position); expression-position paths are the syntax leg's and the call/record arms above already cover the call-shaped ones. Receipt: the resolve goldens (`tests/93_rust_checker_wiring.rs`, `94_rust_checker_types.rs`, `95_rust_macro_callers.rs`) are unchanged, or every changed row is listed with the reason.
- (d): memoize `try_to_nav` per `ModuleDef` in a `HashMap` on the walk (the def set is small).
- (a) or (c): report the numbers and the per-body cost; if a body's inference is over 1 s, name the function and paste `RA_PROFILE` or the span breakdown. Do not "fix" inference by turning features back off.

The receipt for whichever fix lands: `walk_ms` on `src/trace.rs`, three runs, in the PR body, before and after. A test in `tests/108_rust_checker_walk_by_file.rs` or a new `tests/109_rust_checker_site_cost.rs` that pins the COUNT of `resolve_path` calls on `rust_probe/src/lib.rs` through the phase table (counts, never wall time, per the "formerly-quadratic paths get COUNT tests" law), with a SABOTAGE RECEIPT header stating the count on the base sha.

## Ownership

Owned: `src/lang/rust_checker_ra.rs`, `src/lang/rust_checker.rs`, `src/trace.rs` (the new `Phase` variant and nothing else), `tests/108_rust_checker_walk_by_file.rs`, `tests/109_rust_checker_site_cost.rs`, `tests/fixtures/tsi/rust_probe/**`.
Forbidden: `src/project.rs`, `src/lang/rust_type_edges.rs`, `src/tsi/**`, `src/wire.rs`, `src/trail.rs`, `tests/31_tracing.rs` (its phase pins may need one new row: if so, say so and add ONLY that row), `v7/**`, `docs/**`, `v6/prolog/ARCH.pl`.

## Style laws

No em dashes. Comments state only constraints the code cannot show. `tracing` only, no `eprintln!`. Descriptive names. Banned words: provenance, substrate, load-bearing, regime, refusal, "ground truth"; "support" is banned. Commit subjects: `extract: span every rust checker site resolution`, then `extract: <the fix in six words>`.

## Done

Push, PR against `main` with the measurement table and the answer to (a)-(d) first, then:
`boop beep --no-wait --as fix-rust-checker-site-resolve-cost sprefa-coordinator "site-cost PR #<n>: owner=<a|b|c|d>, trace.rs walk_ms <before> -> <after> x3, 108/109 <n>/<n>, battery <pass>/<total>"`.
