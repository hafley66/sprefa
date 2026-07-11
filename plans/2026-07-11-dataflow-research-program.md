# Dataflow × SCIP research program (final-hamon brain dump, 2026-07-11)

Companion to plans/2026-07-11-scip-atlas.md ("Dataflow frontier" addendum has
the blindspot list). This is EVERY idea, organized as plannable sessions.
Recurring theme: datalog is the native home of these analyses (Doop lineage) —
most sessions are .dl programs over existing rels, not engine work. Semi-naive
made them affordable; the oracle-parity culture extends to flow via benchmarks.

## Tier 1 — pure .dl over existing rels (no engine change)

### S-A. Doop-in-dl spike: Andersen points-to (sol, 1 session)
`std/points-to.dl`: alloc sites = `new` df_nodes, assigns = df_edge, stores =
df_field, loads = member nodes. ~6 recursive rules -> `points_to(var, site)`,
`alias(a, b)`. Deliverables: the rules, row counts + tick cost on this repo
(semi-naive stress test), and 3 alias-bug fixtures. Unlocks escape analysis
(`escapes(site)` = reaches return/field/global) in +2 rules.
<!-- todo(feature): std/points-to.dl Andersen spike -->

### S-B. CHA -> VTA dispatch (terra, 1 session, after S-A)
`std/dispatch.dl`: interface-typed call + `scip_impl` -> edges to ALL
implementors (sound CHA); refine by points-to (only allocated implementors) =
VTA. Measure against the otel-rust trait-call bare bucket (14% ceiling) —
this is the direct attack on the biggest resolution hole.
<!-- todo(feature): std/dispatch.dl CHA/VTA -->

### S-C. Backward slicing + flowmark integration (terra, 1 session)
`slice_back(sink_node, node)` = closure over reversed flow_edge; hover shows
"reaches <sink> via N hops"; panel layer per slice. The "why does this value
reach here" answer as a query. Rides existing hover_note/flowmark plumbing.

### S-D. assignable(a, b) subtyping closure (luna, half session)
scip_impl + type_link generic/impl edges -> transitive assignability; then
flow edges gain a type-compatibility filter rail (flag flow_edge pairs whose
endpoint types can't unify = lift bugs made visible).

## Tier 2 — SCIP fields we ingest but don't exploit (importer-adjacent)

### S-E. Occurrence roles -> def-use chains + df-hole rail (terra, 1 session)
scip_occurrence.role distinguishes Write/Read (+ Import/Generated/Test
flags). Build `var_write`/`var_read`; join = index-level def-use with zero
lift dependence. Rail: a Read with no lifted df path from its nearest Write =
a df blindspot, enumerated per language — the coverage table the friction
inventory wants, GENERATED. Bonus: Test/Generated flags = taint noise filters
(exclude test fixtures from findings).
<!-- todo(feature): var_write/var_read + df-hole rail from occurrence roles -->

### S-F. enclosing_symbol attribution (luna-terra, 1 session)
Occurrences carry enclosing_range/enclosing_symbol; we attribute call sites
by predecessor-search heuristic today (oracle comment admits it). Ingest the
field, replace the heuristic, re-run parity — expect wrong-bucket shrink.

### S-G. Signature documentation -> compiler-typed params (terra, 1 session)
SignatureDocumentation carries the compiler's typed signature per symbol.
Parse -> `scip_sig(sym, slot, type_sym)`; compare against our syntactic
type_sig (drift rail = our arrow errors made visible); use for typed flow
pruning at call boundaries.

### S-H. scip_impl repo column + scip_typedef (terra, already ledgered)
The atlas gap-4 residual. Prereq for S-B on multi-repo corpora.

### S-I. External-symbol boundary packs (luna data + terra harness)
SCIP marks external symbols (other packages). Key `flow_summary`/
`flow_sanitizer` fact packs by scip PACKAGE symbol (versioned!) instead of
bare name: std/flow-tokio.dl, flow-express.dl... — ecosystem models that
survive renames and pin to versions. The flow-collections.dl precedent
generalized and made version-aware.

## Tier 3 — lift/engine extensions (bigger, sequenced)

### S-J. cfg_edge lift (sol design + terra impl per language)
One new rel: intra-fn control-flow edges from the same one-parse walk.
Unlocks in dl: reaching definitions, liveness, kill (flow-sensitivity — the
over-taint fix), dominance, and path-condition guards later. Start Rust or
TS; the TypeLang seam means each language is an independent session.
<!-- todo(feature): cfg_edge lift rel, one language first -->

### S-K. SSA-lite kill via write versioning (terra, needs S-J)
Version df var nodes by dominating write -> reassignment kills stale flow.
The single biggest taint precision win after positional args.

### S-L. Compositional summaries over SCC condensation (sol research)
Bottom-up per-fn param->ret summary rels over the call-graph condensation
(scc machinery exists) = context-insensitive compositional flow that scales
to org-size corpora; callers join summaries instead of walking callee bodies.
The "callee param->ret merges callers" residual gets its principled fix here.

### S-M. Demand-driven taint (sol research, magic-sets)
Only derive flow reachable from marked sources/sinks (demand transformation
over the flow rules). Interactive-query latency for taint on big corpora;
pairs with the closure-query guard philosophy.

### S-N. TS class-method df hole (terra, JUST FIX IT)
The 3x-sighted zero-df-nodes hole. Not research — a lift bug with a ledger
trail. Do before any TS flow measurement.
<!-- todo(bug): TS class-method df lift hole -->

## Tier 4 — measurement culture (the honest layer)

### S-O. Taint benchmark oracle (sol, 1 session)
Juliet / OWASP-benchmark style labeled vuln corpora as a parity harness for
taint.dl — precision/recall per weakness class, the RA-oracle discipline
extended to flow. Every Tier 1-3 session re-runs it; no flow claim without a
scored arm. Fixture subsets per language under tests/fixtures/.
<!-- todo(feature): taint parity oracle on labeled benchmark corpora -->

### S-P. Two-rev taint diff gate (terra, after D5 rev twins)
diff_pair + flow twins -> "this PR newly connects source X to sink Y" as a
--check rail; the taint REGRESSION gate (findings delta, not findings count —
adoptable on brownfield corpora immediately).

## Suggested order

S-N (fix the hole) -> S-A (points-to spike, also stress-tests semi-naive) ->
S-E (roles def-use + hole rail = generated coverage table) -> S-O (benchmark
oracle BEFORE more precision work, so wins are scored) -> S-B (CHA/VTA) ->
S-F/S-G/S-H (index exploitation batch) -> S-J/S-K (flow-sensitivity arc) ->
S-C/S-D/S-I sprinkled as gravy -> S-L/S-M (research, after the measured base).

## Open-tech landscape beyond SCIP (what else exists, what to steal)

- **Zoekt** (Google->Sourcegraph, OSS Go): trigram-shard code SEARCH index —
  fast regex/literal over huge corpora. TEXT tier, not semantic: fits as a
  candidate-generation accelerator under scan/match (the ref-spine's deferred
  FTS5-trigram idea is a mini-zoekt). Steal the shape, or shell to it for
  org-scale grep.
- **LSIF**: SCIP's predecessor (graph JSON, heavier); scip CLI converts. No
  new value over scip — skip.
- **Kythe** (Google, OSS): richer semantic graph schema (xrefs + semantic
  node/edge kinds). Bazel-centric extractors make it costly; worth reading
  the SCHEMA for edge-kind vocabulary, not adopting.
- **CodeQL**: license-restricted engine, but **Models-as-Data (MaD)** — their
  YAML format for source/sink/summary models — is an open spec and maps 1:1
  onto our flow_summary/flow_sanitizer facts. ADOPT the format: instant
  interop with their published ecosystem model packs.
  <!-- todo(feature): MaD-format importer -> flow_summary/flow_sanitizer facts -->
- **Joern / CPG (Code Property Graph)**: OSS; AST+CFG+PDG in one graph, spec
  is open, exports protobuf/graphml. Two uses: (a) the PDG design informs our
  cfg_edge/S-K shape; (b) a CPG IMPORTER = compiler-adjacent facts for
  languages we haven't lifted (C/C++!).
- **SARIF**: the findings interchange standard — emit taint/rail findings as
  SARIF so GitHub code scanning / editors render them natively. Cheap, big
  adoption surface.
  <!-- todo(feature): --format=sarif for check findings -->
- **Semgrep OSS rules**: pattern-based taint rules (sources/sinks as
  patterns); a translator to sg + flow facts inherits their rule corpus.
- **purl / SBOM**: package-URL identity for the sym_pkg / pin-skew federation
  (scip symbols already carry package+version — purl is the interop spelling).
- **Compiler-IR tier (the real next level)**: per-language IR dumps as an
  OPTIONAL third tier above scip, exactly as scip sits above syntactic:
  - Rust: **stable MIR** (rustc stable_mir API / charon) — post-macro,
    post-desugar CFG+locals; kills the macro blindspot outright.
  - Go: **golang.org/x/tools/go/ssa** — real SSA from a 100-line Go dumper.
  - TS: tsc checker API for resolved types + control flow graph.
  - JVM: bytecode via ASM (Kotlin/Java both).
  Shape: each dumps facts (cfg/def-use/alloc) into the same rels the lift
  emits, keyed source=ir vs source=lift — the resolution_source column idea
  generalized to flow. Diet mode stays zero-setup; IR mode is compiler truth.
  <!-- todo(feature): IR fact tier — stable MIR / Go SSA dumpers behind a want -->

## The ladder (how analysis goes next-level from here)

tier 0 text (zoekt/trigram: find candidates fast) ->
tier 1 syntactic lift (zero-setup, always on) ->
tier 2 index (scip: resolution truth, roles, packages) ->
tier 3 IR (MIR/SSA/bytecode: flow truth, post-macro) ->
interop shell (MaD models in, SARIF findings out, purl identity).
Every tier is optional and additive over the same rels; rails and queries
never change spelling as tiers switch on — that's the moat.

## Open-tech verification (researched 2026-07-11)

| technology | license verdict | concrete integration path | gotcha |
|---|---|---|---|
| **CodeQL engine/CLI** | RESTRICTED. CLI/queries repo (`github/codeql`) is MIT-code, but the [published Terms](https://github.com/github/codeql-cli-binaries/blob/main/LICENSE.md) forbid running it on non-OSS/non-GitHub-hosted codebases outside academic research — the *engine* is not usable on our own private corpora. | Do not embed the engine. | commercial GHAS license needed for closed-source use. |
| **CodeQL Models-as-Data (MaD) YAML format** | The *format itself* (extensions/addsTo/pack/extensible/data YAML shape) ships inside the MIT-licensed `github/codeql` repo and in community packs (e.g. [GitHubSecurityLab/CodeQL-Community-Packs](https://github.com/GitHubSecurityLab/CodeQL-Community-Packs), also MIT). It's a data spec, not engine code — parsing/importing it doesn't touch the restricted CLI terms. | Write a small YAML->`flow_summary`/`flow_sanitizer` importer keyed on `(package, symbol, signature)`; MIT community packs are directly ingestible. | model packs vary per-repo; check each pack's own LICENSE before bulk-importing (most GitHub-published ones are MIT, but third-party packs aren't guaranteed). |
| **Semgrep engine (OSS/CE)** | LGPL-2.1, genuinely open. [semgrep.dev/docs/licensing](https://semgrep.dev/docs/licensing), [github.com/semgrep/semgrep/blob/develop/LICENSE](https://github.com/semgrep/semgrep/blob/develop/LICENSE). | Not needed — we'd translate *rule patterns*, not embed the engine. | none for the engine itself. |
| **Semgrep Registry rules (semgrep/semgrep-rules)** | RESTRICTED as of the Dec 2024 change: "Semgrep Rules License v1.0" — internal-business-use only, no resale/redistribution without permission. [InfoQ](https://www.infoq.com/news/2025/02/semgrep-forked-opengrep/), [github.com/semgrep/semgrep-rules/issues/1245](https://github.com/semgrep/semgrep-rules/issues/1245). Do NOT bulk-translate the upstream registry into shipped `.dl` fact packs. | Use **Opengrep** instead (LGPL-2.1 fork, github.com/opengrep) — restores taint/interproc CE features. Its companion **opengrep-rules** and third-party sets like [AikidoSec/opengrep-rules](https://github.com/AikidoSec/opengrep-rules) are MIT. | verify per-rule-set license before translating; "Semgrep-branded" registry entries keep the restrictive license even when mirrored elsewhere. |
| **Juliet Test Suite (NIST/SARD)** | CC0 1.0 / public domain (17 USC 105 — US government work). [samate.nist.gov/SARD/test-suites/112](https://samate.nist.gov/SARD/test-suites/112). Genuinely unencumbered. | Pull C/C++, Java, C# suites from SARD; slice CWE-labeled subsets per language as `tests/fixtures/juliet/<lang>/`. | 28k+ tests — subsetting by CWE class up front keeps the oracle harness fast (S-O). |
| **OWASP Benchmark** | UNVERIFIED exact SPDX id in this pass, but OWASP-hosted and consistently described as free/open for benchmarking use (BSD-3-Clause is OWASP's typical project default) — confirm the LICENSE file at github.com/OWASP-Benchmark/BenchmarkJava before shipping fixtures. | Smaller (2,800 tests) Java-only complement to Juliet for S-O; use for a quick CI-sized oracle arm. | needs the direct LICENSE-file check flagged above — don't assume. |
| **Zoekt** | Apache-2.0. [github.com/sourcegraph/zoekt](https://github.com/sourcegraph/zoekt) (Sourcegraph fork is the active upstream; the old `google/zoekt` is the historical origin). | Usable as a library (`zoekt` Go pkg) OR shelled indexserver/webserver; trigram index format is Zoekt-internal/stable enough for `zoekt-index`+`zoekt` CLI round-trips. Steal-the-shape route (mini-FTS5-trigram) is lower integration cost than embedding Go tooling into a Rust engine. | embedding means a Go dependency in a Rust codebase — likely shell-out via CLI, not linked library, unless we accept cgo/FFI cost. |
| **LSIF** | Open (Microsoft, MIT), but superseded — `scip` CLI converts LSIF->SCIP already. | Skip; no new value confirmed. | none beyond doc's own note. |
| **Kythe** | Apache-2.0 (+ some NCSA-licensed subcomponents). [en.wikipedia.org/wiki/Google_Kythe](https://en.wikipedia.org/wiki/Google_Kythe). Maintenance is thin (Google's US team was laid off/replaced by an India-based team in 2024) — treat as low-velocity, not abandoned. | Read the `.proto` schemas (`kythe.io` protos) for edge-kind vocabulary (childof, ref, defines/binding, overrides) to widen `type_link`/`type_edge` kinds; Bazel-centric extractors make direct indexer reuse costly. | schema-only steal, not a library integration — matches the doc's own call. |
| **Joern / CPG** | Apache-2.0. [github.com/joernio/joern LICENSE](https://github.com/ShiftLeftSecurity/joern/blob/master/LICENSE) (copyright The Joern Project + ShiftLeft Inc, Apache-2.0 terms). | `joern-export` emits `neo4jcsv`/`graphml`/`graphson`/`dot` — a `graphson` or `neo4jcsv` importer is the lowest-friction path to ingest a CPG for a language we haven't lifted (esp. C/C++) into `type_entity`/`type_link`-shaped rels. Protobuf CPG schema also available for a native-language frontend. | Joern itself is a JVM/Scala toolchain — shell out (`joern-parse`+`joern-export`) rather than embed; export formats are graph-shaped, need a mapping layer to our rel columns. |
| **SARIF 2.1.0** | OASIS standard under RF-on-RAND IPR terms — usable as a data format (not copyleft code); the spec PDF/HTML itself isn't a code license concern for a *producer* of SARIF. [docs.oasis-open.org/sarif/sarif/v2.1.0](https://docs.oasis-open.org/sarif/sarif/v2.1.0/sarif-v2.1.0.html). | Minimal valid producer = one `runs[].tool.driver.name` + `results[].message`+`locations[].physicalLocation`; `--format=sarif` on `--check` output is a straightforward serializer over existing diag/rail-finding rows. | version-pin 2.1.0 (current stable); GitHub code-scanning upload has its own extra schema constraints beyond bare-minimum SARIF (check before wiring CI upload). |
| **purl (package-url)** | purl-spec repo license: MIT ([github.com/package-url/purl-spec/blob/main/LICENSE](https://github.com/package-url/purl-spec/blob/main/LICENSE)); spec has also been standardized as ECMA-427. | Adopt the `pkg:type/namespace/name@version` string as the identity spelling for `sym_pkg`/pin-skew federation; scip symbols already carry package+version, so this is a formatting convention, not a library dependency. | ECMA standardization means a canonical parser exists in multiple languages if we ever need one — but the format is simple enough to hand-roll in Rust. |
| **Rust stable MIR** | The project itself (`rust-lang/project-stable-mir`) is dual MIT/Apache-2.0 like the rest of rustc, and it RENAMED: "Stable MIR" -> **`rustc_public`** (rustc_public crate), reframed as a public-but-not-frozen SemVer-ish interface rather than a fully "stable" one. [rust-lang.github.io/project-stable-mir](https://rust-lang.github.io/project-stable-mir/). | Target `rustc_public` (not the old `stable_mir` name) for a post-macro CFG+locals dumper; it rides nightly-adjacent rustc internals via a driver, same shape as Charon. | naming churn means any doc/skill referencing "stable_mir" needs the rename; API is public-but-evolving, not frozen — pin a rustc/rustc_public version pair. |
| **Charon (AeneasVerif)** | MIT/Apache-2.0 dual (typical Rust default; check repo LICENSE directly — not independently re-verified this pass, treat as UNVERIFIED-exact-file but high-confidence given the ecosystem norm). Explicitly labeled "alpha software" with planned breaking API changes. [github.com/AeneasVerif/charon](https://github.com/AeneasVerif/charon). | Emits an LLBC-shaped IR (locals/CFG, post-monomorphization) via a custom rustc driver — closer to "done for you" than hand-rolling a `rustc_public` consumer, at the cost of alpha-stability risk and coupling to Charon's own release cadence. | breaking changes are *planned*, not hypothetical — pin a Charon commit/tag, don't track `main`. |
| **Go SSA (`golang.org/x/tools/go/ssa` + `ssautil`)** | BSD-3-Clause (the whole `golang.org/x/tools` module). [pkg.go.dev/golang.org/x/tools/go/ssa](https://pkg.go.dev/golang.org/x/tools/go/ssa). | Confirmed: `ssautil.Packages` (load via `go/packages` first) is the documented simplest path; `golang.org/x/tools/cmd/ssadump` is a ready-made standalone dumper binary to shell out to instead of hand-writing one. | `ssadump` output is human-readable IR text, not a stable machine schema — a real integration still needs a small wrapper program emitting rows, not raw ssadump stdout parsing. |
| **TypeScript compiler API (checker + control flow)** | The TS repo/compiler is Apache-2.0 (Microsoft). Confirmed split: `getFlowTypeOfReference`/`getTypeAtFlowNode` and the whole flow-node graph are `@internal` — NOT part of the public `typescript` npm package's `.d.ts` surface. Only the checker's public surface (`getTypeAtLocation`, `getSymbolAtLocation`, etc.) is contractually stable. | Any CFG-from-tsc approach means depending on `@internal` APIs (version-fragile, no semver guarantee) or re-deriving control flow ourselves from the AST (what a `cfg_edge` lift already plans to do per-language) — tsc does NOT hand you a public CFG. | this changes S-J's TS session: budget for hand-built CFG extraction, not a tsc-internals shortcut; internal APIs can and do move across TS minor versions. |
| **JVM bytecode (ASM)** | BSD-3-Clause, confirmed. [asm.ow2.io/license.html](https://asm.ow2.io/license.html). | Small (25KB), fast, already what Spring/AspectJ use — safe base for a Kotlin/Java bytecode-tier dumper (post-erasure ground truth) if the source-level Kotlin TypeLang ever needs a bytecode cross-check. | bytecode is post-erasure/post-compile — generics and some Kotlin-specific shapes (data class synthetics, coroutines state machines) show up transformed; not a drop-in source-level CFG. |
| **egglog** | MIT. [crates.io/crates/egglog](https://crates.io/crates/egglog), org `egraphs-good`. Actively maintained (Feb 2026 activity). | Not a direct fit for `.dl`'s SQL-fixpoint model (egglog is equality-saturation + datalog over e-graphs, a different execution shape) — relevant as *prior art* for S-L/S-M research sessions, not an embeddable dependency today. | scope mismatch: egglog's e-graph rewriting targets program-synthesis/optimization, not incremental fact-derivation over a growing corpus. |
| **ascent (Rust datalog crate)** | MIT. [github.com/s-arash/ascent](https://github.com/s-arash/ascent), confirmed via crates.io version history. | Macro-embedded Datalog in Rust — a possible reference implementation for prototyping S-A (points-to) or S-L (SCC summaries) logic in isolation before porting to `.dl` syntax, since it's pure-Rust and requires no new toolchain. | in-memory only, no SQL/SQLite backing — doesn't replace the engine's persistence model, just a scratch-pad for algorithm design. |
| **differential-datalog (DDlog)** | Apache-2.0-era project, but ARCHIVED: repo lives at `vmware-archive/differential-datalog` (VMware's own "-archive" org convention marks it dead). [github.com/vmware-archive/differential-datalog](https://github.com/vmware-archive/differential-datalog). | Do not depend on it going forward — read-only reference for Doop/incremental-datalog design (CLAUDE.md already flags DD machinery as "exorcised"), consistent with this repo's existing stance. | confirms the plan doc's implicit skepticism; no new information changes any session here. |

### Missed alternatives (verified real + open)

| technology | license | one-line value |
|---|---|---|
| **Glean** (Meta, `facebookincubator/Glean`) | BSD. [github.com/facebookincubator/Glean](https://github.com/facebookincubator/Glean) | Datalog-queried (Angle language) fact store over C++/Hack/Python/Haskell/Flow + LSIF/SCIP for Go/Java/Rust/TS — closest philosophical sibling to `dl` itself (typed schema-defined facts, Datalog query surface); worth reading Angle's schema design and its SCIP-ingestion code for import-mapping ideas, separate from our own SCIP importer. |
| **Infer** (Meta, `facebook/infer`) | MIT, "no usage restrictions" confirmed. | Bi-abduction (automatic pre/post-condition inference per-procedure, analyzed in isolation then composed) is directly relevant prior art for **S-L** (compositional summaries over SCC condensation) — Infer's whole design *is* "analyze functions independently, compose results," the same shape S-L wants for flow_summary generation instead of hand-authored MaD packs. |
| **Opengrep** (`opengrep/opengrep` + rule forks) | LGPL-2.1 engine; rule sets vary (e.g. AikidoSec's fork MIT) | The genuinely-open successor to Semgrep's now-restricted registry — supersedes "translate Semgrep OSS rules" as the safe source for a sg/flow-pattern translator (S-I-adjacent). |
| **CodeQL Community Packs** (`GitHubSecurityLab/CodeQL-Community-Packs`) | MIT | A ready MIT-licensed corpus of MaD-format model packs to seed the MaD importer (todo item already in the doc) without touching GitHub's own restricted first-party packs. |


