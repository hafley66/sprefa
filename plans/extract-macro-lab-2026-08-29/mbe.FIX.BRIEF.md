# Brief: rust macro_rules expansion in the call arm (lane `feature-extract-rust-mbe`)

Read `plans/extract-corpus-2026-08-28/COMMON.md` and
`plans/extract-macro-lab-2026-08-29/PLAN.md` Option 1 (the lab's receipts;
the lab crate's last copy is commit `dae353d75`, `v6/sprefa-extract/labs/macro_expand/`,
read it with `git show dae353d75:<path>`, copy code from it freely).

## First action
```
git merge --ff-only 0debe6e50ef20c7ab2448577b1aadde33a7c2fc1
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```
Failure: STOP, `boop beep --no-wait --as feature-extract-rust-mbe sprefa-coordinator "<one line>"`.
Binary: `v6/sprefa-extract/target/release/extract` in YOUR worktree.

## Ownership
Yours: new `src/lang/rust_mbe.rs`, `Cargo.toml` (add
`ra_ap_{syntax,mbe,tt,parser,span,syntax-bridge,intern}` + `salsa` at the
versions the lab pinned), `tests/58_rust_mbe.rs`,
`tests/fixtures/rust_findings/mbe/**`, and in `src/lang/rust.rs` ONLY the
call-arm hook that runs expansion before the call walker. Forbidden: every
other file under `src/`. Another lane owns `rust.rs`'s resolve arms and
`use` handling right now; touch nothing below `impl Resolve` there.

## Build
1. `rust_mbe::expand_file(content) -> Option<Expanded>`: the lab's pipeline
   (parse with `ra_ap_syntax`, collect `macro_rules!` defs in THIS file,
   expand each invocation with `mbe::expand` to a fixpoint, splice into the
   text). `Expanded` carries the spliced text plus a byte-offset map
   (spliced offset -> original invocation span) so every def and site minted
   inside an expansion reports the INVOCATION's span in the original file.
   Macros defined in another file, `format!`/`vec!`/derives/`include!`
   are not expanded; the site stays as it is today.
2. Hook in `project_call` for rust: run `expand_file`; when it returns
   `Some`, walk the spliced text; map spans back. Defs and sites gained
   carry a new `origin: macro` marker (add ONE field to the CallF def/site
   row only if `--schema` already has a place for it; otherwise emit a
   `macro_site` aux row `{span, macro_name}`; say which in the PR body).
3. Cap: expansion output over 4x the input bytes or a fixpoint past 8
   passes stops and emits an `unresolved` row reason `macro_budget`.

## Tests, fail-first, commit per step
`tests/58_rust_mbe.rs` over the lab's f1..f8 fixtures (copy them from
`dae353d75`): sites and defs per fixture equal the lab's "expanded" column;
every gained span lies inside its invocation's span; f2/f4/f5/f6/f8
unchanged; a recursive macro hits `macro_budget`. COUNT: expansion is one
parse per file, wall(873 files) under 2 s.

## Receipt
Rerun `plans/extract-crawl-2026-08-29/rust.crawl.py` over
`/Users/chrishafley/projects/rust-analyzer` with YOUR binary: call sites
133,102 -> n (lab: 137,945), reachable union 12,221 -> n. Append section
14 to `plans/extract-crawl-2026-08-29/rust.REPORT.md`. Gate
`cargo test --features cli --no-fail-fast` in background, SUM. Push,
`gh pr create --base main`, hail
`boop beep --no-wait --as feature-extract-rust-mbe sprefa-coordinator "rust mbe: PR #N, sites 133,102-><n>, union 12,221-><n>, gate <p>/<f>"`.

## Laws (inline)
No em dashes. No `eprintln!`. Comments state constraints only, no dates.
Descriptive names. Every `extract` call under `timeout 10`. No `cargo fmt`
outside files you own. Never `--no-verify`.
