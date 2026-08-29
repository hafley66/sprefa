# Lane `fix-extract-ts-ns-iface` (opus): ts namespace-merged receivers, interface fan-out, destructured receivers

Read `plans/extract-crawl-2026-08-29/ts.GAPS.md` (the oracle-only table,
rows for "namespace-merged", "interface receiver needing implementer
fan-out", "interface receiver via destructuring") and
`ts-closure-init.RECEIPT.md` (#575). After #575 ts call vs the tsc oracle:
66,714 ours, 46,958 overlap, oracle-only 12,398, drops 6,865, precision
79.11%. Three classes remain at 2,553 / 1,722 / 1,543 projected.

## First action
```
git merge --ff-only 78c41673dc2b762f763de6a577c8ef0b55367e52
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```
Corpus `/Users/chrishafley/projects/TypeScript-5.9`, `find src -name '*.ts' ! -name '*.d.ts'`,
ONE process, `--resolve --project-root`, `timeout 30`, background, log
(3.31 s and 383 MB measured at #575).

## Task A: namespace-merged receivers (2,553)
`ts.factory.createX(...)`, `Debug.assert(...)`, `performance.mark(...)`
where `ts` / `factory` / `Debug` come through `src/compiler/_namespaces/*.ts`
barrels (`export * from`, `export * as X from`). The module plane
(`ts_resolve.rs`, TsModuleIndex, ECMA ResolveExport) already walks
`export *`; the miss is the member step: a member access on a namespace
binding must resolve the member through the namespace's export table
(`ResolveExport(module, member)`) before the receiver-type leg runs. Cite
the ts.rs arm where a MemberExpression callee is split into receiver and
member, and add the namespace branch ahead of the typed-receiver branch.
Fail-first fixture: `_namespaces/ns.ts` with `export * from "./impl"`,
`impl.ts` exporting `createX`, caller `import * as ns from "./_namespaces/ns"; ns.createX()`.

## Task B: interface receiver fan-out (1,722), the go #565 twin
Receiver typed as an interface (`TypeCheckerHost`, `Program`,
`SourceFile`): emit one edge per implementer whose class or object literal
implements the interface, cap 64 (`UnresolvedReason::FanoutCap` exists;
add no variants), kind `Implements` edges already emitted by the ts arm
(grep `Implements` in ts.rs). Where the "implementer" is an object literal
returned by a factory (`createProgram(): Program` returning `{ getSourceFile, ... }`),
bind the member to the property fn inside that literal; that is the corpus
shape and a class-only fan-out will read near zero. Fail-first fixture with
both shapes.

## Task C: destructured receivers (1,543)
`const { factory } = context;` then `factory.createStringLiteral(...)`:
`receiver_of` (`ts_receivers.rs`) reads only identifier declarators; add
ObjectPattern declarators, binding the property's type from the
initializer's declared type (one hop, via `type_anchor`). Fail-first
fixture.

## Receipt
After each task, single-process rerun and
`plans/extract-bench-2026-08-29/bench.py <yours normalised> ts5.oracle.call.tsv`
(tsvs under `plans/extract-crawl-2026-08-29/`). PR body: the four-row
stage table from #575's receipt extended by three rows, wall, RSS, gate.

## Ownership
`v6/sprefa-extract/src/lang/ts.rs`, `ts_receivers.rs`, `ts_resolve.rs`,
new `tests/7N_ts_*.rs` and fixtures under `tests/fixtures/ts5_findings/`,
`plans/extract-crawl-2026-08-29/ts*`. NOT `src/types.rs`, `src/project.rs`,
`go*.rs`, `rust*.rs`, NOT `plans/extract-bench-2026-08-29/`. No
`cargo fmt` on files you do not own. Gate `cargo test --features cli
--no-fail-fast` in background with a log; wall-ratio flakes rerun 3x
isolated, say so. No file over 1 MB.

Push `fix/extract-ts-ns-iface`, `gh pr create --base main`, hail
`boop beep --no-wait --as fix-extract-ts-ns-iface sprefa-coordinator "ts ns/iface/destructure: PR #N, overlap 46958->n, oracle-only 12398->n, gate a/b"`.
Laws: no em dashes, no eprintln, descriptive names, comments only for what
code cannot show, no words provenance/substrate/load-bearing/regime/refusal,
never "ground truth" (say oracle).
