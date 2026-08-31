# Lane `fix-extract-ts-closure-init` (opus): ts closure-caller mirror, then unannotated-const receivers

Read `plans/extract-crawl-2026-08-29/ts.GAPS.md` (rows at lines 41, 44,
63, 76, 93, 96) and `ts5.REPORT.md`. After #566 ts recall vs the tsc
oracle is 70.05% (41,547/59,311), precision 70.00%. Two classes carry most
of the gap and go's twins already landed: #563 (closure-caller mirror,
`src/lang/go.rs` closure mirror block) and #562 (one hop of return-type
inference, `go.rs` `go_binding_of_rhs`).

## First action
```
git merge --ff-only 28681eb9a8b96f42dd5c8d022b3d036c1de5f8b4
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```
Corpus `/Users/chrishafley/projects/TypeScript-5.9`, `src/**/*.ts`.
ONE process per run, `timeout 30`, background with a log; `ts.rss.tsv`
records the RSS history, so watch RSS with `/usr/bin/time -l`.

## Task A: closure-caller mirror (5,165 oracle-only + 14,389 ours-only)
A call site whose enclosing fn is `closure@N` also emits the edge with
the nearest named enclosing fn as caller (mirror, both rows stay, same as
go #563; grep go.rs for the mirror block and copy its shape). Fail-first
test in `tests/` (pick the ts resolve test file; new one if none fits):
fixture with `function outer() { arr.forEach(x => helper(x)) }` asserting
a `resolved_edge` outer -> helper. Paste the HEAD failure in the header.

## Task B: unannotated const from initializer (3,049 drops + 1,722 oracle-only)
`const printer = createPrinter(); printer.writeNode(...)`: bind the
receiver's type through the initializer callee's declared return type, one
hop, callee resolved through the module plane (`ts_resolve.rs`
TsModuleIndex) when cross-file. Entry points: `receiver_of`
(`ts_receivers.rs:98`), `type_anchor` (`ts_receivers.rs:421`). Fail-first
test with a two-file fixture. `UnresolvedReason` variants exist already;
add none.

## Receipt
`plans/extract-bench-2026-08-29/bench.py <yours normalised> ts5.oracle.call.tsv`
before and after each task (normalise with `normalize.py`; write your tsvs
under `plans/extract-crawl-2026-08-29/`, the bench dir belongs to another
lane). PR body: recall 70.05% -> after A -> after B, precision, drops
7,638 -> n, RSS peak, wall, gate counts.

## Ownership
`v6/sprefa-extract/src/lang/ts.rs`, `ts_receivers.rs`, `ts_resolve.rs`,
the ts test file and its fixtures, `plans/extract-crawl-2026-08-29/ts*`.
NOT `src/types.rs`, `src/project.rs` (if the closure caller key at
`project.rs:1004-1013` must change, hail the coordinator with the diff
instead), NOT `go*.rs`, `rust*.rs`, NOT `plans/extract-bench-2026-08-29/`.
No `cargo fmt` on files you do not own. Gate `cargo test --features cli
--no-fail-fast` in background with a log; wall-ratio flakes rerun 3x
isolated, say so. Never commit a file over 1 MB.

Push `fix/extract-ts-closure-init`, `gh pr create --base main`, hail
`boop beep --no-wait --as fix-extract-ts-closure-init sprefa-coordinator "ts closure+init: PR #N, recall 70.05->x->y, drops 7638->n, gate a/b"`.
Laws: no em dashes, no eprintln, descriptive names, comments only for what
code cannot show, no words provenance/substrate/load-bearing/regime/refusal,
never "ground truth" (say oracle).
