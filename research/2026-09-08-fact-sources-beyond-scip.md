# Fact sources beyond SCIP: what extract should read to compete with CodeQL, Semgrep and Glean

## Research Metadata

- Research date: 2026-09-08
- Trigger: a same-day field test of `extract` as a code-review instrument on `@hafley66/vitest-playwright` (17 TypeScript files, 1053 lines). Full record: `issues/extract-port-closeout` (Agent Runs, 2026-09-08) and its two attachments.
- Question: for the facts a review needs, is there a better source than SCIP?
- Short answer: SCIP stays as the name-resolution layer (Glean ingests the same format). The facts the review lacked, exception-aware control flow, resource pairing, structured types, come from an AST-lowered CFG (CodeQL's own method) and from per-language compiler IR doors, not from any index format.
- Measurements in §7 were taken on this machine (M-series mac, node 24, typescript 7.0.2, scip-typescript 0.4.0, codeql 2.26.4).

## Table of contents

0. Glossary
1. What the review needed
2. Candidates, one each
3. Matrix
4. Where each competitor's moat lives
5. Recommendation, ordered
6. Uncertainties
7. Measurements

---

## 0. Glossary

Every abbreviation used in this document and in the two review reports, with where it shows up in this repo.

| term | expansion | plain meaning | where it appears here |
|---|---|---|---|
| AST | abstract syntax tree | the parse tree with punctuation and whitespace dropped | ast-grep patterns, `--ast-pattern` |
| CST | concrete syntax tree | the parse tree with every token kept | `--family cst`, tree-sitter output |
| CFG | control flow graph | nodes are statements or expressions, edges say "can run next" | `--family cfg`; gap: no exception edges |
| exception edge / exceptional successor | | a CFG edge from a call or `throw` to the `catch` or `finally` that would receive it | CodeQL JS has them; extract does not |
| post-dominance | | B post-dominates A when every path from A to exit passes through B. "Does this `finally` always run" is a post-dominance question | gap 1 in the field test |
| DFG / dataflow | data flow graph | edges say "this value reaches that use" | `--family df`, `flow_edge` |
| def-use | definition to use | the pair (where a variable is assigned, where it is read) | df family |
| SSA | static single assignment | an IR form where each variable is assigned exactly once, which makes def-use trivial | CodeQL, Go `go/ssa` |
| taint | | dataflow from an untrusted source to a sensitive sink, through sanitizers | Semgrep, CodeQL security queries |
| PDG | program dependence graph | CFG plus dataflow plus control dependence in one graph | Joern's third layer |
| CPG | code property graph | AST + CFG + PDG merged into one graph (Joern) | `issues/cpg-spec-research` |
| IR | intermediate representation | the compiler's own lowered form of the program | MIR, SSA, bytecode |
| MIR | mid-level IR (Rust) | rustc's CFG-shaped IR; `Call` terminators carry an unwind target | Rust door candidate |
| stable MIR / `rustc_public` | | the supported crate for reading MIR from outside rustc | Rust door candidate |
| HIR | high-level IR (Rust) | rust-analyzer's typed, name-resolved tree | `ra_ap_hir` crates |
| unwind edge | | MIR's exception edge: where control goes if a call panics | MIR `UnwindAction` |
| SCIP | Sourcegraph Code Intelligence Protocol | a protobuf file of documents, occurrences (symbol + role + range) and symbol info (docs, relationships) produced by an indexer | `--family scip`, `scip_def`, `scip_ref`, `index.scip` |
| LSIF | Language Server Index Format | SCIP's predecessor, a JSON graph dump of LSP answers; larger and slower | Glean ingests it |
| LSP | Language Server Protocol | the editor-to-server request protocol (go to definition, references, hover) | `plans/2026-06-02-lsp-via-state-research-plan.md` |
| callHierarchy / typeHierarchy | | LSP 3.16 / 3.17 requests: callers and callees of a symbol; supertypes and subtypes | LSP door candidate |
| inlayHint | | LSP 3.17 request; the grey inferred-type labels editors draw | a way to get inferred types from any server |
| hover / `scip_documentation` | | the signature text a server shows on mouse-over; SCIP stores it as a string per symbol | the "inferred `any`" finding came from here |
| occurrence | | one mention of a symbol at a byte range with a role (definition, reference, write, import) | `scip_occurrence` |
| def / ref | definition / reference | where a name is declared; where it is used | `scip_def`, `scip_ref` |
| relationship | | SCIP's typed link between symbols: implementation, type definition, reference | `scip_relationship`, `scip_impl` |
| indexer | | the per-language program that produces SCIP (scip-typescript, scip-go, rust-analyzer) | `scip_ensure.rs`, `issues/extract-scip-indexer-roster` |
| extractor | | CodeQL's word for the same job, producing TRAP instead of SCIP | CodeQL |
| TRAP | | CodeQL's tuple file format that loads into its relational database | CodeQL db-javascript |
| QL | | CodeQL's query language, an object-oriented Datalog | CodeQL |
| Datalog | | a logic query language over relations; rules derive new facts from existing ones | dl6, Soufflé, Doop |
| dl6 | | this repo's Datalog dialect and engine over extract facts | `.dl`, `dl6 run` |
| Angle | | Glean's query language over its schema of predicates | Glean |
| predicate / relation / fact | | Datalog words: a table, a table, a row | dl6, Glean |
| name resolution | | mapping each use of a name to the declaration it means, across files | SCIP's job; diet_scip's weak spot |
| diet_scip | | extract's compiler-free resolution: tree-sitter parse plus name matching across the given files | `--family diet_scip` |
| type checker | | the compiler pass that assigns a type to every expression | tsc, go/types, rustc |
| call graph | | nodes are functions, edges are calls; precision varies with how virtual calls are resolved | `scip_fn_edge`, `resolved_edge` |
| CHA / RTA / VTA | class hierarchy / rapid type / variable type analysis | three call-graph algorithms, cheap to precise, for resolving method calls on interfaces | Go `go/callgraph` |
| tree-sitter | | the incremental parser generator behind extract's per-file parse | `src/lang/**` |
| ast-grep | | pattern matching and rewriting over tree-sitter trees; extract's pattern engine | `issues/extract-astgrep-soopy` |
| oxc | | the Rust JavaScript/TypeScript parser extract uses for TS; a parser, no type checker | `lang/ts.rs` |
| tsgo / TypeScript 7 | | the Go port of the TypeScript compiler, shipped as `typescript@7`; the old JS compiler API is gone, `@typescript/api` is the preview replacement | measured: `typescript@7.0.2`, `createProgram` undefined |
| stack graphs | | GitHub's compiler-free, incremental name resolution built from tree-sitter graph rules | candidate |
| Kythe | | Google's code-indexing schema and indexers (C++, Java, Go, TypeScript) | candidate |
| Glean | | Meta's fact store for code, with Angle; ingests SCIP/LSIF for most languages | competitor |
| Joern | | open-source CPG tool (Scala) with per-language frontends | competitor for CFG/PDG |
| Semgrep | | tree-sitter based pattern and taint scanner; Community Edition is intra-file | competitor |
| CodeQL | | GitHub's extractor + relational database + QL; the reference for CFG and dataflow libraries | competitor |
| GHAS | GitHub Advanced Security | the commercial license CodeQL needs outside open source | licensing row |
| span containment | | "is byte range A inside byte range B"; the join that found the double release | extract-only review, finding 1 |
| phase-1 / phase-2 records | | extract's per-file facts (`node`, `site`, `sig`) vs cross-file facts (`resolved_edge`, `flow_edge`) | `--resolve` drops phase-1 |
| receipt | | a measured, reproducible proof (a passing test, a timed command, a file:line) | house word |
| P0 / P1 / P2 | | crash or leak / wrong result / smell | review severity scale |
| blake3 digest | | the content hash extract stamps on `file` rows | used to pin the review snapshot |
| worktree | | a second checkout of the same git repo in another directory | `.boop-worktrees`, `--scip-facts` requires one |
| ALS | AsyncLocalStorage (node) | per-async-call-chain storage; the seam the anti tests attacked | vitest-playwright bridge |

---

## 1. What the review needed

Six gaps, from the field test. Each is what a reviewer asked and could not get from `extract`.

| # | question asked | fact needed | who had it |
|---|---|---|---|
| 1 | does this `finally` run twice on the normal path | CFG with exception edges, post-dominance | nobody; rebuilt by hand from `try_statement` / `finally_clause` spans |
| 2 | is every `acquire` released on every path | acquire/release pairing over spans | nobody; three findings had this shape |
| 3 | what is the inferred type of `proxy()` | structured type per symbol | SCIP hover string only |
| 4 | does `waitForURL` take a signal in playwright | facts about a dependency, not the package | eyes lane, by reading `node_modules` |
| 5 | which rxjs calls are deprecated | compiler diagnostics | SCIP `scip_diagnostic`, free |
| 6 | which src files no test reaches | file-level import edges | `--scip-deps`, worked |

Gaps 5 and 6 are already answered. Gaps 1 to 4 are the subject.

---

## 2. Candidates, one each

### 2.1 SCIP (current)

- Gives: per-occurrence symbol + role + range; per-symbol documentation string, relationships (implementation, type definition, reference), diagnostics, enclosing symbol.
- Lacks: any control flow, any dataflow, types as structure (hover is a string), facts about anything not handed to the indexer.
- Languages: the indexer roster (scip-typescript, scip-go, rust-analyzer, scip-python, scip-java, scip-clang, scip-ruby, scip-dotnet).
- Measured here: 1.25 s cold for 19 documents including the indexer run; 0.00 s warm; 661 KB index.
- License: Apache 2.
- Verdict: keep as the resolution layer. Glean reads the same format, so Glean is not ahead on input.

### 2.2 CodeQL

- Gives: full AST, CFG with exceptional successors (built from the AST by the extractor, no compiler needed for the CFG itself), SSA, local and global dataflow, taint, types (the JS/TS extractor bundles its own TypeScript compiler, so it is independent of the TS7 API removal), and the largest library of per-language dataflow models.
- Lacks: an open data door. The database is query-only through QL; the CLI is free for open source and research and needs GHAS for commercial use.
- Languages: 10 (C/C++, C#, Go, Java/Kotlin, JS/TS, Python, Ruby, Swift, Rust, GitHub Actions).
- Measured here: `codeql database create --language=javascript-typescript` 3.61 s wall (13.6 s user, 8 threads), 13 MB database.
- Verdict: the target vocabulary. Its JS CFG is proof that exception edges do not need a type checker.

### 2.3 Semgrep

- Gives: pattern matching over a generic AST for 30+ languages; intra-file taint in the Community Edition; cross-file and cross-function only in Pro.
- Lacks: an index, a CFG export, cross-file resolution in the open edition.
- Measured here: not installed.
- Verdict: its open half is the pattern UX, which ast-grep already provides inside extract. Not a fact source.

### 2.4 Glean

- Gives: a schema of predicates, the Angle query language, derived predicates, incremental DB stacking, scale (Meta monorepo).
- Lacks: any CFG or dataflow; it is a store and a query engine over indexer output.
- Input: its own indexers for Hack, Flow, and clang; LSIF or SCIP for TypeScript, Go, Rust, Java, Python.
- Verdict: same input as extract; the competition is dl6 versus Angle, and derived predicates. Not a better source.

### 2.5 Joern (CPG)

- Gives: AST + CFG + PDG per file, exception edges in the CFG, dataflow queries; frontends per language (jssrc2cpg uses a TypeScript-compiler-based astgen, gosrc2cpg, javasrc2cpg, pysrc2cpg, kotlin2cpg, c2cpg, others).
- Lacks: speed (JVM, minutes cold on real repos), a streaming door (its graph lives in its own store).
- License: Apache 2.
- Verdict: take the schema, the edge kinds and the lowering rules for `try`/`finally`; skip the tool. `issues/cpg-spec-research` is already the inventory.

### 2.6 Kythe

- Gives: a graph schema richer than SCIP (`defines`, `ref`, `childof`, `typed`, `param`, `overrides`), compiler-integrated indexers for C++, Java, Go, TypeScript, protobuf.
- Lacks: light tooling (Bazel builds), an alive TypeScript path: its TS indexer sits on the JavaScript compiler API that TypeScript 7 removed.
- Verdict: no.

### 2.7 Stack graphs

- Gives: incremental, per-file, compiler-free name resolution from graph rules written per grammar; partial paths cache per file, so a one-file change re-resolves one file.
- Lacks: types, anything beyond names.
- Languages with rules: JavaScript, TypeScript, Python, Java.
- Status: `github/stack-graphs` was archived read-only on 2025-09-09 ("no longer supported or updated by GitHub"); 1,946 commits, rules for 4 languages, written by hand.
- Verdict: fork for the partial-path idea only. Its shape (per-grammar resolution rules graded against nothing) is the thing extract's ratchet loop already does with an oracle: diet_scip rows are scored against SCIP (`scip_override`), PyCG, go/types and trace oracles per language on the scoreboard.

### 2.8 LSP as a fact door

- Gives: from any language server, `references`, `definition`, `callHierarchy` (callers/callees), `typeHierarchy` (super/sub), `inlayHint` (inferred types as strings), diagnostics, `semanticTokens`.
- Lacks: bulk shape. Every fact is one request; a whole-corpus dump is N round trips. Server warm state is per process.
- Languages: every language with a server.
- Verdict: the coverage door for languages with no SCIP indexer, and the cheapest route to inferred types today. The existing plans `2026-06-02-lsp-via-state-research-plan.md` and `2026-07-10-lsp-thin-client-daemon.md` already sketch the daemon.

### 2.9 Compiler IR doors, one per language

| language | door | gives | cost |
|---|---|---|---|
| Go | `go/packages` + `go/types` + `golang.org/x/tools/go/ssa` + `go/callgraph` | SSA with basic blocks, defer/recover lowering, typed values, CHA/RTA/VTA call graphs | one Go binary; scip-go is already built on `go/packages` |
| Rust | `rustc_public` (stable MIR) | MIR basic blocks, `Call` terminators with unwind targets (real exception edges), fully typed | needs a nightly-compatible driver; rust-analyzer HIR via `ra_ap_*` is the lighter, less precise sibling |
| TypeScript | `@typescript/api` on tsgo | checker access (type at node, signatures) | preview status; `typescript@7.0.2` on this machine has no `createProgram` |
| JVM | bytecode via Soot / SootUp | SSA (Jimple), call graphs | mature, heavy |
| Python | none clean; pyright is a TS program with no export | | LSP door instead |

Verdict: the only origin of unwind edges and types-as-data. Build where ROI is proven: Go first, Rust second, TypeScript when the API stabilizes.

### 2.10 AST-lowered CFG with exception edges (own)

- Gives: for every tree-sitter language at once, a CFG where each call, `throw`, `await`, `yield` inside a `try` block gets successors to the matching `catch` and `finally`; `finally` gets successors to the continuation and to the rethrow path; `return` inside `try` routes through `finally`. From that CFG, dominance and post-dominance by the standard iterative algorithm.
- Lacks: precision on which exception type reaches which `catch`; every call is assumed able to throw.
- Cost: one lowering rule per language for the `try` family; dominance is a few hundred lines once.
- Verdict: this is CodeQL's method for JavaScript and it answers gaps 1 and 2 without a compiler. Highest value per line of the whole list.

---

## 3. Matrix

| need | SCIP | CodeQL | Semgrep | Glean | Joern | Kythe | stack graphs | LSP door | IR door | own CFG |
|---|---|---|---|---|---|---|---|---|---|---|
| cross-file def/ref | yes | yes | pro | yes (SCIP) | yes | yes | yes | yes | yes | no |
| types as structure | string | yes | no | string | yes | edges | no | string | yes | no |
| CFG with exception edges | no | yes | no | no | yes | no | no | no | Go partial, Rust yes | yes |
| post-dominance | no | derivable | no | no | derivable | no | no | no | derivable | yes |
| dataflow / def-use | no | SSA + taint | intra-file | no | PDG | no | no | no | IR level | no |
| dependency corpus | index it | walks it | no | yes | partial | yes | yes | server does | yes | index it |
| languages | 8 | 10 | 30+ | SCIP set + 3 | 10 | 5 | 4 | all servers | 1 each | all grammars |
| streamable facts | yes | no | no | no | no | yes | yes | per request | yes | yes |
| open license | Apache (indexers) | GHAS for commercial | LGPL, pro closed | BSD | Apache | Apache | MIT/Apache | varies | varies | MIT OR Apache-2.0 (sprefa) |
| cold on 17 files | 1.25 s | 3.61 s | n/a | n/a | minutes | n/a | n/a | ms/request | n/a | ms |

---

## 4. Where each competitor's moat lives

| competitor | moat | not the moat |
|---|---|---|
| CodeQL | per-language dataflow and taint libraries (years of models for frameworks), CFG with exceptions | extraction; the JS CFG is AST-lowered |
| Semgrep | rule registry and pattern ergonomics, 30+ languages on one generic AST | analysis depth; Community Edition is intra-file |
| Glean | store scale, incremental stacking, Angle derivations | input; it reads SCIP and LSIF like extract does |

What extract already has that none of them stream: per-file facts as JSONL in 0.09 s, dl6 rules over them, and ast-grep rewrites that drain into soopy.

---

## 5. Recommendation, ordered

| # | work | closes | issue to file |
|---|---|---|---|
| 1 | exception-edge lowering in the `cfg` family for the `try` family of every grammar, plus `cfg_dom` / `cfg_postdom` relations | gaps 1, 2 | `extract-cfg-exception-edges` |
| 2 | `pairing` relation in dl6: call X at span a, call Y inside a `finally` whose `try` does not contain a; ship as a rule, not code | gap 2 | `dl6-acquire-release-pairing` |
| 3 | index `node_modules/<pkg>` through the existing scip-typescript path on demand (`--corpus`), and follow resolved imports into it | gap 4 | `extract-dependency-corpus` |
| 4 | Go IR door on `go/ssa` + `go/callgraph`; emit `cfg_*`, `flow_edge`, `resolved_edge` in the same vocabulary | gap 3 for Go, precise call graph | `extract-go-ssa-door` |
| 5 | Rust door on `rustc_public`; unwind edges land as `cfg_edge kind=unwind` | gap 3 for Rust | `extract-rust-mir-door` |
| 6 | LSP door: `callHierarchy`, `typeHierarchy`, `inlayHint`, diagnostics behind the thin-client daemon | coverage, inferred types | `extract-lsp-door` |
| 7 | default `--scip-cache` under the scratch dir when the root is a clean worktree | papercut | `extract-scip-cache-default` |
| 8 | keep phase-1 records under `--resolve` behind `--with-phase1` | papercut | `extract-resolve-keep-phase1` |

Items 1 and 2 are pure extract + dl6, no new dependency, and would have turned the best finding of the field test into one query.

---

## 6. Uncertainties

- Stack graphs: verified archived 2025-09-09 (fetched 2026-09-08).
- `@typescript/api`: preview status as of typescript 7.0.2; the shape may change.
- CodeQL JS CFG: the exceptional-edge claim comes from its documentation and the shape of its `ControlFlowNode` successors; the exact predicate names were not run here.
- Joern frontends: language list from the project's README; per-frontend exception-edge fidelity not measured.
- Kythe TypeScript indexer status under TypeScript 7: inferred from the API removal, not tested.

---

## 7. Measurements

All on `@hafley66/vitest-playwright` (17 ts files, 1053 lines) on 2026-09-08.

| run | wall | rows or size |
|---|---|---|
| `extract --family scip .` cold, indexer included | 1.25 s | 2965 rows, `documents:19`, 661 KB index |
| same, warm | 0.00 s | `reused:true` |
| `extract --family diet_scip src/*.ts tests/*.ts` | 0.12 s | 694 rows |
| `extract FILE` x17 serial, all four families | 0.09 s | 40645 rows |
| `extract --scip-facts` warm | ~0.1 s | 7885 rows, 1215 `scip_documentation` |
| `codeql database create --language=javascript-typescript --threads=8` | 3.61 s wall, 13.6 s user | 13 MB |

Review outcome on the same snapshot: eyes 34 rows (P0 3), extract-only 18 rows (P0 3), adversarial tests 6 breaks; extract-unique 7 rows, all real, all fixed the same day. Details in `issues/extract-port-closeout`.

---

## 8. CodeQL swing (added 2026-09-08, later the same day)

Queries and fixture: `research/2026-09-08-codeql-review-queries/` (5 `.ql` files, ~90 lines, plus the four pre-fix shapes reconstructed as a fixture).

### 8.1 Stock suite on the fixed package

| run | wall | result |
|---|---|---|
| `codeql database create` (17 ts files) | 3.61 s | 13 MB |
| `codeql database analyze codeql/javascript-queries` (default code-scanning suite) | 11.04 s | 87 rules, 0 findings |

The stock suite is security-shaped (injection, prototype pollution, regex); none of the session's defect shapes is in its vocabulary.

### 8.2 Custom queries

| query | what it asks | fixture (pre-fix shapes) | fixed package |
|---|---|---|---|
| `DuplicatedCleanupInNestedFinally` | same `recv.method()` in the finally of a try and the finally of an enclosing try | 3 rows (`connection.unsubscribe`, `pageH.release`, `ctxH.release`) | 0 after one more fix; the first run flagged `active.delete` / `connection.unsubscribe` still doubled in the refactor |
| `AcquireBeforeTry` | `await acquire()` released in a finally whose try starts only after another call ran | 1 row (`acquire(context$())` with a second acquire between it and the try) | 0 |
| `AcquireNeverReleased` | `await acquire()` with no `release()` in any finally | 1 row (`globalSetup`) | 1 row, same shape: the release is returned as vitest's teardown closure, a false positive by design |
| `RequireInEsm` | `require()` in a file with `import` declarations | 1 row | 0 |
| `ExceptionalEdgeIntoFinally` | receipt: a call inside a try whose `getASuccessor()` lands in the finally | 10 rows | 85 rows |

The last row is the point: CodeQL's JavaScript CFG carries exceptional successors out of the box, so "does this finally run twice on the normal path" is `getParentStmt*()` containment plus `getASuccessor()`; no compiler, the extractor lowers it from the AST. That is exactly what `extract-cfg-exception-edges` asks for.

### 8.3 Cost

| step | cold | warm |
|---|---|---|
| database create, package | 3.61 s | 3.6 s (no incremental path) |
| database create, 4-file fixture | 6.89 s | |
| 5 custom queries, compile + run | 51.1 s | 2.6 s (package), 11.5 s (fixture, after a query edit) |
| stock suite, 87 rules | 11.0 s | |

Against extract: 1.25 s cold for scip, 0.09 s for all per-file families, no compile step for a dl6 rule. CodeQL's first-query latency is the cost of its query compiler; its payoff is that the four review questions were each under 20 lines of QL once the CFG and `getParentStmt*()` exist.

### 8.4 What this changes in §5

Nothing in the order. It sharpens rows 1 and 2: the five `.ql` files and the fixture are the acceptance test for `extract-cfg-exception-edges` and `dl6-acquire-release-pairing`; a dl6 rule set that reproduces the fixture rows (3, 1, 1, 1) and the package rows (0, 0, 1, 0) is done.
