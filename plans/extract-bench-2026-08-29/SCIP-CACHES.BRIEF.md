# Lane `fix-extract-scip-caches` (glm53f): the scip caches live in the index, not in process-global statics

Audit (coordinator, 2026-08-30 11:50) of the day's diff found three items to
fix in one lane. No behavior change: sorted jsonl must be byte-identical to
the origin/main binary on all three corpora, on every commit.

## First action
```
git merge --ff-only <BASE_SHA>
cd v6/sprefa-extract && nice -n 15 cargo build --release --features cli 2>&1 | tail -1
```
Baseline binary for identity: `CARGO_TARGET_DIR=/tmp/base-target` build of
origin/main in a detached worktree, or the one under `/tmp` if #590 left it.
Driver and index paths: `plans/extract-bench-2026-08-29/out/scip_runs.sh`.
Read `SCIP-SPEED.REPORT.md` sections 5-9 first.

## Item 1: `scip.rs` `DOC_CACHES` and `DEF_MAPS`
Both are `static Mutex<Option<HashMap<..>>>` keyed on the raw address of a
`ScipDocument` / `ScipIndex` (`doc as *const _ as usize`) plus a fingerprint
(`DocKey`, `IndexKey`), and nothing ever evicts them. Move them into the
index: `ScipDocument` gains the sorted `(start, end, SymbolId)` span vector
and its `LineTable` as fields; `ScipIndex` gains the symbol->def map. Build
all of them once at the end of `scip_decode::load_index` (or lazily via a
per-field `std::sync::OnceLock` on the struct if the content join is needed
first; say which and why in the report). Delete the statics, `DocKey`,
`IndexKey`, `path_digest`, `doc_cache`, `def_map`. `site_occurrence` and
`definition_of` keep their signatures and read the fields. Fail-first test:
two indexes loaded in one process with the same document count and path
must not share a span table (the fingerprint hazard the comment describes).

## Item 2: `project.rs` `load_scip`, the fresh-index default
The `ScipMode::Off` arm calls `ScipTypescript.load(&path)` for every
language. Call `crate::scip_decode::load_index(&path)` directly. Only that
arm changes in `project.rs`.

## Item 3: cruft
Delete `fn spanned<T: Spanned>(t: &T) -> &T { t }` in
`src/lang/rust_modules.rs` (call `.span()` on the value directly) and the
unused `scratch` vector in `scip_decode.rs::for_each_message`. `cargo build`
must print zero new warnings for the crate.

## Receipt
Per corpus, 3 runs, `nice -n 15`, `/usr/bin/time -l` (REAL is the first
field, not the third): wall and peak RSS before -> after, identity
IDENTICAL. Go RSS is expected to drop (the caches were the second copy of
every span table); report it against the 700 MB ceiling either way.
`just extract-ratchet` plain (no BUMP) must hold every row. Gate
(`nice -n 15 cargo test --release --features cli`) in background with a
log; wall-ratio flakes rerun 3x isolated.

## Ownership
`src/scip.rs`, `src/scip_decode.rs`, `src/types.rs` (scip types only),
`src/project.rs` (the `ScipMode::Off` arm only), `src/lang/rust_modules.rs`
(the `spanned` lines only), `tests/scip_freshness.rs`,
`tests/8_scip_families_cli.rs`, `SCIP-SPEED.REPORT.md` section 10. NOT any
resolve logic, NOT `tests/bench/mod.rs`, NOT RATCHET.tsv. No `cargo fmt` on
files you do not own. One extract run at a time, `timeout 60`. No file over
1 MB. Budget 60 min; past it, post with Item 1.

Push `fix/extract-scip-caches`, `gh pr create --base main`, hail
`boop beep --no-wait --as fix-extract-scip-caches sprefa-coordinator "scip caches: PR #N, statics gone, go RSS a -> b MB, identity ok, gate x/y"`.
Laws: no em dashes anywhere, no eprintln (tracing only), descriptive names,
comments only for what code cannot show, no words
provenance/substrate/load-bearing/regime/refusal, never "ground truth".
