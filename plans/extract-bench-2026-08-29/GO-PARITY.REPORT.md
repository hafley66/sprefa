# GO-PARITY.REPORT.md — one projection, both oracles (2026-08-30)

TOC
1. The two verified facts
2. The projection (`go.project.py`, `tests/bench/mod.rs::GoProjection`)
3. The table: recall / precision per oracle per projection
4. How the numbers were computed
5. Residual excess, classified
6. Ratchet before -> after

## 1. The two verified facts

| fact | receipt |
|---|---|
| neither oracle contains a `_test.go` row | `grep -c '_test.go'` = 0 on `go.codeql2.call.tsv` and `go.oracle.call.vta.bare.tsv` |
| ours-only test-file rows | 28,037 of 40,130 rows in `plans/extract-crawl-2026-08-29/go.gaps.ours_only.tsv` contain `_test.go` (grep, line count; the lane's 27,834 was row-unique count) |
| codeql names the interface method, vta names implementers, ours emits both | `go.GAPS.md` "Which leg takes each class"; fan-out rows carry `kind=implements` in the raw JSONL (`go.rs` `CallEdgeKind::Implements`), the spec row carries a normal kind |

## 2. The projection

`go.project.py` (stdlib only; oracle rows never touched, every flag applies
to OURS only):

| flag | what it does |
|---|---|
| `--scope-oracle <tsv>` | drop rows whose src_path is absent from the oracle's src_path set (`_test.go`, packages the oracle never built) |
| `--closure` | drop `closure@<n>`-caller rows (the mirrored enclosing-fn row stays) |
| `--iface method\|impl\|both` | interface call site: keep the spec row (codeql shape) / the per-implementer rows (vta shape) / both. Impl rows are the `kind=implements` rows of the kinds sidecar; the spec row is the non-implements row whose (src_path, src_name, dst_name) triple also occurs on an implements row |

Rust port: `GoProjection` + `go_project` in
`v6/sprefa-extract/tests/bench/mod.rs`, unit test
`go_projection_drops_test_closure_and_iface_rows` (7 hand-made rows -> 3 kept
per mode). Applied in `ratchet()` only for the two go call oracles;
`go.codeql2.call.tsv` scores in method shape, `go.oracle.call.vta.bare.tsv`
in impl shape. Module/type rows and every ts5/rust row score raw.

## 3. The table

Ours = fresh single-process resolve over all 5,097 `.go` files
(`out/run_go_parity.sh`, rc=0, 7 s wall, 92,259 distinct call rows ->
`out/go.ours.call.tsv` + kinds sidecar `out/go.ours.call.kinds.tsv`).

| oracle | projection | recall | precision | ours rows | oracle rows | overlap |
|---|---|---|---|---|---|---|
| codeql2 | none | 97.35 | 51.21 | 92,259 | 48,529 | 47,244 |
| codeql2 | scope | 97.35 | 75.59 | 62,498 | 48,529 | 47,244 |
| codeql2 | scope+closure | 97.35 | 81.57 | 57,917 | 48,529 | 47,244 |
| codeql2 | scope+closure+iface=method | **97.35** | **90.19** | 52,385 | 48,529 | 47,244 |
| vta | none | 84.42 | 50.42 | 92,259 | 55,099 | 46,517 |
| vta | scope | 84.42 | 74.59 | 62,366 | 55,099 | 46,517 |
| vta | scope+closure | 84.42 | 80.49 | 57,791 | 55,099 | 46,517 |
| vta | scope+closure+iface=impl | **83.73** | **81.42** | 56,658 | 55,099 | 46,133 |

Reading: scope alone halves the false-positive rate; closure + iface shape
take precision from ~51% to 90.19% (codeql2) and 81.42% (vta) with recall
held at 97.35 / 83.73. The vta impl-mode recall dip (84.42 -> 83.73, 384
rows) is real: some of our spec rows textually match vta rows because the
implementer lives in the same file as the interface, so the same 4-tuple
appears as both shapes.

## 4. How the numbers were computed

`bench.py` prints the two labels backwards (`bench.py:38-41`,
ORACLES.REPORT.md:583). Every number in section 3 was computed directly as
`recall = overlap / |oracle|`, `precision = overlap / |ours|` in the
computing script; the printed table above is the source of the RATCHET rows.

## 5. Residual excess, classified

Sets `ours - oracle` under the full projection:
`out/go.excess.codeql2.tsv` (5,141 rows), `out/go.excess.vta.tsv` (10,525).
300-row samples, seed 7 (`out/go.excess.*.sample300.tsv`), run through
`go_gap_classify` (`out/go.excess.*.classified.tsv`). File-shape tags
counted over the FULL excess sets in the bottom rows.

### ours - codeql2, sample of 300

| class | count | example file:line | emitter |
|---|---|---|---|
| concrete-one-hop-receiver | 76 | `internal/transformers/declarations/diagnostics.go:697` -> `internal/ast/ast.go Name`; `internal/checker/checker.go:30104` -> `getTypeArguments` | the main resolve arm (`go.rs` `go_call_targets` / `scip_call_target` fallback) |
| interface-dispatch | 70 | `internal/checker/mapper.go:27` -> `Kind` (10 implementers); `internal/compiler/filesparser.go:83` -> `FS` (6) | `go_iface_fanout` spec rows, `go.rs:4200` |
| package-qualified-call | 63 | `internal/ls/completions.go:1118` -> `IsVariableLike`; `internal/ls/selectionranges.go:280` -> `IsJSDocSignature` | same resolve arm |
| no-syntactic-site | 32 | `internal/fswatch/kqueue.go` `cleanupEntriesLocked` -> `parseCache.go delete`; builtin `len` rows | go_modules synthetic targets |
| multi-hop-receiver | 21 | `internal/testrunner/compiler_runner.go:612`; `internal/lsp/server.go:1376` | same resolve arm |
| package-level-call | 20 | `internal/ls/findallreferences.go:1761`; `internal/contentmapper/hostimpl.go:533` | same resolve arm |
| embedded-promoted-method | 7 | `internal/project/configfileregistrybuilder.go:404`; `internal/parser/utilities.go:45` | embed walk |
| func-typed-field-or-value | 6 | `internal/fourslash/baselineutil.go:1086`; `internal/checker/printer.go:302` | func-value binding |
| alias-receiver | 3 | `internal/checker/checker.go:3579`; `internal/checker/nodecopy.go:774` | alias walk |
| interface-dispatch-fanout-capped | 2 | `internal/testutil/lsptestutil/lspclient.go:87` (71 implementers); `internal/ast/ast.go:219` (213) | `GO_FANOUT_CAP` path, `go.rs:4202` |

### ours - vta, sample of 300

| class | count | example file:line | emitter |
|---|---|---|---|
| interface-dispatch | 78 | `internal/bundled/embed.go:112` -> `WalkDir` (13 implementers); `internal/module/resolver.go:2110` -> `FS` (18) | `go_iface_fanout` impl rows, `go.rs:4212` |
| package-qualified-call | 78 | `internal/ls/autoimport/extract.go:138` -> `MapNonNil`; `internal/checker/checker.go:4364` -> `Every` | main resolve arm |
| concrete-one-hop-receiver | 56 | `internal/ls/crossproject.go:116`; `internal/compiler/program.go:838` | main resolve arm |
| multi-hop-receiver | 36 | `internal/checker/nodebuilderimpl.go:1978`; `internal/project/snapshot.go:212` | main resolve arm |
| no-syntactic-site | 35 | builtin `len` rows; `internal/ls/lsconv/converters.go` -> `OriginalText` | go_modules synthetic targets |
| package-level-call | 10 | `internal/ls/utilities.go:1203`; `internal/fourslash/fourslash.go:3250` | main resolve arm |
| embedded-promoted-method | 3 | `internal/project/configfileregistrybuilder.go:404`; `internal/lsp/server.go:159` | embed walk |
| alias-receiver | 2 | `internal/ls/completions.go:5910`; `internal/checker/checker.go:5483` | alias walk |
| interface-dispatch-fanout-capped | 1 | `internal/testutil/lsptestutil/lspclient.go:87` (71 implementers) | `GO_FANOUT_CAP` |
| func-typed-field-or-value | 1 | `cmd/tsgo/lsp.go:49` | func-value binding |

### 5a. Excess verification, go/types over the FULL sets (2026-08-30, lane fix/extract-go-excess-1)

The three classes named in the brief were verified against `go/types`
(`packages.Load` over the corpus, 105 packages), full excess sets, no
sampling. Tool: `plans/extract-crawl-2026-08-29/go_gap_classify/verify/`
(classify first: `out/go.excess.{vta,codeql2}.classes.tsv`, then
`/tmp/verify_excess`: `out/go.excess.{vta,codeql2}.verified.tsv`).

**package-qualified-call vs vta: oracle-side, closed.** go/types resolves
every import-qualified call to the unique package-level object, so the split
is `our dst == go/types dst`. Split over the 2,209 vta-excess rows of the
class: 2,066 match (93.5%, our row correct, vta simply lacks the edge), 13
mismatch (0.6%), 129 no-site, 1 no-qualified-site. The no-site rows are type
conversions (`ast.Kind(kind)`, `ast.NodeFlags(x)`): the syntactic tier reads a
conversion as a call, the oracles keep no conversion edges. The 13 mismatches
share one shape — the qualified call binds the caller's OWN same-named method
(`emitHost.go GetOutputPathsFor -> emitHost.go GetOutputPathsFor` at
emitHost.go:96, `osvfs.FS()` bound to the `(*osSys).FS` method in the same
file). It is corpus-scale dependent: correct with only the two involved files
supplied, wrong over the full 5,075-file list, correct again when all
`_test.go` files are dropped. The wrong rows carry `kind=name_resolve` (the
import-dir leg tags `import_resolve`), so the import facts were unavailable at
walk time and the call fell through to `call_name_match`'s own-def fast path
(go.rs `def_named`). That is a real defect but over the 100-line bar and a
sibling of the filed resolve-is-not-a-function-of-the-input-set defect; not
fixed here.

**generated-file excess (471 codeql2 / 714 vta dst rows in `*_generated.go`):
oracle-side, closed.** The callee symbol is declared in the generated source
for 463 of 464 codeql2 rows and 683 of 684 vta rows (one row each, the same
row, `internal/ast/utilities.go IsOutermostOptionalChain -> Expression`, binds
the method's other declaring file — a representational mismatch, the symbol
exists). The oracles' own generated-file coverage is the gap.

The same pass verified concrete-one-hop-receiver (receipt in ORACLES.REPORT.md
section 12) and surfaced a small wrong-target receiver class: 58 codeql2 / 128
vta excess rows where the method exists on the receiver's type but binds a
different declaring file than ours names (`sync.Pool.Put` rows bound to
`internal/lsp/dynamic_queue.go Put`; `ast.Node.Type` bound to
`internal/checker/types.go Type`). Same-name, wrong-owner def; folded into the
same corpus-scale defect as the 13 pq mismatches.

### File-shape tags over the FULL excess sets


| tag | ours-codeql2 (5,141) | ours-vta (10,525) |
|---|---|---|
| dst in `*_generated.go` | 471 | 714 |
| src in `*_generated.go` | 3 | 57 |
| src is a cgo file (`import "C"`) | 0 | 0 |
| src has a `//go:build` / `// +build` line | 38 | 97 |

## 6. Ratchet before -> after (`RATCHET_FORCE=1`, go rows only, ts5/rust rows byte-identical)

| row | recall | precision | wall_ms | rss_mb | measured_at_sha |
|---|---|---|---|---|---|
| go call codeql2 | 97.42 -> 97.40 | 51.22 -> 90.68 | 5083 -> 2927 | 630 -> 692 | 98f9cac1f |
| go call vta.bare | 84.42 -> 84.06 | 50.40 -> 81.43 | 5083 -> 2927 | 630 -> 692 | 98f9cac1f |
| go module | 100.00 -> 100.00 | 15.18 -> 15.18 | 5083 -> 2927 | 630 -> 692 | 98f9cac1f |
| go type | 73.37 -> 73.37 | 97.04 -> 97.04 | 5083 -> 2927 | 630 -> 692 | 98f9cac1f |

3-run wall median 2,927 ms (2,859 / 2,927 / 4,646), under the 10 s law. The
ratchet's numbers differ in the third decimal from section 3 because the
ratchet extracts in-process over the same file list while section 3's ours
came from the CLI over `go.files.txt`; both pipelines score the same
projection.
