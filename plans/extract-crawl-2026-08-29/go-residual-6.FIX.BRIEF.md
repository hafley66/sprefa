# Lane `fix-extract-go-residual-6` (glm53f): the 1,083 go rows codeql and vta agree on that we miss

Read `plans/extract-crawl-2026-08-29/go.GAPS.md` (the #579 section and its
per-class table) and `go_gap_classify/main.go`. After #579: recall 84.42%
of vta bare, agreed-and-missed 1,083: one-hop receiver never typed 487,
embedded-struct promoted method past the depth-4 cap 289, multi-hop 195,
alias 89, and the rest. After #583 the go wall is 6,680 ms; every leg you
add must keep it under 10 s (three-run median in the PR body).

## First action
```
git merge --ff-only <sha>
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```
Corpus `/Users/chrishafley/projects/typescript-go`, `internal/**/*.go cmd/**/*.go`,
ONE process, `timeout 30`, background, log.

## Rules, fail-first each (tests in `tests/71_go_residual.rs` or a new
`tests/7N_go_*.rs`, HEAD failure in each header, commit after each green)
- never typed 487: rerun `go_gap_classify` on the 487 and split by shape
  (range var, field read, index read, multi-value define, other) before
  coding; take the top two shapes.
- promoted past depth 4: raise the cap to 9 (the `ast.Node` hierarchy
  depth #577 measured) and show the wall stays under 10 s; if it does not,
  memoise the embed walk per (type, method) pair.
- multi-hop 195 and alias 89: classify 50 each and fix the top shape.

## Receipt
Single-process rerun; `bench.py` vs `go.oracle.call.vta.bare.tsv` and
`go.codeql2.call.tsv`; rerun `go_gap_classify`. PR body: recall 84.42% -> n,
precision, agreed-and-missed 1,083 -> n per class, median wall of 3 runs,
gate. `just extract-ratchet` green, `RATCHET_BUMP=1` when go rows improve.

## Ownership
`v6/sprefa-extract/src/lang/go.rs`, `go_modules.rs`, go test files and
fixtures, `plans/extract-crawl-2026-08-29/go*`, RATCHET.tsv go rows. NOT
`src/types.rs`, `src/project.rs`, `rust*.rs`, `ts*.rs`, `scip*.rs`. No
`cargo fmt` on files you do not own. Gate in background with a log;
wall-ratio flakes rerun 3x isolated. No file over 1 MB. Budget 60 min.

Push `fix/extract-go-residual-6`, `gh pr create --base main`, hail
`boop beep --no-wait --as fix-extract-go-residual-6 sprefa-coordinator "go residual 6: PR #N, recall 84.42->x, agreed-missed 1083->n, wall s, gate a/b"`.
Laws: no em dashes anywhere, no eprintln, descriptive names, comments only
for what code cannot show, no words provenance/substrate/load-bearing/regime/refusal,
never "ground truth".
