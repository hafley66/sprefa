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
- [The codeql-agreed set: edges two tools bind and we did not](#codeql-agreed)
- [The residual five, taken](#residual-five)

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

<a id="codeql-agreed"></a>
## The codeql-agreed set: edges two tools bind and we did not

Set: `(codeql2 INTERSECT vta bare) MINUS ours`, one process per corpus
(`go.resolve.promoted2.runs.tsv`, 5,075 files, rc=0). Two independent tools
naming one edge makes the row a gap in our resolve legs, never oracle scope.
Sets from `go.codeql_gap.py`; per-row classes from `go_gap_classify/`, which
reads `go/packages` + `go/types` so the class is the frontend's own answer,
not a text guess. Full-set classification, so projection IS the count; the
300-row seed-7 sample beside it (`go.codeql_agreed_missed.sample300.tsv`)
agrees within noise.

Not a class here: interface dispatch, closure naming, method values, generic
instantiation, stdlib. codeql's pass-2 query resolves `call.getTarget()`
statically and names the interface METHOD, while vta names the concrete
implementer, so those rows never intersect and never enter this set. Same for
`fn$N` closure callees. The agreed set is exactly the statically-bindable
edges.

| | rows |
|---|---:|
| vta bare oracle | 55,099 |
| codeql 2.26.4 | 48,529 |
| agreed (codeql AND vta) | 45,406 |
| agreed and missed, before | 5,077 |
| agreed and missed, after | 3,041 |

### Classes, before and after the promoted-method fix

`go.codeql_agreed_missed.classes.tsv` (before), `.after.classes.tsv` (after),
five columns: the normal form plus the class. The evidence text and the caller
`file:line` ride the 300-row sample only, to keep every file under 1 MB.

| class | before | after | owning leg | one caller site |
|---|---:|---:|---|---|
| embedded-struct promoted method | 2,322 | 302 | `go_promoted_method` (go.rs:3272), `go_embeds_of_dir` (go.rs:3347) | `internal/api/encoder/encoder.go:394`, `:414` (`*ast.SourceFile` promotes `AsNode` 2 embeds deep) |
| alias receiver: `type Expression = Node`, methods owned by `Node` | 929 | 926 | `go_method_in_dir` (go.rs:3236) matches `owner_of` by written name; needs a `type X = Y` table in `GoFileFacts` | `internal/api/encoder/encoder.go:776`, `:761` (`*ast.ModuleName`, `*ast.LiteralLikeNode`) |
| multi-hop receiver chain | 820 | 816 | `go_chain_receiver_target` (go.rs:3395) | `internal/api/encoder/decoder_generated.go:410`, `internal/api/encoder/encoder.go:833` |
| one-hop receiver our scope never typed (range var, field read, index read, multi-value define) | 820 | 811 | `go_seed_top_scope` (go.rs:1270), `go_walk_receivers` (go.rs:1365) | `internal/api/proto.go:958`, `internal/api/protocol_msgpack.go:126` |
| bare in-package call, corpus-wide name not unique | 105 | 105 | `GoSource::call_name_match` (go.rs:2733) is corpus-wide, never the caller's own dir first | `internal/api/server.go:100` (`NewSession` also at `internal/project/session.go:230`), `internal/api/session.go:214` |
| import-qualified call shadowed by a same-named METHOD in the target dir | 81 | 81 | `GoModuleIndex::resolve_in_dir` (go_modules.rs:291) counts methods as candidates | `internal/binder/binder.go:827` (`ast.IsExternalModule` vs `func (s *Symbol) IsExternalModule` at `internal/ast/symbol.go:23`), `internal/api/session.go:2970` |

The last two rows are one root cause: a free-function leg must exclude defs
that carry an `owner_of` entry, and the bare leg must scope to the caller's
directory before it falls to a corpus-wide name. 186 rows, not taken here.

### What the promoted-method fix moved

`go_receiver_target` matched a def whose OWNER equals the receiver's own type
name, so every call to a method reached through an embedded field declined.
The walk now runs breadth first over `go_embeds_of_dir` when the type declares
no such method, which is Go's own shadowing rule; the shallowest depth wins and
a tie at one depth binds nothing. Depth cap 4 covers 2,199 of the 2,322 rows;
123 sat deeper (the `ast.Node` hierarchy reaches 9 embeds).

| receipt | before | after |
|---|---:|---:|
| ours rows | 84,459 | 86,630 |
| overlap with vta bare | 41,806 | 43,843 |
| recall (overlap / 55,099) | 75.87% | **79.57%** |
| precision (overlap / ours) | 49.50% | 50.61% |
| recall against codeql (overlap / 48,529) | 88.49% | 92.72% |
| agreed and missed | 5,077 | 3,041 |
| median wall, one process, 3 runs | 9,527 ms (1 run) | 9,147 ms (10,060 / 9,024 / 9,147) |

Precision rose with recall, so the 2,171 new rows are not noise. The first cut
of the walk scanned the whole path list per (dir, struct) and cost 14,716 ms, a
10-second-law defect; `go_dir_index` (go.rs:3323) groups the resolve universe's
paths by directory in ONE pass and put the wall back under the law.

Pins: `tests/69_go_promoted.rs`, 8 tests, fixtures `tests/fixtures/go_promoted`.
HEAD failure before the fix is in that file's header.

<a id="residual-five"></a>
## The residual five, taken

Same corpus and same one-process recipe, this lane's own build. The recipe is
order sensitive: `find internal cmd -name '*.go'` order reproduces the #577
receipt row for row, `| sort` loses 5,147 edges, and the two runs otherwise
differ in nothing. That is a separate defect, filed below, not measured here.

```
cd /Users/chrishafley/projects/typescript-go
find internal cmd -name '*.go' > relfiles.txt          # 5,075 files, find order
xargs -s 900000 timeout 30 <extract> --resolve < relfiles.txt > all_resolved.jsonl
python3 plans/extract-bench-2026-08-29/normalize.py resolved all_resolved.jsonl \
  /Users/chrishafley/projects/typescript-go go.parse.call.tsv go.parse.type.tsv
python3 plans/extract-crawl-2026-08-29/go.codeql_gap.py go.parse.call.tsv \
  plans/extract-bench-2026-08-29/go.oracle.call.vta.bare.tsv \
  plans/extract-bench-2026-08-29/go.codeql2.call.tsv --out missed.tsv
plans/extract-crawl-2026-08-29/go_gap_classify <corpus> missed.tsv classes.tsv
```

| receipt | #577 | this lane |
|---|---:|---:|
| ours rows | 86,324 | 91,837 |
| overlap with vta bare | 43,843 | 46,517 |
| recall (overlap / 55,099) | 79.57% | **84.42%** |
| precision (overlap / ours) | 50.79% | 50.65% |
| unique call rows lost | | 93 |
| recall against codeql (overlap / 48,529) | 92.72% | 97.17% |
| agreed and missed | 3,041 | **1,083** |
| median wall, one process, 3 alternating runs | 11,684 ms | 11,969 ms |

`go_gap_classify` rerun over the 1,083, full-set classification:

| class | #577 | this lane | what took it |
|---|---:|---:|---|
| one-hop receiver never typed | 811 | 487 | range element by Go's own arity rule, type switch case, field read, index read, type assertion, parenthesized conversion `(*T)(x)`, and `TypeBinding::Chained` for an rhs chain this file cannot name |
| embedded-struct promoted method | 302 | 289 | untouched; the residual sits past the depth-4 cap |
| multi-hop receiver chain | 816 | 195 | the plan no longer demands a call hop, so `a.b.M()` and `pkg.F().M()` record; hops replay as `GoTypeId` (declaring dir + bare name) so a cross-package field or result resolves through its OWN file's imports |
| alias receiver `type A = B` | 926 | 89 | `GoFileFacts.aliases` + `go_aliases_of_dir`; `go_method_on_type` follows the chain, cap 4, cycle guarded |
| bare in-package call, name not unique | 105 | 12 | `resolve_call_in_own_dir`: Go's package block runs before any corpus-wide name guess |
| import-qualified call shadowed by a method | 81 | 11 | `free_sites_in_dir` cuts every def carrying an `owner_of` entry |

Row movement against #577: 5,220 rows gained (2,442 of them in the vta oracle),
93 lost (22 in the oracle). The 22 are receivers the new typing binds to a
concrete type that declares no such method, where the old corpus-wide name
guess happened to be right.

Two corrections to the table above this section. `multi-value define` was
already built (the i-th result slot binding), so it cost nothing here; and the
81-row import-qualified class also needed the phase-1 `is_import` test to lose
to a local of the same name (`checker, done := ...; checker.GetSymbolAtLocation`),
which `go_shadowing_receiver_target` now settles.

Filed, not taken:

- **Resolve output depends on input file ORDER.** Same 5,075 files, one
  process, the #577 binary: `find` order gives 86,324 call rows, the same list
  through `sort` gives 81,177. A resolve run must be a function of its input
  SET, and every number in this file is `find`-order only.
- `tests/fixtures/go_modules/module_a/shadow.go` was not valid Go: a
  package-level `Widget` plus a dot-imported `Widget` in one package is a
  redeclaration, and the test pinned the accident. Moved to its own package
  `module_a/shadowpkg`.
- The wall is over the 10-second law on this machine at HEAD too (11,684 ms
  before, 11,969 ms after, +2.4%), where the #577 lane measured 9,147 ms. The
  law is broken by the pre-existing cost, not by this change.

Pins: `tests/71_go_residual.rs`, 17 tests, fixtures `tests/fixtures/go_residual`.
HEAD failure (13 of 17) is in that file's header. Classes beside this file:
`go.codeql_agreed_missed.residual5.classes.tsv`; run row
`go.resolve.residual5.runs.tsv`.

<a id="residual-six"></a>
## Lane `fix/extract-go-residual-6`: the bound-type qualifier, the Elem hop, the promoted field

Same corpus and the residual-five recipe, with ONE change: `xargs -n 10000`
forces a single process. The `xargs -s 900000` recipe above SPLITS 5,075
files into two invocations (5,000 + 75, macOS xargs' per-invocation cap), and
the second process resolves against a DefIndex of only its own 75 files, so
every cross-package leg dies for those files. That is the filed
order-dependence defect: find vs sort order only changed which 75 files fell
into the tail batch. Every number below is one process.

| receipt | #579 | this lane |
|---|---:|---:|
| ours rows | 91,837 | 93,728 |
| overlap with vta bare | 46,517 | 47,251 |
| recall (overlap / 55,099) | 84.42% | **85.76%** |
| precision (overlap / ours) | 50.65% | 50.41% |
| recall against codeql (overlap / 48,529) | 97.17% | 98.73% |
| agreed and missed | 1,083 | **351** |
| median wall, one process, 3 runs | 11,969 ms | **6,860 ms** (7.74 / 6.82 / 6.86) |

`go_gap_classify` rerun over the 351 (full set,
`go.codeql_agreed_missed.residual6.classes.tsv`):

| class | #579 | this lane | what took it |
|---|---:|---:|---|
| one-hop receiver never typed | 487 | 155 | two root causes: (a) `ret_of` stores the result type as the DECLARING file writes it (`*types.Widget` through its own import) and `go_qualify_bound_type` prefixed the caller's qualifier, yielding `callee.types.Widget` whose first dot named the wrong package — every downstream define on such a call died with it; (b) a range over a call result had no chain: `ret_of` recorded NO text for slice/array/map results, so `for _, it := range sh.Items()` typed nothing |
| embedded-struct promoted method | 289 | 8 | the embed walk cap 4 -> 9 (the ast.Node hierarchy height); wall stays under the law |
| multi-hop receiver chain | 195 | 139 | a chain's Field hop now follows Go's field promotion through embeds (`node.Kind.String()` where `Kind` sits on `TypeSyntaxBase`); the residual needs var-reassignment binds and mid-chain type assertions, filed below |
| alias receiver | 89 | 26 | rode the same bound-type fix; the residual pairs an alias with a multi-hop operand |
| bare in-package call, not unique | 12 | 12 | untouched |
| import-qualified call shadowed | 11 | 11 | untouched |

Filed, not taken:

- `x := y.M()` REASSIGNMENT (`=` on a name declared elsewhere) records no
  bind plan row, so a receiver typed from a reassigned value never binds
  (~40 of the one-hop residual, e.g. `userPreferences = workingSnapshot
  .UserPreferences()`).
- A mid-chain type assertion (`x.(*ast.Node).Name()`) has no chain step: the
  asserted type is written and static, but `go_chain_of` and the replay have
  no `Assert` hop (~30 of the multi-hop residual).
- The macOS xargs split above is a HARNESS defect, not an extractor one; the
  recipe in this file now pins `-n 10000`.

Pins: `tests/72_go_bound_qualify.rs`, `tests/73_go_range_elem.rs`,
`tests/74_go_field_promote.rs`, and `tests/69_go_promoted.rs`'s
`depth_five_embed` (rewritten from `depth_five_declines`). HEAD failures in
each file's header.
