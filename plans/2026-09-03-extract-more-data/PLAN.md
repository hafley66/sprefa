# extract: every axis along which more data can come (2026-09-03, lane research-more-data)

User word, verbatim: "go find out a new way to get more data with all possibilities".
Data = facts the extractor emits, per language, per family, per tier, scored against
oracles and ratcheted in `plans/extract-bench-2026-08-29/RATCHET.tsv`.

Read-only research on `origin/main` a5d6099cd (worktree `sprefa-wt/research-more-data`).
No cargo build ran. Every count below names the command that printed it. The one
binary run is the prebuilt `v6/sprefa-extract/target/release/extract` of the main tree,
dated Sep 1 20:53 (`ls -la`), which predates `--witness` (PR #645) and the go/python
tsi arms (PR #698, #700); it was used only for the per-file family census in sec 2.

## TOC

1. Method and receipts
2. Axis 1: languages (source arms, grammars compiled in, one dep away)
3. Axis 2: family x language x tier matrix
4. Axis 3: oracles, per language and family, and the gaps with candidate tables
5. Axis 4: held-out corpora and the parked heldout lane
6. Axis 5: trace oracle candidates per language
7. Axis 6: mutation battery and `resolution_origin` as data about the resolvers
8. Axis 7: sprefa as its own corpus (rust, ts, prolog, dl6, md)
9. Axis 8: the dl7/v7 consumer side
10. Ranking of every row and the top 8 dispatchable arcs
11. Three surprises with file:line
12. Stale claims found in CLAUDE.md and plan docs

## 1. Method and receipts

| what | command | result |
|---|---|---|
| source roster | `sed -n 87,100p v6/sprefa-extract/src/lang/mod.rs` | 10 `Source`s in first-match order: Rust, Go, Kotlin, Markdown, Prolog, Python, Data, Dl, Ts, Astgrep |
| family mask | `grep -n "pub struct FamilyMask" -A 6 src/types.rs` | `cst, types, call, df, data` (types.rs:2651-2657); `FamilyTag` adds `Flow, Module, Cfg` (types.rs:94-103) |
| per-source mask use | `grep -oE "mask\.[a-z_]+" src/lang/<file>` | sec 3 matrix |
| per-file family census | `extract <fixture> \| grep -oE '"family":"[a-z]+"' \| sort \| uniq -c` | sec 3 |
| tsi arms | `grep -rlE "tsi_rows\|tsi_type_id\|TsiNames" src` | go.rs, go_type_edges.rs, python/_0_source.rs, python/_1_type_edges.rs, rust_type_edges.rs, ts.rs (+ project.rs, tsi/, types.rs, wire.rs) |
| tsi relations emitted per arm | `grep -ohE '"(tsi\|ts\|rust\|go)\.[a-z_]+"' <file> \| sort -u` | sec 3 |
| registry | `grep -oE '"(tsi\|ts\|rust\|go)\.[a-z_]+"' src/tsi/registry.rs \| sort -u \| wc -l` | 36 relations |
| scip indexers | `grep -n INDEXERS -A 40 src/scip_ensure.rs` | 6 rows: rust, typescript, python, go, kotlin/java, cpp (scip_ensure.rs:62-100) |
| oracle files | `wc -l plans/extract-bench-2026-08-29/*.tsv` | sec 4 |
| bench cases | `grep -n "fn cases" -A 120 tests/bench/mod.rs` | 18 `Case` rows, tiers Syntax and Checker only (mod.rs:201-218); `Tier::Scip` exists (mod.rs:72) and no case names it |
| ratchet | `cat plans/extract-bench-2026-08-29/RATCHET.tsv` | 18 accuracy rows, 5 cost rows |
| heldout lane | `git -C .boop-worktrees/lab/extract-heldout-oracles log --oneline -5` | tip 3bfbe9d16, 22 SCORES rows, 4 langs |
| loader | `grep -cE "^tsi_relation\(" v7/src/2_comptime/0c_extract_loader.pl` | 35 on origin/main; 36 on the main tree's dirty branch (adds `tsi.name` at :15) |
| prelude | `grep -c "^(: " v7/prelude/5_tsi_primitives.dl7` | 27 classes (10 ts + 17 rust), no go, no python, no `unit` |
| tools on PATH | `command -v <bin>` (PATH + ~/go/bin + ~/.cargo/bin) | sec 4.4 |

## 2. Axis 1: languages

### 2.1 Languages with a source arm today (`lang/mod.rs:87-100`)

| Source | `matches` | file:line | families it writes (mask fields touched) | tsi rows | checker tier | scip indexer row |
|---|---|---|---|---|---|---|
| RustSource | `.rs` | rust.rs:3266 | cst, types, call, df | syntax (rust_type_edges.rs, 884 lines) + semantic (rust_checker_ra.rs, 956 lines, feature `rust-checker`) | rust-analyzer in-process (project.rs:340, 698) | `rust-analyzer` on PATH |
| GoSource | `.go` | go.rs:2652 | cst, types, call, df | syntax (go_type_edges.rs, 666 lines, PR #698) | none (`grep -c go_checker src/project.rs` = 0) | `scip-go` on PATH (~/go/bin) |
| KotlinSource | `.kt .kts` | kotlin.rs:1604 | cst, types, call, df | NONE (`grep -c tsi_rows kotlin.rs` = 0) | none | `scip-java` MISSING (install via coursier, also missing) |
| MarkdownSource | `.md .markdown` | markdown/_0_source.rs:112 | cst; types only when `mask.types && !mask.cst` (:142) -> `doc_node` heading/code_block rows | none | none | none |
| PrologSource | `.pl .plt .pro` | prolog/_0_source.rs:1052 | cst, types, call, df | none | none | none |
| PythonSource | `.py .pyi` | python/_0_source.rs:2297 | cst, types, call, df | syntax (python/_1_type_edges.rs, 861 lines, PR #700) | none | `scip-python` on PATH (npm -g 0.6.6) |
| DataSource | json jsonl ndjson yaml yml toml | data/_0_source.rs:24-28 | cst (delegated to ast-grep), data (`data_doc`, `data_value`) | none | none | none |
| DlSource | `.dl6` | dl6/_0_source.rs:396 | cst, types, call | none | none | none |
| TsSource | oxc `source_type_for` (.ts .tsx .mts .cts .js ...) | ts.rs:4007 | cst, types, call, df | syntax (ts.rs, 19 hits) + semantic (ts_checker.mjs 744 lines, node subprocess ts_checker.rs:411) | tsc via node (project.rs:355, 763) | `scip-typescript` on PATH (npm -g 0.4.0) |
| AstgrepSource | any `SupportLang::from_path` | astgrep.rs:146-151 | cst only | none | none | cpp row exists, `scip-clang` MISSING |

Module family (`resolved_import`) is a resolve-phase index, not a per-file mask:
`project.rs:1066-1080` builds ts, rust and go module facts only; `import_facts`
(`project.rs:1624-1665`) binds ts_rows, rust_rows, go_rows. `py_module_specifiers`
(python/_0_source.rs:1328) and `kt_module_specifiers` (kotlin.rs:621) exist and emit
`specifier` records (7 on `tests/fixtures/tsi/probe_graph.py`, 3 on
`kotlin_modules/sample.kt`) but nothing indexes them into `resolved_import`.

CFG family: `cfg.rs:123-126` has role tables for `rust`, `go`, `ts`, `kotlin` only.
Measured: `extract --family cfg probe_graph.py` -> 611 cst rows, 0 cfg rows;
`extract --family cfg rust_same_file.rs` -> 10 cfg + 35 cst rows.

Docs family (`DocFact`, types.rs:357): pushed by rust_docs.rs (structs/enums/unions/traits/fns/impl methods; "a documented const or alias mints no row", rust_docs.rs:12), go.rs:273, kotlin.rs:326, python/_0_source.rs:1053, ts.rs:280. Prolog, dl6, markdown push none.

Flow family: `flow_edges` (types.rs:1276) is language-agnostic over `df` + resolved call edges, so every arm with df + call gets it (rust, go, kotlin, python, ts, prolog). dl6 has no df plane (dl6/_0_source.rs touches `mask.call, mask.cst, mask.types` only), so no flow.

### 2.2 Grammars already compiled into the crate

`Cargo.lock` (`grep -oE '^name = "tree-sitter-[a-z0-9-]+"' | sort -u`): 27 grammar crates:
bash c c-sharp cpp css dl6 elixir go haskell html java javascript json kotlin-sg lua md php prolog python ruby rust scala swift toml-ng typescript yaml (+ tree-sitter-language runtime).

ast-grep `SupportLang` (ast-grep-language 0.38.7 `src/lib.rs:241-263`): Bash C Cpp CSharp Css Go Elixir Haskell Html Java JavaScript Json Kotlin Lua Php Python Ruby Rust Scala Swift Tsx TypeScript Yaml = 23 languages reach the cst-only fallback today with zero new deps.

Direct deps beyond ast-grep (`Cargo.toml:110-146`): tree-sitter-go, -python, -kotlin-sg, -prolog, -dl6 (path), -json, -yaml, -toml-ng, -md, -html.

| candidate language | grammar state | oracle available on this machine | indexer row | cost to a full arm (cst+type+call+df) |
|---|---|---|---|---|
| Java | tree-sitter-java compiled (ast-grep transitive); cst today | codeql `java` pack present (`codeql resolve languages`), scip-java row exists (binary missing) | kotlin/java row | L: kotlin.rs is 1,845 lines for the same JVM shape |
| JavaScript (plain .js) | oxc handles it inside TsSource (`source_type_for`) | same as ts | typescript | already covered |
| C / C++ | tree-sitter-c, -cpp compiled; cst today | codeql `cpp` pack present; `scip-clang` MISSING | cpp row | L, plus compile_commands.json dependency |
| C# | tree-sitter-c-sharp compiled | none installed | none | L |
| Swift | tree-sitter-swift compiled | codeql `swift` pack present | none | L |
| Ruby, PHP, Lua, Scala, Elixir, Haskell, Bash, CSS | compiled, cst only | none | none | L each |
| XML | NOT compiled (`grep -c tree-sitter-xml Cargo.lock` = 0); `tree-sitter-xml = "0.7.0"` on crates.io (`cargo search`) | codeql `xml` pack present | none | M: one dep + DataSource-shaped element rows |
| HTML | tree-sitter-html direct dep; cst today (51 rows on `astgrep/sample.html`) | codeql `html` pack present | none | S-M: doc rows (element_path, attr, text) on the data plane |
| Markdown | tree-sitter-md direct dep; cst + doc_node today | none | none | S: links and fence languages are in the cst already (35 `inline_link`, 26 `fenced_code_block` on README.md) but no doc row names them |
| YAML / JSON / TOML | DataSource (data_doc, data_value) | codeql `yaml` pack present | none | built |

`ARCH.pl:811` `doc_format_extraction` is `unbuilt`, yet json/yaml/toml (DataSource), md (MarkdownSource) and html (cst) are all in the roster. Only xml has nothing. The row is stale on 5 of its 6 formats.

## 3. Axis 2: family x language x tier

Legend: E = emitted, P = partial, - = not emitted. Tier columns: syntax = per-file parse, checker = compiler-backed tier, scip = `--family scip` index rows (`scip_*` records, 22 record kinds: `grep -ohE '"scip_[a-z_]+"' src/scip/*.rs src/*.rs | sort -u`).

| language | cst | type | call | df | flow | module (`resolved_import`) | cfg | docs | data | tsi syntax | tsi semantic | scip tier |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| rust | E | E | E | E | E | E (rust_modules.rs) | E | E | - | E 14 relations (rust_type_edges.rs) | E 25 relations (rust_checker_ra.rs) | E (rust-analyzer) |
| ts/js | E | E | E | E | E | E (ts_resolve.rs) | E | E | - | E 16 relations (ts.rs) | E 25 relations (ts_checker.mjs) | E (scip-typescript) |
| go | E | E | E | E | E | E (go_modules.rs) | E | E | - | E 15 relations (go_type_edges.rs) | - | E (scip-go) |
| python | E | E | E | E | E | - (specifiers only) | - | E | - | E 13 relations (_1_type_edges.rs) | - | E (scip-python) |
| kotlin | E | E | E | E | E | - (specifiers only) | E | E | - | - | - | - (scip-java missing) |
| prolog | E | E | E | E (2 rows on corpus_2_meta_use.pl) | E by construction | - | - | - | - | - | - | - |
| dl6 | E | E | E | - | - | - | - | - | - | - | - | - |
| markdown | E | P (`doc_node` only under `--family type` without cst) | - | - | - | - | - | - | - | - | - | - |
| json/yaml/toml | E | - | - | - | - | - | - | - | E | - | - | - |
| html | E | - | - | - | - | - | - | - | - | - | - | - |
| 13 other ast-grep langs | E | - | - | - | - | - | - | - | - | - | - | - |

Per-file census (prebuilt binary, `grep -oE '"family":"[a-z]+"' | sort | uniq -c`):

| fixture | call | cst | df | type | data |
|---|---:|---:|---:|---:|---:|
| origin/rust_same_file.rs | 3 | 35 | 6 | 2 | |
| go_modules/sample.go | 6 | 73 | 6 | 1 | |
| kotlin_modules/sample.kt | 4 | 69 | 6 | 1 | |
| tsi/probe_graph.py | 16 | 611 | 31 | 16 | |
| scip_rel/animal.ts | 2 | 53 | 4 | 3 | |
| packages/README.md | | 45 | | | |
| prolog/corpus_2_meta_use.pl | 19 | 101 | 2 | 1 | |
| dl6/2_callee.dl6 | 1 | 47 | | 3 | |
| data/stream.yaml | | 247 | | | 23 |
| astgrep/sample.html | | 51 | | | |

tsi relation sets per arm (`grep -ohE '"(tsi|ts|rust|go)\.[a-z_]+"' <file> | sort -u`):

| arm | relations |
|---|---|
| rust syntax (rust_type_edges.rs) | rust.impl rust.trait tsi.argument tsi.callable tsi.called tsi.conforms tsi.has_type tsi.input tsi.output tsi.parameter tsi.primitive tsi.product tsi.sum tsi.type |
| ts syntax (ts.rs) | ts.interface ts.optional ts.readonly tsi.argument tsi.callable tsi.called tsi.conforms tsi.denotes tsi.has_type tsi.input tsi.output tsi.parameter tsi.primitive tsi.product tsi.sum tsi.type |
| go syntax (go_type_edges.rs) | go.embedding go.interface go.type_set tsi.argument tsi.callable tsi.called tsi.denotes tsi.has_type tsi.input tsi.output tsi.parameter tsi.primitive tsi.product tsi.symbol tsi.type |
| python syntax (_1_type_edges.rs) | tsi.argument tsi.callable tsi.called tsi.denotes tsi.has_type tsi.input tsi.output tsi.parameter tsi.primitive tsi.product tsi.sum tsi.symbol tsi.type |
| ts semantic (ts_checker.mjs) | + ts.conditional ts.mapped tsi.assignable tsi.edge tsi.equivalent tsi.name tsi.origin tsi.subtype tsi.symbol |
| rust semantic (rust_checker_ra.rs) | + rust.assoc rust.lifetime rust.ownership tsi.assignable tsi.denotes tsi.edge tsi.equivalent tsi.name tsi.origin tsi.subtype tsi.symbol |

Registry rows with ZERO emitter anywhere under `src/lang` or `src/tsi/semantic.rs`
(`grep -rl '"tsi.value"' src/lang src/tsi/semantic.rs`): `tsi.value`, `tsi.value_argument`, `tsi.scip_symbol`.
They are declared at `issues/extract-semantic-fact-roundtrip/item.md:193-194` and registered
in `src/tsi/registry.rs`; the v7 loader lists all three as `graph_relation` (loader :57-58). Three relations the consumer is wired for and nobody produces.

Checker tiers: `project.rs:340` (rust, in-process ra) and `:355` (ts, node subprocess). `tests/bench/mod.rs` `Tier::Scip` (:72) has no `Case`. The heldout `run.py:98-99` runs `["syntax","checker"]` only where `checker` is set: ts and rust; go and python have `None`.

## 4. Axis 3: oracles

### 4.1 Oracle files that exist (`wc -l plans/extract-bench-2026-08-29/*.tsv`)

| lang | family | file | rows | tool | in `oracle_files()` (bench mod.rs:170-180) |
|---|---|---|---:|---|---|
| ts5 | call | ts5.oracle.call.tsv | 84,958 | tsc TypeChecker (oracle_ts.mjs) | yes |
| ts5 | call | ts.codeql2.call.tsv | 53,140 | codeql 2.26.4 | yes |
| ts5 | call | ts.joern2.call.tsv | 24,451 | joern 4.0.614 | no (TOOLS.REPORT pass 2: 32.3% recall, rejected) |
| ts5 | call | ts5.jelly.call.tsv | 49,290 | jelly | no (OPEN-PROBLEMS row 9: rejected, 35-37% agreement) |
| ts5 | call | ts5.scip_override.call.tsv | 30,289 | scip-typescript | no |
| ts5 | module | ts.madge.module.tsv | 2,011 | madge 8.0.0 | yes (50.57 / 32.85, never grinded) |
| ts5 | module | ts.depcruise.module.tsv | 2,011 | dependency-cruiser | no (PRIOR-ART sec 9 proposes it) |
| ts5 | module | ts.stackgraphs.module.tsv | 1,208 | stack-graphs | no |
| ts5 | module | ts5.oracle.module.tsv | 2,022 | tsc | no |
| ts5 | type | NONE | 0 | | the only ts family with no oracle (ts5.parse.type.tsv is ours) |
| go | call | go.oracle.call.vta.bare.tsv | 55,099 | golang.org/x/tools callgraph vta (oracle_go/main.go) | yes |
| go | call | go.oracle.call.cha.tsv | 172,957 | cha | no |
| go | call | go.codeql2.call.tsv | 48,529 | codeql | yes |
| go | call | go.joern2.call.tsv | 31,617 | joern | no |
| go | module | go.oracle.module.tsv | 2,152 | oracle_go | yes (100.00 / 15.18) |
| go | type | go.oracle.type.typedecl.tsv | 6,204 | go/types (`main.go:258-400`, ORACLES.REPORT 13.2) | yes |
| go | type | go.oracle.type.tsv / .kinds.tsv | 44,214 / 44,869 | go/types full (ref, implements, extends) | no (7.0% recall on the full set) |
| rust | call | rust.oracle.call.tsv | 27,004 | rust-analyzer ide probe | yes |
| rust | call | rust.codeql.call.tsv | 52,744 | codeql rust pack | yes |
| rust | call | rust.scip_override.call.tsv | 18,691 | rust-analyzer scip | yes |
| rust | type | rust.oracle.type.typedecl.tsv | 8,343 | ra_ap_ide | yes |
| rust | type | rust.oracle.type.tsv / .kinds.tsv | 43,134 / 43,689 | ra | no |
| python | call | python-oracle/SCORES.tsv | 137 cases, 236 edges, 18 categories (`ls python-oracle/suite | wc -l`), 119 `main.py` | PyCG micro suite | no (own scorer `pycg_score.py`, not the ratchet) |
| kotlin | any | NONE | 0 | | |
| prolog / dl6 / md / data | any | NONE | 0 | | |
| any | dataflow / flow_edge | NONE | 0 | | |
| any | docs | NONE | 0 | | |
| any | cfg | NONE | 0 | | |

Ratchet rows (`RATCHET.tsv`): 18 accuracy rows over {ts5, go, rust} x {call, module, type} x {syntax, checker}; cost rows 5. Python, kotlin, dataflow, docs, cfg, tsi have no ratchet row.

### 4.2 Gap: ts type oracle

| candidate | what it gives | cost | verdict receipt |
|---|---|---|---|
| tsc `checker.getTypeAtLocation` + `getSymbolAtLocation` (already loaded by ts_checker.mjs, and by `oracle_ts.mjs` for calls) | (owner_decl, referenced type name) rows in the go/rust `typedecl` normal form | S: extend oracle_ts.mjs with the `writeTypeEdges` shape of oracle_go/main.go:258-400 | self-referential with the checker tier, same relation the rust and go type oracles already accept (ra_ap_ide for rust, go/types for go) |
| codeql javascript `TypeAccess` / `TypeAnnotation` | type refs per declaration | M: a .ql beside `tools/` (codeql js pack present: `codeql resolve languages | grep -c javascript` = 1) | independent tool, same 4-col form as ts.codeql2.call.tsv |
| scip-typescript `scip_relationship is_type_definition` + `scip_signature_occurrence` | type refs inside rendered signatures | S: the records are already decoded (`scip_relationship`, `scip_signature_occurrence` in the 22-kind list); ORACLES.REPORT sec 8 lists them as "never read" | reference graph, same protocol caveat as sec 5 |

### 4.3 Gap: dataflow (`flow_edge`) oracle, every language

| candidate | on this machine | what it gives | cost | build-vs-buy |
|---|---|---|---|---|
| CodeQL dataflow library (`DataFlow::localFlowStep`, `TaintTracking`) for go/js/python/rust | codeql 2.26.4 with go, python, rust, javascript, java, cpp, swift packs (`codeql resolve languages`) | local + interprocedural flow step pairs at (file, line) grain | M: one .ql per language; databases already built once for the call oracles (TOOLS.REPORT "db reused") | the established buy; eval PLAN.md sec 6 names it for Arc E |
| joern `reachableBy` | joern MISSING on PATH now; pass-2 measured 35.9% / 32.3% call recall (TOOLS.REPORT pass 2) | flow pairs | M, cpg 7m per corpus | rejected on the call plane; no reason it would do better on flow |
| python `sys.monitoring` trace with argument identity | python 3.14.6 (`hasattr(sys,'monitoring')` = True) | executed arg -> param bindings | S-M | sec 6 |
| rust MIR borrowck facts (`-Z dump-mir`, polonius facts) | nightly only | intra-fn flow | L | not comparable to our ArgToParam/RetToCallRes edge kinds |

### 4.4 Gap: language oracles

| lang | candidate | on PATH | gives | cost | verdict |
|---|---|---|---|---|---|
| python | PyCG (vendored, `python-oracle/PyCG.LICENSE`) on the 3 corpus-stats repos flask/click/requests (cloned at `~/corpora`) | pip list shows no pycg; suite runs from the vendored copy | corpus-scale call oracle | S | eval PLAN.md Arc A names it; unrun |
| python | scip-python (pyright-backed) | yes, 0.6.6 | reference graph (sec 5 caveat) | S | heldout already ran it: 3 repos |
| python | pyright LSP / `pyright --outputjson` | MISSING | diagnostics only, no edge output without driving the LSP | M | scip-python is the packaged form of the same engine |
| python | mypy `mypy.build` API (`BuildResult.types` expr -> Type) | MISSING | inferred types per expression, call targets via `CallExpr.callee` type | M | the only python TYPE oracle candidate with a programmatic API; jedi is per-name goto (slow) |
| python | codeql python pack | present | call + dataflow | M | same query shape as go/ts |
| python | trace (sys.monitoring) | yes | executed calls | S-M | sec 6 |
| kotlin | codeql java pack (kotlin supported by the java extractor) | present | call, type | M | one .ql, the kotlin fixtures are 28 files (`find tests/fixtures -name '*.kt' | wc -l`); no kotlin corpus is cloned |
| kotlin | scip-java (semanticdb-kotlinc) | MISSING (coursier MISSING) | reference graph | M install + Gradle project needed | indexer row exists (scip_ensure.rs:90-93) |
| kotlin | kotlinc `-Xdump-declarations-to` | MISSING | declarations only | S once installed | no edges |
| kotlin | detekt | MISSING | lint findings, no resolution | n/a | not an oracle |
| kotlin | Kotlin Analysis API standalone (K2) | MISSING | full resolve | L (JVM sidecar) | the ts_checker.mjs shape on a JVM |
| go (checker tier, not oracle) | go/types via a Go sidecar: `oracle_go/main.go` already walks `TypesInfo.Uses` (ORACLES.REPORT 13.2) | go 1.26.3 | a go semantic tier and tsi semantic rows (`types.Implements` -> tsi.conforms; 929 holds on typescript-go) | M | the sidecar shape exists for ts (node) |
| go | gopls | MISSING | LSP references | M | go/types direct is cheaper and already written |
| prolog | swipl `xref_source/1`, `xref_called/3`, `xref_defined/3` | swipl 10.0.2 | call oracle for 344 `.pl` self files | S | tests/8_rename_prolog.rs already uses a swipl load oracle |
| dl6 | the v6 compiler's own plan (`v6/prolog/compile/out/*.schema.json`, manifest) | swipl | rel-use edges | S | self-oracle |

## 5. Axis 4: held-out corpora

State of the parked lane `lab/extract-heldout-oracles` (worktree `.boop-worktrees/lab/extract-heldout-oracles`, tip `3bfbe9d16`):

| item | value | receipt |
|---|---|---|
| files the lane owns vs origin/main | `plans/extract-eval-2026-08-31/heldout/{POOL.go,POOL.python,POOL.rust,POOL.ts}.tsv, SCORES.tsv, SKIPS.tsv, run.py` + `--indexer` flag in `src/bin/extract.rs`, `scip_ensure.rs`, `tests/scip_indexer_pick.rs` | `git diff --name-only $(git merge-base origin/main HEAD)...HEAD` |
| pools | 204-205 repos each, `gh search repos --stars>=200 --size 5000..200000` | `wc -l POOL.*.tsv` |
| SCORES rows | 22: go 4 (3 heldout + tuning control), python 3, ts 8 (4 repos x 2 tiers), rust 7 | `cat SCORES.tsv` |
| skips | 9: 3 python repos under the 50-file floor, 2 ts + 1 rust with no root marker, 1 ts indexer failure, typescript-go timed out twice before `--indexer` | `cat SKIPS.tsv` |
| blocker from chat_log 20260901.2 | "control never ran" is CLOSED in the lane: commit 3bfbe9d16 "the go tuning control, unblocked by --indexer (35.27 / 80.42)" | `git log --oneline -1` |
| `--indexer` on origin/main | NOT merged: `grep -n indexer src/bin/extract.rs` hits only doc comments at :117, :243, :321; branch `origin/feature/scip-indexer-pick` exists | `git branch -r | grep indexer` |
| `detect()` | returns EVERY indexer whose marker is present (scip_ensure.rs:653-663), so the "first-match" wording in the chat log is the runner's, not detect's | `sed -n 652,663p src/scip_ensure.rs` |

The 22 rows read 8.61 to 41.24 recall. The oracle they score against is
`scip_fn_edge` (run.py:294-311 "Mirrors normalize.py scip_to_call_tsv"), and
`scip_v5_rels.rs:73-117` builds `fn_edges` from EVERY non-definition, non-local
occurrence inside a callable's range: type refs, field refs, consts, all become a
(caller, symbol) pair. `SCIP.REPORT.md` sec 6 already recorded this:
"`scip_fn_edge` is a reference graph, not a call graph: 83,361 ts rows vs 59,356 oracle,
17.7 % precision." The tuning-corpus control confirms the protocol mismatch, not an
overfit: TypeScript-5.9 reads 1.61 recall vs scip and 88.20 vs the tsc oracle (RATCHET.tsv);
typescript-go reads 35.27 vs scip and 98.96 vs codeql2. Until the oracle side filters
to callable symbols (SCIP descriptor suffix `().` or `#` for methods, or a join against
`scip_def` kinds), no held-out number can be compared with a ratchet floor.

Second finding in the same table: every ts `checker` row equals its `syntax` row
byte-for-byte (umami 25.92/60.60 twice, vite 15.62/25.77 twice, trpc 8.61/11.07 twice).
The ts checker tier answered nothing on those runs. PR #674 (`tier decline is a
diagnostic`, ARCH.pl:997) landed on 2026-09-03, after the lane's binary was built, so the
rows carry no decline record. The run must be repeated with a post-#674 binary.

Corpus-stats repos (`corpus-stats/REPOS.tsv`): 14 repos, 4 languages, all cloned in
`~/corpora` (gin hugo caddy zod hono express flask requests click ripgrep tokio serde clap
alacritty, plus PyCG and slugify). `STATS.tsv` carries volume only (rows_call, rows_type,
rows_module, wall_s, peak_rss_mb), zero accuracy. `rust-corpora/RESULTS.tsv` covers the
5 rust repos with an ra oracle (recall 32.14 to 99.39 depending on scope).

## 6. Axis 5: trace oracle candidates

Arc B of eval PLAN.md, python first. Executed calls are facts; a trace scores recall of covered edges and never judges uncovered ones (3-bucket, landed in PR #619).

| lang | candidate | on this machine | completeness | caller identity | cost | feasibility receipt |
|---|---|---|---|---|---|---|
| python | `sys.monitoring` (PEP 669) `CALL` + `PY_START` events | python 3.14.6, `sys.monitoring` True | complete for python-level calls; C builtins appear as `CALL` with a builtin callable | code object -> (file, qualname) | S: one script under `python-oracle/trace/`, 119 `main.py` entries (`find suite -name main.py | wc -l`) | the suite categories with written stops (dynamic 0%, builtins 25%, lambdas 35.71%, dicts 36.84%: OPEN-PROBLEMS row 2) are exactly the ones a trace answers |
| python | `sys.setprofile` | yes | complete, slower | frame -> code | S | fallback below 3.12 |
| python | coverage.py / pytest-cov | not installed | lines only, no edges | n/a | n/a | not an edge oracle |
| go | `runtime/trace` | go 1.26.3 | goroutine/GC/syscall events, NO function-call events | n/a | n/a | not an edge oracle |
| go | pprof CPU profile (`-cpuprofile`, `-test.cpuprofile`) | yes | SAMPLED stack pairs; `pprof -edgefraction` prints caller->callee edges with sample counts | frame symbols | S | recall-of-covered only for hot paths; misses anything under ~10 ms |
| go | `go build -cover` / `-covermode=atomic` | yes | function/block coverage, no edges | n/a | n/a | callee set only |
| go | delve `dlv trace 'pkg.*'` | dlv MISSING | complete for matched functions (breakpoint per function entry) | frame | M: install + per-test harness; slow | the only complete go option; linux-free |
| go | `-gcflags=-d=...` instrumentation | none exists | n/a | | | no upstream hook |
| rust | `cargo test` + `-C instrument-coverage` (llvm-cov) | stable toolchain | function coverage, no edges | n/a | n/a | callee set only |
| rust | `-Z instrument-mcount` + uftrace | nightly; uftrace MISSING (linux) | complete | symbol | L | linux only |
| rust | dtrace `pid$target:<bin>::entry` with `ustack(1)` | `/usr/sbin/dtrace` present; SIP blocks pid probes on unsigned/hardened binaries unless the binary is built with `--entitlements` or SIP is reduced | complete when it runs | symbolized frames | M and machine-policy dependent | feasibility gated on SIP state, not measured here |
| rust | `perf record -g` / valgrind callgrind | linux; valgrind MISSING | sampled / complete | | | not on this machine |
| rust | a `#[tracing::instrument]`-style shim | needs source edits to the corpus | complete for instrumented fns | | L | corpus mutation, rejected as an oracle |
| ts/js | node `--cpu-prof` / V8 `Profiler.start` | node 24.15.0 | SAMPLED (1 ms default; `--cpu-prof-interval` lowers it) with a full call tree; every sampled frame carries its parent | function name + url + line | S: `node --cpu-prof` over each test file; `.cpuprofile` JSON `nodes[].children` are caller->callee edges | recall-of-covered only |
| ts/js | jelly `--dynamic` via NodeProf (Graal) | jelly MISSING, Graal not installed | complete | | L | PRIOR-ART pass 2 verified `--compare-callgraphs`; the dynamic half needs GraalVM |
| ts/js | `node --experimental-...`, `--trace-*` | none traces JS calls | | | | no complete tracer in stock node |
| ts/js | instrumentation transform (istanbul-style, inject a call-site hook) | nyc MISSING | complete | | L, bespoke | bespoke build, disallowed before a library census; nyc counts, it does not record edges |

Verdict per language: python S (fund), ts S sampled (fund as recall-of-covered), go M (dlv, or pprof sampled), rust gated on SIP policy (ask the user before pricing).

## 7. Axis 6: mutation battery and `resolution_origin`

| item | state | receipt |
|---|---|---|
| `tests/90_mutation_battery.rs` | BUILT, 14 tests (`grep -c '#\[test\]'`), 4 invariants (duplicate-def, relocation, shadow, origin conservation) | header :1-14 |
| `tests/91_origin_column.rs` | BUILT, 5 tests | |
| `ResolutionOrigin` | 8 variants: SameFile CorpusUnique ModulePlane Checker AliasChain Param Receiver SelfType (types.rs:1650-1664) | `sed -n 1646,1668p src/types.rs` |
| per-origin counts in the ratchet | `call_origins`, `type_origins` maps in `tests/bench/mod.rs:341-346`; PR #619 prints them; NO floor per origin exists (RATCHET.tsv has no origin column) | `grep origin tests/ratchet_recall.rs` = 0 hits |
| OPEN-PROBLEMS row 8 follow-up | "which origins the held-out ratchet should floor separately" is open | OPEN-PROBLEMS.md row 8 |

Data this axis can still yield: an `origin` column in RATCHET.tsv (one row per origin per case) turns every resolver leg into its own ratcheted series; the mutation battery run over the 14 corpus-stats repos (not only the 3 fixture dirs it walks today) gives a per-origin fragility count at corpus scale.

## 8. Axis 7: sprefa as its own corpus

`git ls-files | grep -cE '\.<ext>$'` on origin/main:

| ext | files | arm | oracle candidate | state |
|---|---:|---|---|---|
| rs | 1,432 | RustSource + checker | rust-analyzer (in-tree tier), `CTF.extract-only.REPORT.md` covered 35 engine files | measured once, failure-mode 106 (relative paths drop edges) |
| ts | 1,457 (+54 mjs) | TsSource + tsc | tsc via `oracle_ts.mjs`; `tools/1_madge_oracle.sh` over v6/tsv2 (761 madge edges, deps.rs:13-15) | module family measured; call not |
| md | 2,626 | MarkdownSource | none; the v5 `walk_md_comments` port is the comment-parity oracle (`plans/2026-07-29-comment-node-verdict.md`) | cst only |
| dl6 | 1,104 | DlSource (cst, call, type) | the v6 compiler manifest (`compile/out/manifest.json`) and per-fixture `.schema.json` | none run |
| json | 1,451 | DataSource | codeql yaml/json packs exist; a self-oracle is `serde_json` (the same reader) | none |
| pl | 344 (+1 plt) | PrologSource (cst, type, call, df) | swipl `xref_source/1` + `xref_called/3` | tests use a swipl LOAD oracle only (tests/8_rename_prolog.rs, tests/1_move.rs) |
| py | 274 | PythonSource | PyCG, scip-python | none |
| go | 74 | GoSource | vta | none |
| toml | 56 | DataSource | basic-toml (already a dep, Cargo.toml:157) | none |
| kt | 33 | KotlinSource | none | none |
| dl7 | 14 | none (tree-sitter-dl7 lives in v7/, not a Source) | v7 compiler | no arm |
| yaml/yml | 27 | DataSource | | none |
| html | 12 | ast-grep cst | | none |

Prolog and dl6 arms emit (prolog/_0_source.rs:1-6: "Predicate identities include arity: `name/2` for clauses and `name//2` for DCGs"; dl6/_0_source.rs:3-7: rule heads as `CallKind::Free` defs, body goal atoms as sites, `use` as specifiers, `rel` as `TypeEntityKind::Struct` with columns as fields). Neither has a ratchet row, a corpus run, or an oracle tsv. The prolog corpus throughput test (`tests/34_prolog_corpus_throughput.rs`) is the only corpus-scale receipt.

## 9. Axis 8: the dl7/v7 consumer side

| item | value | receipt |
|---|---|---|
| relations the loader accepts | 35 `tsi_relation/2` rows on origin/main (loader :13-47); 36 on the main tree's dirty `feature/dl7-source-intelligence` branch (adds `tsi.name` at :15) | `grep -c "^tsi_relation("` |
| relations the loader installs as graph structure | 11 `graph_relation/1`: tsi.type product sum edge primitive symbol value value_argument called argument origin (:51-61) | |
| relations accepted as comptime seed facts, not graph | the other 24 (has_type denotes scip_symbol callable parameter input output subtype assignable conforms equivalent, ts.* 5, rust.* 5, go.* 3) go through `comptime_relations/7` (:630-647) as `relation(tsi_relation(Owner,Name), Arity, [])` + seeds; nothing is dropped | `sed -n 630,660p` |
| relations the extractor emits that origin/main's loader reports unknown | `tsi.name` (registry.rs, emitted by ts_checker.mjs, rust_checker_ra.rs, TsiNames syntax tiers) -> `tsi_unknown_relation` diagnostic (:654-658) | |
| relations the loader graphs that nobody emits | `tsi.value`, `tsi.value_argument` (loader :57-58, registry rows, zero emitters) and `tsi.scip_symbol` (accepted, zero emitters) | sec 3 |
| foreign records skipped by name | 56 `foreign_record/1` rows (:133-188): every non-TSI record incl. flow_edge, doc, doc_node, data_doc, cfg_scope, scip_* | `grep -c "^foreign_record("` |
| prelude primitive classes | 27 (`grep -c "^(: "`): ts 10, rust 17; go and python blocks absent; `unit` absent (ARCH.pl:1000 `tsi_primitive_class_absent(unit)`; :1008 go classes absent) | `v7/prelude/5_tsi_primitives.dl7` |
| dispatched owner | `dl7_tsi_render_takeover` (ARCH.pl:1009, brief `plans/2026-09-03-dl7-tsi-render-takeover.BRIEF.md`, U4 = Go primitive block) | do not double-own |

Data the consumer side can still take: python primitive classes (str int float bool bytes None complex, emitted by `_1_type_edges.rs` `tsi.primitive`) have no prelude block and are not in the dispatched brief's U4 (Go only); every `flow_edge`, `doc`, `doc_node`, `data_doc`, `cfg_scope` row is a foreign record the v7 side never loads.

## 10. Ranking

Score = (new facts gained) / (cost). New facts measured as a count or a measure id the receipt command prints. Cost S = one file or one script, no new dep; M = a new module of the size of an existing twin; L = a new language arm or a JVM/nightly toolchain.

### 10.1 Every row

| # | axis | row | new facts | cost | ratio |
|---|---|---|---|---|---|
| 1 | 4 | heldout oracle: filter `scip_fn_edge` callees to callable symbols, rerun 22 rows + controls with a post-#674 binary, merge the lane's `--indexer` | 22 SCORES rows become comparable with 18 ratchet floors; ts checker rows stop duplicating syntax | S | highest |
| 2 | 2 | python + kotlin module plane (`resolved_import`) from the existing `py_module_specifiers` / `kt_module_specifiers` | resolved_import rows for 274 py + 33 kt self files; cross-file call resolution for python (heldout python rows 11.71 to 18.34 recall today) | M | high |
| 3 | 2 | CFG role tables for python, prolog, dl6 (`cfg.rs:123-126`) | cfg rows: 0 today on `probe_graph.py` (611 cst rows) | S | high |
| 4 | 2 | kotlin tsi syntax rows (twin of go_type_edges.rs 666 lines / python _1_type_edges.rs 861 lines) | 13 to 15 tsi relations for .kt; kotlin is the one full arm with zero tsi rows | M | high |
| 5 | 3 | SCIP records decoded but never read (ORACLES.REPORT sec 8): `scip_relationship.is_implementation` -> tsi.conforms, `symbol_roles` write/read, `enclosing_symbol`, `scip_signature`, `scip_diagnostic` | implementation edges for every scip language incl. kotlin/java/cpp with no per-language code; go `implements` oracle has 929 holding pairs to score against | S-M | high |
| 6 | 5 | python trace oracle under `sys.monitoring` over 119 PyCG mains | a dynamic oracle for the 4 categories at 0 to 37% static recall | S-M | high |
| 7 | 1 | markdown doc rows for links and fence languages (35 `inline_link`, 26 `fenced_code_block` on README.md already in the cst) | md -> path edges over 2,626 self md files; fence language -> nested extraction seed | S | high |
| 8 | 3 | go checker tier via a go/types sidecar (oracle_go/main.go:258-400 is the walk) | go semantic tsi rows (subtype/conforms via types.Implements), go checker call/type tier rows; today go has no checker tier | M | medium-high |
| 9 | 7 | prolog call oracle via swipl xref over 344 .pl files + a ratchet row | first accuracy number for the prolog arm | S | medium |
| 10 | 3 | ts type oracle from tsc (extend oracle_ts.mjs with the go writeTypeEdges shape) | a `ts5.type.{syntax,checker}.oracle-typedecl` measure id | S-M | medium |
| 11 | 6 | origin column in RATCHET.tsv (per-origin floors) | 8 origins x 18 cases series | S | medium |
| 12 | 4 | Arc A proper: PyCG on flask/click/requests, vta on gin/hugo/caddy, tsc on zod/hono/express (all cloned in ~/corpora) | held-out rows on 9 repos with the SAME oracle tools as the floors | M | medium |
| 13 | 3 | CodeQL dataflow oracle for go/ts/python/rust `flow_edge` | first oracle for the flow family | M-L | medium |
| 14 | 8 | python primitive prelude block + `tsi.value`/`tsi.value_argument` emitters or registry removal | closes 3 zero-emitter rows and the python class-absent diagnostics | S | medium (needs user: lang design) |
| 15 | 1 | XML: tree-sitter-xml 0.7.0 dep + DataSource element rows; HTML element rows on the data plane | 12 html self files; xml 0 | M | low |
| 16 | 1 | Java arm over tree-sitter-java (codeql java pack + scip-java row exist) | a new language with two oracles | L | low-medium |
| 17 | 3 | kotlin oracle via codeql java pack | first kotlin accuracy row; needs a kotlin corpus clone | M | low-medium |
| 18 | 5 | ts sampled trace (`node --cpu-prof`) | recall-of-covered on ts corpora | S | low-medium |
| 19 | 5 | go trace via dlv | complete go dynamic oracle | M + install | low |
| 20 | 5 | rust trace via dtrace | gated on SIP | M, needs user | low |
| 21 | 2 | docs rows for prolog (`%!` pldoc) and dl6 comments | doc rows on 344 + 1,104 files | S | low |
| 22 | 2 | dl6 df plane (dl6 has cst/type/call only) | flow for dl6 | M | low |
| 23 | 1 | C/C++/Swift/C#/Ruby/PHP arms | new languages | L each | low |

### 10.2 Top 8, dispatchable one-lane arcs

| arc | new facts (count or measure id) | files owned | receipt command | cost | needs-user |
|---|---|---|---|---|---|
| A. heldout oracle repair + lane landing | 22 rows of `plans/extract-eval-2026-08-31/heldout/SCORES.tsv` re-scored against callable-only scip callees; measure ids `{go,python,ts,rust}.call.{syntax,checker}.scip`; tuning controls comparable to `RATCHET.tsv` | `plans/extract-eval-2026-08-31/heldout/{run.py,SCORES.tsv,SKIPS.tsv,REPORT.md}`; `src/bin/extract.rs`, `src/scip_ensure.rs`, `tests/scip_indexer_pick.rs` (the `--indexer` flag from `origin/feature/scip-indexer-pick`) | `python3 plans/extract-eval-2026-08-31/heldout/run.py tuning --lang go` prints `go.call.syntax.scip` within 10 pt of `go.call.syntax.codeql2` (98.96); every ts checker row differs from its syntax row or carries a `tier.tsc` decline record | S: run.py:294-311 filter + a rerun; the flag is already written in the lane branch | no |
| B. python + kotlin module plane | `resolved_import` rows > 0 on `~/corpora/flask` (0 today: `STATS.tsv` rows_module = 0 for flask/click/requests) and on `tests/fixtures/kotlin_modules` | new `src/lang/python/_2_modules.rs`, new `src/lang/kotlin_modules.rs`, `src/project.rs:1066-1080` and `:1624-1665`, new `tests/118_python_modules.rs`, `tests/119_kotlin_modules.rs` | `extract --resolve --family call $(find ~/corpora/flask/src -name '*.py' | sed "s|^|$PWD/|") \| grep -c '"record":"resolved_import"'` > 0; heldout `python.call.syntax.scip` recall moves on redash/serena/yolov5 | M: go_modules.rs is the twin | no |
| C. CFG roles for python, prolog, dl6 | `"family":"cfg"` rows on `tests/fixtures/tsi/probe_graph.py`: 0 -> N; same on `prolog/corpus_2_meta_use.pl`, `dl6/2_callee.dl6` | `src/cfg.rs` (PYTHON_ROLES, PROLOG_ROLES, DL6_ROLES), `tests/17_cfg_first_plane.rs` | `extract --family cfg <file> \| grep -c '"family":"cfg"'` per file, 3 files, each > 0 | S: a `kind_role` table per language, the builder is generic (cfg.rs:1-2) | no |
| D. kotlin tsi syntax rows | tsi relations on a `tests/fixtures/tsi/probe_graph.kt`: 0 -> 13+ (`tsi.type product sum callable parameter input output argument called has_type denotes primitive symbol`) | new `src/lang/kotlin_type_edges.rs`, hook in `src/lang/kotlin.rs`, new fixture `tests/fixtures/tsi/probe_graph.kt`, new `tests/120_kotlin_syntax_graph.rs`; v7 prelude kotlin primitive block stays with the dl7 lane | `extract --witness --resolve --family type <abs>/probe_graph.kt \| grep -oE '"relation":"tsi\.[a-z_]+"' \| sort -u \| wc -l` >= 13; `tests/100_tsi_intersection.rs` gains a kotlin leg | M: go_type_edges.rs (666 lines, 12 tests) is the twin | no |
| E. SCIP unread records into the informed leg | `tsi.conforms` rows from `scip_relationship.is_implementation` on the ts5 and go indexes; scored against `go.oracle.type.tsv` kind=implements (929 pairs, 1.2% recall today per ORACLES.REPORT 13.2); `symbol_roles` write/read bit on `scip_occurrence` | `src/scip_rows.rs`, `src/scip_v5_rels.rs`, `src/project.rs` (scip informed leg), `tests/fixtures/scip_relationship/`, a new `tests/121_scip_relationship_conforms.rs` | `extract --resolve --scip-index <ts5 index> --family type <files> \| grep -c '"relation":"tsi.conforms"'` > 0; `go` implements recall printed by `bench.py` against `go.oracle.type.kinds.tsv` | S-M: records are decoded (22 kinds), only the read is missing | no |
| F. python trace oracle | `python-oracle/trace/TRACE.tsv` rows for 119 mains; per-category recall-of-covered for dynamic, builtins, lambdas, dicts, generators, lists | new `plans/extract-bench-2026-08-29/python-oracle/trace/{run.py,TRACE.tsv,SCORES.tsv}`; `pycg_score.py` gains a `--oracle trace` switch | `python3 python-oracle/trace/run.py` exits 0 and prints 119 case rows; `python3 pycg_score.py --oracle trace` prints a per-category table with 3 buckets | S-M: `sys.monitoring` is stdlib on 3.14.6 | no |
| G. markdown link and fence rows | `doc_node` kinds `link` (target text) and `code_block` with `name` = fence language: on `README.md` 35 links, 26 fences (cst counts today) | `src/lang/markdown/_0_source.rs`, `src/types.rs` `DocNodeKind` (+Link), `src/wire.rs` doc_node flatten, `tests/11_markdown.rs`, `tests/fixtures/markdown/` | `extract --family type <abs>/README.md \| grep -c '"kind":"link"'` = 35; `\| grep -c '"kind":"code_block"'` = 26 | S: one grammar, one projection (markdown/_0_source.rs is 294 lines) | no |
| H. go checker tier via go/types sidecar | `tier.go-types` run row; go checker rows for call and type; `go.call.checker.codeql2` and `go.type.checker.oracle-typedecl` measure ids (none exist: RATCHET.tsv has no go checker row) | new `src/lang/go_checker.rs` (the `ts_checker.rs` shape, subprocess at :411), new `tools/go_checker/main.go` (from `oracle_go/main.go:258-400`), `src/project.rs` (a `go_checker` root beside `rust_checker`/`ts_checker` at :112-117), `tests/bench/mod.rs` two `Case` rows, `tests/122_go_checker_tier.rs` | `extract --resolve --go-checker <root> --family call,type <files>` emits `resolution_origin:"checker"` rows > 0; `RATCHET.tsv` gains 2 go checker rows | M: the sidecar walk exists, the seam exists | no, the tier shape was decided for ts and rust |

Disjoint ownership check: A owns heldout/ + the indexer flag files; B owns python/_2_modules.rs, kotlin_modules.rs, project.rs:1066-1080 and :1624-1665; C owns cfg.rs; D owns kotlin_type_edges.rs + kotlin.rs; E owns scip_rows.rs, scip_v5_rels.rs and the scip informed leg of project.rs; F owns python-oracle/trace; G owns markdown/; H owns go_checker.rs, tools/go_checker, project.rs:112-117 and :340-370. B, E and H all touch `src/project.rs` on different line ranges; run them serially or give H the file and let B and E post their project.rs hunks as follow-up PRs.

## 11. Three surprises

1. The held-out "overfit" numbers measure a protocol mismatch, not overfitting. `scip_v5_rels.rs:73-117` turns every non-definition occurrence inside a callable into a `scip_fn_edge` row; `SCIP.REPORT.md` sec 6 already says "`scip_fn_edge` is a reference graph, not a call graph"; the heldout `run.py:294-311` uses exactly that record as the oracle. The tuning control in `heldout/SCORES.tsv` reads 1.61 (TypeScript-5.9) and 35.27 (typescript-go) against scip, where `RATCHET.tsv` reads 88.20 and 98.96 against tsc and codeql.
2. Three registered relations have no emitter: `tsi.value`, `tsi.value_argument`, `tsi.scip_symbol` (`src/tsi/registry.rs`; `grep -rl` over `src/lang` and `src/tsi/semantic.rs` = 0 files). The v7 loader graphs two of them (`0c_extract_loader.pl:57-58`). Conversely the extractor's `tsi.name` (PR #689 era, ARCH.pl:1007) is an unknown relation to origin/main's loader (35 rows, `:654-658` diagnostic); only the main tree's dirty branch lists it.
3. `ARCH.pl:811` `doc_format_extraction` is `unbuilt` and the spelunk plan (`plans/2026-07-30-sprefa-extract-spelunk.md:269-293`) says "the roster contains no Markdown, HTML, XML, TOML, or YAML Source", yet `lang/mod.rs:87-100` carries `MarkdownSource` (`.md .markdown`, `_0_source.rs:112`) and `DataSource` (json jsonl ndjson yaml yml toml, `data/_0_source.rs:24-28`), and `sample.html` yields 51 cst rows. Only XML is absent from the crate (`grep -c tree-sitter-xml Cargo.lock` = 0).

## 12. Stale claims found

| where | claim | code |
|---|---|---|
| CLAUDE.md "Open, needing the user" | "`source_for` still returns `None` for `.md`" | `MarkdownSource` is 4th in `sources()` (mod.rs:91); `matches` at markdown/_0_source.rs:112 |
| ARCH.pl:811 | `doc_format_extraction` unbuilt, six formats | 5 of 6 have a Source or cst; xml missing |
| chat_log 20260901.2 | "`detect()` is first-match-wins" | `detect()` returns every matching indexer (scip_ensure.rs:653-663); the single-indexer pick happened downstream and the lane added `--indexer` |
| chat_log 20260901.2 | "control never ran" | lane commit 3bfbe9d16 ran it (35.27 / 80.42) |
| eval PLAN.md sec 6 | "no closure program consumes `flow_edge`" | still true: `v6/dl/fixtures/flagship-flow.dl6:143-145` derives its own `flow_edge` rel from `df_direct`, it does not read the extractor's `flow_edge` record; the v7 loader lists `flow_edge` as a foreign record (:152) |
| spelunk plan sec 5 | no Markdown/HTML/XML/TOML/YAML Source | see surprise 3 |
