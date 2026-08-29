# Brief: go interface dispatch fan-out at the call site (lane `fix-extract-go-iface-fanout`)

Read `plans/extract-corpus-2026-08-28/COMMON.md` and
`plans/extract-crawl-2026-08-29/go.GAPS.md` (PR #564): the top vta-only
class is interface method dispatch, ~4,589 projected edges (4,003 multi-
implementer + 586 single). Today a site `x.M()` with `x: I` binds
`I.M` (the method_spec node, #554) and separate `Implements` edges
`I.M -> T.M` exist (`go_interface_implements`, `go.rs:3218`). The crawl
and the vta oracle both want the site to reach `T.M` directly.

## First action
```
git merge --ff-only 25c63fb00bd49f837394b86e3ca9bc982aebe12d
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```
Binary: `v6/sprefa-extract/target/release/extract` in YOUR worktree.
Failure: STOP, `boop beep --no-wait --as fix-extract-go-iface-fanout sprefa-coordinator "<one line>"`.

## Build
For every call edge whose callee is an interface method_spec `I.M`, emit
one additional `resolved_edge` per implementer `T` in the same module
(`caller -> T.M`, same call_site span, edge kind `implements`, the variant
#554 added). The implementer map is keyed by (interface, method name),
built once per corpus (`go_interface_implements` already computes the
sets; reuse, do not recompute per site). Keep the `I.M` edge. Cap: an
interface with more than 64 implementers (e.g. `ast.Node` with 170) emits
the `I.M` edge only plus one `unresolved` row reason `fanout_cap` with
the count; say the cap in the schema line.

## Ownership
`src/lang/go.rs`, `tests/66_go_iface_fanout.rs`,
`tests/fixtures/go_findings/iface_fanout/**`, go.REPORT.md (append "Fixes 7"),
`plans/extract-bench-2026-08-29/go.parse.call.tsv` (overwrite on the
current binary). Forbidden: everything else under `src/`.

## Tests, fail-first
two implementers -> two fan-out edges plus the spec edge; an implementer
missing one method -> excluded; 65 generated implementers -> `fanout_cap`
row and no fan-out; COUNT: the implementer map is one pass per corpus.

## Receipt, ONE process
`extract --resolve --project-root /Users/chrishafley/projects/typescript-go $(cat gofiles.txt)`;
normalize bare names; `bench.py go.parse.call.tsv go.oracle.call.vta.bare.tsv`:
recall 73.4% -> n, precision 50.9% -> n; `go.crawl.py` reachable 8,983 -> n.
Gate in background, SUM. Push, PR, hail
`boop beep --no-wait --as fix-extract-go-iface-fanout sprefa-coordinator "go iface fanout: PR #N, vta recall 73.4%-><r>, reachable 8,983-><n>, gate <p>/<f>"`.
Laws: no em dashes, no eprintln, comments state constraints only, no
`cargo fmt` outside files you own, never --no-verify.
