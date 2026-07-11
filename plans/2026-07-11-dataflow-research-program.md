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
