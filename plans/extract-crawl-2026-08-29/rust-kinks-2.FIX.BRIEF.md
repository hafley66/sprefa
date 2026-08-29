# Brief: rust crawl kinks 4 and 7 on the new path seam (lane `fix-extract-rust-crawl-2`)

Read `plans/extract-corpus-2026-08-28/COMMON.md` (style laws). Findings and
the blockers the previous lane cited: `plans/extract-crawl-2026-08-29/rust.REPORT.md`
sections 7 and 11.3. The blocker is gone: PR #546 added
`IndexBag` blob -> path index (`src/types.rs`, `PathIndex`; set in
`resolve_project`, `src/project.rs`) and `src/lang/go.rs` shows the
import-qualified leg reading it. Copy that shape.

## First action
```
git merge --ff-only b9b98e3af
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```
Failure: STOP, `boop beep --no-wait --as fix-extract-rust-crawl-2 sprefa-coordinator "<one line>"`.

## Files you own
`v6/sprefa-extract/src/lang/rust.rs`, `src/types.rs` (two `UnresolvedReason`
variants + their wire tags only), `src/schema.rs:162` vocabulary line,
`src/project.rs` (only if the unresolved channel needs a resolve-phase
seat; state why), `tests/52_rust_crawl_kinks.rs`, `tests/fixtures/rust_findings/**`,
`tests/6_kind_vocab.rs` EXT_KINDS row for `const_init` (section 11.4 names it),
`tests/fixtures/kind_vocab/wire_golden.jsonl` (regen by the documented
procedure, hunk count stated). Forbidden: `src/lang/ts.rs`, `ts_resolve.rs`
(another lane is live there), every other language arm. No whole-crate fmt.
No subagents.

## Kink 4, wrong_fact: `callee_path` ignored on qualified calls
`rustc_wrapper::main()` at rust-analyzer `crates/rust-analyzer/src/bin/main.rs:30`
binds to whichever `main` the name index returns; 294 wrong edges. Rule:
when the site carries `callee_path` (`a::b::f`), candidates are defs whose
file path segments (minus `.rs`, with `mod.rs`/`lib.rs`/`main.rs` collapsing to
their dir, plus the `mod` scope owner from `tests/30_rust_mod_scope_owner.rs`)
end with `a::b`; `crate::`, `self::`, `super::` resolve relative to the
caller's file path; an external crate prefix (not a corpus path) resolves to
nothing. Fixture `rust_findings/qualified_path/`. Fail-first.

## Kink 7, missing_fact: the rust arm emits zero `unresolved` rows
89,500 of 138,223 sites resolve to nothing silently. Add `UnresolvedReason`
variants `no_corpus_def` and `ambiguous` (closed vocabulary, one issue row
in `issues/` per the doc comment at `src/types.rs:566-572`, cite it) and
emit one `unresolved` row per site the resolve leg drops, from wherever
the drop happens; if that is phase 2, add the resolve-phase seat in
`project.rs` and say so. Fixture `rust_findings/unresolved_reason.rs`.
COUNT test: rows == sites - edges on the fixture set.

## Receipt
Rerun `plans/extract-crawl-2026-08-29/rust.crawl.py` over
`~/projects/rust-analyzer` (before: 59,506 edges, 12,221 of 19,339 reachable;
wrong-file edges 294): edges, reachability, and the count of edges whose
callee file disagrees with the site's `callee_path` (target 0), in a Fixes
table appended to rust.REPORT.md. Gate `--no-fail-fast`, SUM. Push,
`gh pr create --base main`, hail
`boop beep --no-wait --as fix-extract-rust-crawl-2 sprefa-coordinator "rust kinks 4+7: PR #N, wrong-file edges 294-><n>, unresolved rows <n>, gate <p>/<f>"`.
