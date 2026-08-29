# Go call-edge gap classes (lane `chore-go-gap-classes`, measure only)

Binary at the #563 receipt commit. Sets derived from
`bench.py plans/extract-bench-2026-08-29/go.parse.call.tsv go.oracle.call.vta.bare.tsv`
(precise numbers: |a| 80,584, |b| 55,099, both 40,454, recall 73.4% (bare
names), precision 50.2% by oracle-direction).

| set | rows | sample |
|---|---:|---:|
| vta-only (`go.gaps.vta_only.tsv`) | 14,645 | 300 (seed 42) |
| ours-only (`go.gaps.ours_only.tsv`) | 40,130 | 200 (seed 42) |

Per-row classes beside this file: `go.gaps.vta_only.classes.tsv`,
`go.gaps.ours_only.classes.tsv`; classifier `go.gaps.classify.py`.

## Contents

- [vta-only classes](#vta-only-classes)
- [ours-only classes](#ours-only-classes)
- [Which leg takes each class](#legs)

## vta-only classes

Project = sample count x (14,645 / 300), rounded.

| class | sample | projected | example (src \| dst) |
|---|---:|---:|---|
| closure-caller naming: vta attributes the edge to `<fn>$N`, we emit caller `closure@N` plus one Fix-6 mirror to `<fn>`; the `$N` rows can never intersect | 83 | 4,052 | `checker.go getAdjustedTypeWithFacts$1 -> getGlobalNonNullableTypeInstantiation` |
| interface method dispatch: receiver is an interface (vfs.FS, ast.Node, Visitor with 170 implementers); vta names the concrete implementer | 82 | 4,003 | `overlayfs.go processChanges -> callbackfs.go UseCaseSensitiveFileNames` |
| interface method dispatch, single implementer: same shape, one concrete impl (ast.Node As* accessors on `*Node`) | 12 | 586 | `printer.go emitExpression -> ast_generated.go AsObjectLiteralExpression` |
| multi-hop receiver chain `a.b().c()`: intermediate hop is a call whose result type our bind plan does not carry | 53 | 2,587 | `buildtask.go cleanProjectOutput -> iovfs/iofs.go FileExists` (`orchestrator.host.FS().FileExists`) |
| vta-closure-callee: vta names a func literal as callee (`fn$1`); we drop literal callees by design | 45 | 2,197 | `ast.go visit -> parseoptions.go findChildNode$1` |
| func-typed field/param: callee stored/passed as a func value (checkerpool `noop`, `reportDiagnostic` field, option maps) | 8 | 391 | `api/session.go handleGetTypesAtPositions -> checkerpool.go noop` |
| other: concrete struct/pkg call our legs should resolve but dropped (spot-checked; no single shape) | 17 | 830 | `fileInclude.go computeDiagnostic -> parsedcommandline.go ConfigName` (same receiver `config` resolves `FileNames` two lines up) |

## ours-only classes

Project = sample count x (40,130 / 200). These are edges the oracle lacks;
mostly scope, not defect.

| class | sample | projected | example (src \| dst) |
|---|---:|---:|---|
| test-only caller: caller in `_test.go` / fourslash tests / `_tools`; vta SSA reach from program roots excludes them | 126 | 25,282 | `fourslash/tests/gen/documentHighlights02_test.go TestDocumentHighlights02 -> fourslash.go VerifyBaselineDocumentHighlightsWithOptions` |
| closure-caller naming, mirror side: we emit caller `closure@N` + mirror; vta has only the `<fn>`-caller row, our mirrored row intersects but the `closure@N` row stays ours-only | 42 | 8,427 | `findallreferences.go closure@105125 -> checker/types.go Symbol` |
| non-test caller vta did not reach: receiver is an interface whose vta-resolved target differs (mock/test impls e.g. `fswatch fallback_test.go WatchFile`) or vta reach missed the path | 32 | 6,421 | `fswatch/watcher.go WatchFile -> fallback_test.go WatchFile` |

## Which leg takes each class

| class | owning leg (go.rs) | note |
|---|---|---|
| interface dispatch (both flavors) | `go_mint_interface_methods` (go.rs:868) + `go_interface_implements` (go.rs:3218) | biggest actionable class (~4,589). Minting method edges for known implementers would need the implementer map keyed by interface method name |
| multi-hop chain | `go_bind_plan_of` / `go_binding_of_rhs` (go.rs:983/1195) + `go_receiver_binding` (go.rs:1217) | extend the Fix-5 bind plan to record the result type of the intermediate hop (`host.FS()` -> `vfs.FS`) |
| func-typed field/param | `go_field_types` (go.rs:1066) + `go_receiver_binding` | fields already have declared types; seed func-typed fields like any receiver |
| closure-caller naming (both directions) | closure mirror block (go.rs:3120) | representational, no recall gain from a new leg; would need the mirror edge (or the primary) to carry a caller name that intersects vta's `<fn>$N` form |
| vta-closure-callee | `go_walk_call_sites` (go.rs:912) | we drop func-literal callees by design; adopting vta's `fn$N` naming for literal callees is the only route |
| other (vta-only) | `go_receiver_binding` | 830 rows, per-row root cause; `ConfigName` example is a same-receiver sibling of two resolved calls |
| ours-only test-only caller | none | oracle scope: vta reach excludes test roots |
| ours-only non-test | none | mostly oracle-side: mock implementers vta did not seed |
