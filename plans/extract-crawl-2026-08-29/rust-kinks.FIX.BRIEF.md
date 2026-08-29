# Brief: rust crawl kinks 3, 4, 5, 6, 7 (lane `fix-extract-rust-crawl`)

Read `plans/extract-corpus-2026-08-28/COMMON.md` (style laws, 10-second law).
Findings come from `rust.REPORT.md` in your tree; read its kinks table first.

## First action
```
git merge --ff-only c60e5c4cc
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```
Failure: STOP, `boop beep --no-wait --as fix-extract-rust-crawl sprefa-coordinator "<one line>"`.
(If the build cannot find `../../../hafley-rs`, say so in the hail; the
coordinator adds the symlink.)

## Method
Every fix: failing test FIRST (red output pasted in the commit body), fix,
green, one commit per fix. Fixtures under `tests/fixtures/<lang>_findings/`
already exist for most rows; reuse them. Never weaken a golden or parity
test; regenerate `tests/fixtures/kind_vocab/wire_golden.jsonl` only by the
procedure `tests/6_kind_vocab.rs` documents and state the hunk count. Run
the gate as `cargo test --features cli --no-fail-fast` and report the SUM
over all binaries. No whole-crate `cargo fmt`. No subagents.

## Files you own
`v6/sprefa-extract/src/lang/rust.rs`, `src/lang/rust_resolve*.rs` if such
a file exists, new `tests/52_rust_crawl_kinks.rs`,
`tests/fixtures/rust_findings/**`, `tests/fixtures/rust/*.v5.jsonl` (only
with a stated reason). Forbidden: every other file, including
`src/project.rs` and `src/wire.rs`.

## Kinks (rust.REPORT.md section 7, fixtures already in `rust_findings/`)
- Kink 4, wrong_fact: `callee_path` on a qualified call
  (`rustc_wrapper::main()` at `crates/rust-analyzer/src/bin/main.rs:30`) is
  ignored by resolve, so the edge binds to whichever `main` the name index
  returns; 294 wrong edges. Fix: when a site carries `callee_path`, restrict
  candidates to defs whose module path (file path segments + `mod` scope
  owner, see `tests/30_rust_mod_scope_owner.rs`) ends with that path.
  Fixture `rust_findings/qualified_path/`.
- Kink 3, missing_fact: an edge whose caller is a closure (`closure@<n>`)
  ends the crawl; 6,973 edges, 934 callees only reachable through one.
  Fix: ALSO emit the edge from the closure's enclosing named def (one extra
  edge per closure-caller edge, `kind: name_resolve`, caller = the
  enclosing fn), so a BFS over named defs passes through. Keep the closure
  edge. COUNT test: edges before/after on the fixture.
- Kink 5 + 6, missing_fact: a fn inside `const _: () = { ... }` mints no
  def (`rust.rs:1082` `call_defs_in_items`); a call in a `static`/`const`
  initializer has no caller (`rust.rs:1031` bail). Fix both: walk const
  and static item bodies for defs; give initializer calls the const/static
  item as caller. Fixtures `rust_findings/static_init_call.rs`, `const_*`.
- Kink 7: the rust arm emits zero `unresolved` rows. Emit one per site
  that resolves to nothing, with reason from the classes in section 4 of
  the report (`no_corpus_def`, `ambiguous`), the same way `src/lang/ts.rs:1194`
  does. Fixture `rust_findings/unresolved_reason.rs`. If `--resolve` still
  drops phase-1 `unresolved` rows at the CLI (PR #532 F6), the per-file
  `extract FILE` stream is where the test reads them.
- Kink 2 (macro bodies) is OUT of scope: report it, do not start it.
Receipt: rerun `plans/extract-crawl-2026-08-29/rust.crawl.py` over
~/projects/rust-analyzer with your binary; reachability before/after
(before: 10,928 of 19,190) in the Fixes table.

## Deliverables
Commits as above; append a Fixes table (kink / before / after / test) to
the report named at the top; push; `gh pr create --base main`; hail
`boop beep --no-wait --as fix-extract-rust-crawl sprefa-coordinator "fix-extract-rust-crawl: PR #N, <fixes>, gate <p>/<f>"`.
