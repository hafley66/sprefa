# What sprefa-extract does, and how we know the numbers mean anything

TOC
1. What one run is
2. The trait, and who goes around it
3. What comes out (the records)
4. Excess and leaks: how a wrong edge is counted
5. The oracles and tools: the exact question each was asked
6. Staging: why the numbers moved 30 points in one day without a code change
7. What is the same across languages and engines, and what is not
8. Where each language's ceiling is
9. Why to trust it, and why not

## 1. What one run is

`extract --resolve --project-root <corpus> <files>` is one process over the
whole corpus. Step trace for go on typescript-go (5,097 files):

```
step 0  read     5,097 files -> bytes                       (project.rs:594 read_inputs_inner)
step 1  parse    one tree-sitter tree per file               (project.rs:687 dispatch)
step 2  facts    per file: defs, call sites, imports, receivers, embeds, aliases
step 3  index    def_index (name -> def sites), kind_index, path_index   (project.rs:218-226)
step 4  modules  GoModuleIndex::build: package dir -> exported names       (project.rs:284)
step 5  resolve  per file, in parallel: Resolve<CallF>::resolve(output, cx) (project.rs:293)
                   for each call site:
                     import-qualified?  -> module plane                (ImportResolve)
                     receiver typed?    -> (type, method) impl lookup  (NameResolve)
                     receiver interface -> one edge per implementer    (Implements, cap 64)
                     bare name          -> own dir, then corpus-unique (NameResolve)
                     else               -> unresolved{reason}
step 6  types    Resolve<TypeF>::resolve                                 (project.rs:1015)
step 7  emit     sorted jsonl rows: resolved_edge / resolved_import / resolved_type_edge / unresolved
steady state: 6,680 ms, 601 MB, byte-identical across runs
```

`--family diet_scip` is the same path (`project.rs:475`); the name is the
technique (parse + name matching, no compiler), never a subset.

## 2. The trait, and who goes around it

```rust
pub trait Resolve<F: Family>: Source {                      // types.rs:1872
    fn resolve(&self, output: &ExtractOutput, cx: &ProjectCx) -> Vec<ProjectEdge<F>>;
}
```

| language | `Resolve<CallF>` | `Resolve<TypeF>` | goes through the trait | goes around it |
|---|---|---|---|---|
| go | go.rs:3907 | go.rs:2817 | call and type edges | `go_publish_file_facts` (project.rs:277) publishes per-file facts before the module index builds |
| rust | rust.rs:1243 | rust.rs:849 | call and type edges | `rust_scip_macros::mint_macro_edges` post-pass (project.rs:327) mints `ScipMacro` edges |
| ts | ts.rs:3885 | ts.rs:3408 | call and type edges | none |
| all three | | | | `import_facts` (project.rs:1135) reads the three module indexes directly and writes `resolved_import` rows; the module plane never passes through `Resolve` |

Dispatch is one table, `RESOLVE_ARMS` (project.rs:913): each language
registers `call`, `types`, and a `drops` callback. A language without an
impl is a compile error, never an empty result (types.rs:1868).

Receiver typing sits inside each language's arm, not in the trait:
ts_receivers.rs (called from ts.rs:3514 onward), rust_receivers.rs (rust.rs:1851,
and the impl table in rust_modules.rs:23), go's inline in go.rs. So the
trait fixes the shape of the answer; each language's algorithm behind it is
bespoke, on purpose: "module resolution is the language's own algorithm".

## 3. What comes out

| record | columns | who emits |
|---|---|---|
| `resolved_edge` | caller_path, caller_name, callee_path, callee_name, site span, kind | every `Resolve<CallF>` arm, plus the rust macro post-pass |
| `resolved_type_edge` | owner, target, kind | every `Resolve<TypeF>` arm |
| `resolved_import` | src_path, name, local, target_path, target_name, kind, hops | `import_facts`, off the module indexes |
| `unresolved` | site, reason | the `drops` callback per language |

One shared enum for call kinds (types.rs:448): `NameResolve`, `ScipOverride`,
`ValueRef`, `ImportResolve`, `Implements` (go only today), `ScipMacro` (rust
only). One shared enum for drop reasons (types.rs:608): `NoCorpusDef`,
`Ambiguous`, `Builtin`, `Inferred`, `External`, `FanoutCap`, plus three
phase-1 markers. The `resolved_import.kind` column is NOT one enum: three
per-language enums share a wire vocabulary (ts_resolve.rs:394,
rust_modules.rs:267, go_modules.rs:171).

The drop reasons are the honest half of the output: every call site that
did not become an edge says why, and the gap classifications
(go.GAPS.md, ts.GAPS.md, rust.REPORT.md section 18) were built by sampling
those rows and reading the source.

## 4. Excess and leaks

There is no false-positive accounting inside `src/`. It lives in the bench:

```
normal form   src_path \t src_name \t dst_path \t dst_name       (normalize.py:19)
recall     =  |ours ∩ oracle| / |oracle|                          (tests/bench/mod.rs:259)
precision  =  |ours ∩ oracle| / |ours|
excess     =  ours − oracle   (bench.py prints it as "a-only", with 20 samples)
leak       =  oracle − ours   ("b-only")
```

Hazard, recorded at ORACLES.REPORT.md:583: `bench.py` prints the two labels
the other way round. The ts and rust lanes from #575 to #578 copied
bench.py's labels, so their PR bodies say "recall 70%" where the ratchet
(the canonical definition) says recall 84.88%, precision 71.16%. The ratchet
numbers are the ones to read.

Per-language excess today (precision column of RATCHET.tsv): go call 50.4%,
ts call 71.2%, rust call 33.7%. Half of go's and two thirds of rust's emitted
edges are not in the compiler's set. Part of that is oracle scope (vta names
implementers, we also name the interface method; ra_ap_ide's call hierarchy
is per-crate), part is real over-emission. Nobody classified the excess
sets the way the leak sets were classified. That is the open study.

## 5. The oracles and tools: the question each was asked

| oracle or tool | lang | exact question | yields | caveat on record |
|---|---|---|---|---|
| tsc TypeChecker (`oracle_ts.mjs`) | ts | `checker.getResolvedSignature(node).declaration` for every Call/NewExpression; imports via `ts.resolveModuleName` | call, module | closures named by enclosing fn; `<module>` as fallback |
| go/callgraph vta (`oracle_go/main.go`) | go | `packages.Load("./...")`, SSA, `cha.CallGraph` then `vta.CallGraph`; imports via `ast.File.Imports` | call, module, type (go/types) | writes `Type.Method`; we compare against the `.bare.tsv` with the receiver stripped |
| rust-analyzer `ra_ap_ide` (`ra_ide_probe/main.rs`) | rust | `Analysis::file_structure` then `outgoing_calls` per def | call, type | first cut dropped every impl-block method (14,976 of 24,213 defs); fixed before measuring |
| codeql pass 2 | ts | `getResolvedCallee()` (`TypeResolution::callTarget`) | call | pass 1 asked `getACallee(2)` and filtered same-file edges: 34 rows |
| codeql pass 2 | go | `DataFlow::CallNode.getTarget().getFuncDecl()` | call | pass 1 was name-based: `Error` matched every `Error` |
| codeql | rust | none | | extractor panics at `str/mod.rs:861` |
| joern pass 2 | go, ts | `cpg.call` with `callee.isExternal(false)`, names rebuilt from `method.fullName` | call | go needed `go.mod` `tool (...)` directives patched out |
| madge, dependency-cruiser | ts | `madge --json`, `depcruise --output-type json` | module | route through `_namespaces` barrels; ours binds the direct target |
| stack-graphs | ts | `query definition` per import position | module | 703 of 1,917 positions "file not indexed" |
| raw scip | all | `extract --family scip` over the compiler's index | call, type | root scoping: wrong root sweeps 21,107 docs and exits 0 |
| glean, kythe | | not run | | Linux-only; glean carries no call predicate for these languages |

## 6. Staging

Every "ours" number before 18:50 was a floor: `resolve_runs.py` split the
corpus by directory and every index is per-process, so no cross-directory
edge could exist (docs/failure-modes.md entry 96; ORACLES.REPORT.md section
14). Re-measured single-process with the same binary:

| lang | family | chunked | one process |
|---|---|---|---|
| go | call | 32.1% | 75.9% |
| ts | call | 39.6% | 70.0% (bench.py label; 84.9% by the ratchet definition) |
| rust | call | 46.8% | 50.4% |

Every ratchet number: 3 in-process runs, median wall, facts from the last
run, 30 s budget per call (tests/bench/mod.rs:32,317). Third-party tools:
one run each, wall recorded. Corpora pinned: TypeScript-5.9 `7e133bea1`
(600 files), typescript-go `89d5d5b2` (5,097), rust-analyzer `af4111f` (873).
The evidence is the committed tsvs (about 90 under
plans/extract-bench-2026-08-29/), not the PR bodies.

## 7. Same across languages and engines, and not

| axis | same | different |
|---|---|---|
| output shape | one `Resolve<F>` trait, one call-kind enum, one drop-reason enum, one normal form | import kinds are three enums; `Implements` is go-only, `ScipMacro` rust-only |
| oracle grade | each is the language's own compiler front end | go's is a whole-program call graph (vta), ts's is per-site signature resolution, rust's is an IDE call hierarchy: three different notions of "the call edge" |
| tool queries | all reduced to the same four-column tsv | codeql got two passes; joern got two; glean and kythe got none |
| measurement | one process, 3 runs, median, same binary before/after | ts and rust reports carry the swapped labels; go reports do not |
| timeout | | 10 s in COMMON.md, 30 s in the ratchet, 60 s in resolve_runs.py |

## 8. Ceilings

| lang | recall now | what is left, from the leak classifications |
|---|---|---|
| go | 84.4% (codeql 82.4%) | 1,083 rows codeql and vta agree on: untyped one-hop receivers 487, promotion past depth 4 289, multi-hop 195; the rest is interface dispatch where vta names the implementer and nothing static can |
| ts | 84.9% (codeql 88.6%) | namespace-merged receivers, interface fan-out, destructured receivers landed in #578; the residual was never re-classified after it (the lane was stopped) |
| rust | 68.6% | 8,650 of 11,470 ambiguous sites name external types (`Vec::push`, `FxHashMap`, `Arc::new`): no corpus edge is correct; the in-corpus remainder is trait default bodies and generic bounds |
| all | | scip-informed resolve reads ts 88.6% and rust 88.0% today but costs 113 to 436 s: `scip::site_occurrence` scans the index per site (#585) |

## 9. Trust

For: same binary and same file list on both sides of every before/after;
facts byte-identical across runs; three compilers as oracles, not our own
output; the leak sets were sampled and read by hand with file:line examples;
the ratchet fails a PR that lowers a floor by 0.1 point.

Against: the ts and rust PR bodies from #575 to #578 carry swapped
recall/precision labels; the excess sets are unclassified, so precision has
no story; the three oracles measure three different things; the go scip
table in #585 reads 9.1% on every row, which is the `Type.Method` mismatch
again and was never rerun; codeql rust never ran, so rust has no
third-party mark at all.
