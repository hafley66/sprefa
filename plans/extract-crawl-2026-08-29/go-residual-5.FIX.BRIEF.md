# Lane `fix-extract-go-residual-5` (opus): the five go classes left in the codeql-agreed set

Read `plans/extract-crawl-2026-08-29/go.GAPS.md` (the "codeql agreed and
missed" section from #577, rows at lines 97-101) and
`go_gap_classify/main.go` (the go/types classifier; rerun it for the
receipt). After #577: recall 79.57% of vta bare, agreed-and-missed 3,041.

| class | rows | leg |
|---|---:|---|
| alias receiver `type Expression = Node` | 926 | `go_method_in_dir` (go.rs:3236); needs a `type X = Y` table in `GoFileFacts` |
| multi-hop receiver chain | 816 | `go_chain_receiver_target` (go.rs:3395) |
| one-hop receiver never typed (range var, field read, index read, multi-value define) | 811 | `go_seed_top_scope` (go.rs:1270), `go_walk_receivers` (go.rs:1365) |
| bare in-package call, corpus-wide name not unique | 105 | `GoSource::call_name_match` (go.rs:2733): caller's own dir first |
| import-qualified call shadowed by a same-named method in the target dir | 81 | `GoModuleIndex::resolve_in_dir` (go_modules.rs:291): functions only |

## First action
```
git merge --ff-only 5ea4c683910aa354616b93e91f994332a98f5912
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```
Corpus `/Users/chrishafley/projects/typescript-go`, `internal/**/*.go cmd/**/*.go`,
ONE process, `timeout 30`, background, log. The wall is 9.1 s at #577;
any leg you add runs inside the existing passes, and a wall over 10 s is a
defect you report with a 5 s `sample` profile, never a timeout raise.

## Rules, fail-first each (tests in `tests/69_go_promoted.rs` or a new
`tests/7N_go_*.rs`, fixtures under `tests/fixtures/go_*`, HEAD failure in
each header)
- alias: `type A = B` in `GoFileFacts` (mirror `embeds` from #577);
  method lookup on `A` falls through to `B`, transitively, cap 4.
- multi-hop: the bind plan records the declared result type of each
  intermediate call (`fn_sigs`, go.rs:392) so `a.b().c()` types `c`'s
  receiver from `b`'s return type; depth 8 like ts.
- never typed: `for _, x := range xs` types `x` from the element type of
  `xs` (slice, map value, channel); `x := s.field` from `go_field_types`
  (go.rs:1066); `x := arr[i]` from the element type; `a, b := f()` from
  the i-th result type.
- bare not-unique: try the caller's own package dir before corpus-wide.
- import shadowed: `resolve_in_dir` for a call candidate excludes method
  defs (`owner_of` set).

## Receipt
Single-process rerun; `bench.py` against `go.oracle.call.vta.bare.tsv` and
`go.codeql2.call.tsv`; rerun `go_gap_classify`. PR body: recall 79.57% ->
n, precision, agreed-and-missed 3,041 -> n with the per-class table, median
wall of 3 runs, gate.

## Ownership
`v6/sprefa-extract/src/lang/go.rs`, `go_modules.rs`, the go test files and
fixtures, `plans/extract-crawl-2026-08-29/go*`. NOT `src/types.rs`,
`rust*.rs`, `ts*.rs`, NOT `plans/extract-bench-2026-08-29/`. No `cargo fmt`
on files you do not own. Gate in background with a log; wall-ratio flakes
rerun 3x isolated, say so. No file over 1 MB.

Push `fix/extract-go-residual-5`, `gh pr create --base main`, hail
`boop beep --no-wait --as fix-extract-go-residual-5 sprefa-coordinator "go residual 5: PR #N, recall 79.57->x, agreed-missed 3041->n, wall s, gate a/b"`.
Laws: no em dashes anywhere including test headers, no eprintln, descriptive
names, comments only for what code cannot show, no words
provenance/substrate/load-bearing/regime/refusal, never "ground truth".
