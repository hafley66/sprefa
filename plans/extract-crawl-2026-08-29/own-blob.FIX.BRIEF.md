# Brief: `own_blob` is nondeterministic at corpus scale (lane `fix-extract-own-blob`)

Read `plans/extract-corpus-2026-08-28/COMMON.md` (style laws) and
`plans/extract-crawl-2026-08-29/go.REPORT.md` "Kink: `own_blob` cross-corpus
span search is non-deterministic at scale".

## First action
```
git merge --ff-only 7cafeae8061535bcf6b448d44f96aebe0ce2bc0b
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```
Failure: STOP, `boop beep --no-wait --as fix-extract-own-blob sprefa-coordinator "<one line>"`.
Binary: `v6/sprefa-extract/target/release/extract` in YOUR worktree.

## Defect
`own_blob` (`src/types.rs:1659`) finds the blob of the output being resolved
by scanning `index.map.values().flatten()` (a `HashMap`, per-process random
order) for the FIRST `DefSite` whose span equals one of this output's named
spans. Two files with one equal `(start,end)` pair make the answer depend on
hash order. Measured on typescript-go, 5,097 files: `resolved_edge` 86,070
in 4 of 5 runs, 86,061 in 1 of 5, the same 9 receiver-typed sites in
`internal/execute/tsctests/runner.go` every time.

## Fix
1. `resolve_project` (`src/project.rs:952`) already holds
   `inputs: &[(ContentId, &ExtractOutput)]`. Put the blob on the seam: add
   `own: Option<ContentId>` to `ProjectCx`, set per file inside
   `resolve_project` before each `Resolve::resolve` call. `own_blob` gains a
   `cx` parameter and returns `cx.own.clone()` when set. Every caller of
   `own_blob` (grep it: `go.rs`, `rust.rs`, `ts.rs`, others) passes `cx`.
2. The fallback (a hand-built `ProjectCx` with `own: None`, as in
   `tests/golden_parity.rs`) becomes deterministic: count span matches per
   blob over ALL named spans, pick the blob with the highest count, and
   return `None` on a tie. Iterate blobs in sorted `ContentId` order so the
   result is stable under any tie-break you add later.
3. No behavior change for any arm when `own` is set except the removal of
   the wrong-blob case.

## Ownership
Yours: `src/types.rs` (the `own_blob` fn and `ProjectCx` struct only),
`src/project.rs` (`resolve_project` only), the one-line `own_blob(...)` call
sites in `src/lang/*.rs`, `tests/61_own_blob.rs`, `tests/golden_parity.rs`
(only if its hand-built `ProjectCx` needs the new field). Forbidden: every
other function in `src/lang/*.rs`.

## Tests, fail-first
`tests/61_own_blob.rs`: two fixture files whose only named def sits at the
same byte span; with `own` set each resolves into its own blob; with `own`
unset and one span shared, the max-count rule picks the right blob when a
second named span breaks the tie, and returns `None` on an exact tie.
COUNT: the fallback is one pass over the index, never one pass per named span.

## Receipt
Five back-to-back `extract --resolve` runs over
`/Users/chrishafley/projects/typescript-go` (all `.go` under `internal/` and
`cmd/`, one process each, `timeout 10` per run): five identical
`resolved_edge` counts. Put the five numbers in the PR body. Gate
`cargo test --features cli --no-fail-fast` in background, SUM. Push,
`gh pr create --base main`, hail
`boop beep --no-wait --as fix-extract-own-blob sprefa-coordinator "own_blob: PR #N, 5 runs <n>x5, gate <p>/<f>"`.

## Laws (inline)
No em dashes. No `eprintln!`. Comments state constraints only, no dates.
Descriptive names. Never `--no-verify`. No `cargo fmt` outside files you own.
