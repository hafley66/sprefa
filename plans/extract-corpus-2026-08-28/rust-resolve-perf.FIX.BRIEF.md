# Brief: rust --resolve superlinear after own_file_blob (lane `fix-extract-rust-resolve-perf`)

Read `plans/extract-corpus-2026-08-28/COMMON.md` (style laws, 10-second law).

## First action
```
git merge --ff-only cec3d5c1d
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```
Failure: STOP, `boop beep --no-wait --as fix-extract-rust-resolve-perf sprefa-coordinator "<one line>"`.

## Defect (measured by the coordinator)
`src/lang/rust.rs:947` `own_file_blob` (landed in PR #530) is called from
`call_name_match` (`rust.rs:901`, invoked per call site at `rust.rs:1035`).
Per site it rebuilds `own_spans` from `call.nodes` and, for the candidate
blob, scans the whole `DefIndex` (`index.map.values().flatten()`) once per
span: O(sites x spans x index_entries).
Receipt, tokio 1.x crate files from `~/.cargo/registry/src/*/tokio-1.*`
(`find <dir> -name '*.rs' | head -N`), `extract --resolve`:
  n=200: pre-#530 binary 76 ms, current 196 ms
  n=400: pre-#530 binary 151 ms, current 1032 ms   (ratio 400/200 = 5.3)

## Fix, fail-first
1. Test FIRST: `tests/49_rust_resolve_scaling.rs`, shape copied from
   `tests/46_resolve_scaling.rs` (go), over a synthetic rust corpus of 400
   files each defining a shared `helper` name plus unique fns and calls;
   assert wall(400)/wall(200) < 2.5 and that the same-file-wins edge from
   `tests/60_rust_corpus_scope.rs` still holds. Run it red, paste output in
   the commit body.
2. Fix: compute the file's own blob ONCE per file, not per site. The file's
   identity is its `ContentId` (`content_id_of(content)`), and `ProjectCx` /
   the `Resolve<CallF>` entry already knows which input it resolves; thread
   that blob into `call_name_match` (or precompute a `blob -> BTreeSet<Span>`
   map once from the index) so the same-file leg is one hash probe. Read
   `src/project.rs` `ProjectInput.blob` and the rust `Resolve<CallF>` impl
   before choosing; state the choice in the commit body.
3. Green, then the corpus receipt: the tokio numbers above re-measured with
   your binary, in the commit body and appended to
   `plans/extract-corpus-2026-08-28/rust.REPORT.md` Fixes table.
4. `cargo test --features cli --no-fail-fast`, full passed/failed in the body.

## Files you own
`v6/sprefa-extract/src/lang/rust.rs`, `v6/sprefa-extract/src/project.rs`
(only if threading the blob needs it), `tests/49_rust_resolve_scaling.rs`,
`tests/fixtures/rust_scopes/**`, the rust.REPORT.md Fixes table.
Forbidden: every other file. No whole-crate `cargo fmt`. No subagents.
Then: push, `gh pr create --base main`, hail
`boop beep --no-wait --as fix-extract-rust-resolve-perf sprefa-coordinator "PR #N, tokio n=400 <before>-><after> ms, gate <p>/<f>"`.
