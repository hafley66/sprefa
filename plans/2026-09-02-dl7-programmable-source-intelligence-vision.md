# DL7 programmable source intelligence vision

Status: active implementation driver and boundary map. This document authorizes
no new kernel form.

This file is the authoritative driver for this arc. Every implementation commit
must update the affected task state, architectural description, and verification
receipt here. A code change without its corresponding document change is
incomplete.

## End-goal table of contents

<ol>
  <li><a href="#language-kernel">Language kernel</a>
    <ol>
      <li>Atoms, references, variables, and constants</li>
      <li>Products, sums, binds, calls, and rules</li>
      <li>Positive recursion, stratified negation, and aggregation</li>
      <li><a href="#kernel-change-gate">Kernel change gate</a></li>
    </ol>
  </li>
  <li><a href="#compiler-time">Compiler time</a>
    <ol>
      <li>Pure compiler fixpoint</li>
      <li>Ground external observations
        <ol>
          <li>Filesystem and Git through Soopy</li>
          <li>Source syntax and semantics through sprefa-extract</li>
          <li>HTTP and process adapters</li>
        </ol>
      </li>
      <li>Observation facts re-entering the fixpoint</li>
      <li>Convergence, deduplication, caching, and provenance</li>
    </ol>
  </li>
  <li><a href="#source-intelligence">Source intelligence</a>
    <ol>
      <li>Tree-sitter CST projection</li>
      <li>Native tree-sitter S-expression queries</li>
      <li>Ast-grep patterns</li>
      <li>Ast-grep composed rules
        <ol>
          <li>Pattern, kind, regex, and named matches</li>
          <li>All, any, and negation</li>
          <li>Inside, has, follows, and precedes</li>
          <li>Captures and proposed edits</li>
        </ol>
      </li>
      <li>Module, symbol, type, call, and flow facts</li>
      <li>Syntax matches joined with semantic identity</li>
      <li>Semantic Markdown and generated documents
        <ol>
          <li>Headings, lists, task items, links, code blocks, and source spans</li>
          <li>DL7 document-pattern rules</li>
          <li>Live task facts derived from authored documents</li>
          <li>Generated Markdown with source provenance</li>
          <li>Drift checking and synchronization</li>
        </ol>
      </li>
    </ol>
  </li>
  <li><a href="#type-graph">Common type graph</a>
    <ol>
      <li>Types, symbols, values, applications, and ordered edges</li>
      <li>Products, sums, callables, parameters, inputs, and outputs</li>
      <li>Structural conformance by edge comparison</li>
      <li>Nominal conformance by declared graph reachability</li>
      <li>Implementation witnesses and associated capabilities</li>
      <li>Language adapters
        <ol>
          <li>DL7</li>
          <li>TypeScript</li>
          <li>Rust</li>
          <li>Go</li>
          <li>Kotlin</li>
        </ol>
      </li>
    </ol>
  </li>
  <li><a href="#relational-program">Relational program</a>
    <ol>
      <li>Checked target-neutral Datalog</li>
      <li>Read, write, dependency, polarity, and stratum facts</li>
      <li>DBSP-style incremental execution planning</li>
      <li>SQL and host-language emitter inputs</li>
    </ol>
  </li>
  <li><a href="#output-time">Output time</a>
    <ol>
      <li>Artifact descriptions produced as data</li>
      <li>Filesystem, Git, shell, and network writes staged after convergence</li>
      <li>Conflict checks, content identity, and atomic application</li>
    </ol>
  </li>
  <li><a href="#bootstrap">Bootstrap</a>
    <ol>
      <li>Extract implementation types into the common type graph</li>
      <li>Generate DL7 declarations and adapters</li>
      <li>Compile the generated declarations</li>
      <li>Compare generated and stage-zero schemas</li>
    </ol>
  </li>
  <li><a href="#remaining-tasks-by-section">Remaining tasks by section</a></li>
  <li><a href="#delivery-sequence">Delivery sequence</a></li>
  <li><a href="#verification">Verification</a></li>
</ol>

## Context

DL7 already lowers prefix forms into a checked Datalog program and runs
compiler relations to a fixpoint. Its parser accepts a tree-sitter query literal
as ordinary ground data at
[`v7/src/0_reader/0_parser.pl`](../v7/src/0_reader/0_parser.pl), and the compiler
can load prepared TSI streams through
[`v7/src/2_comptime/0c_extract_loader.pl`](../v7/src/2_comptime/0c_extract_loader.pl).

Sprefa-extract already provides the source-analysis engines:

- raw tree-sitter queries in
  [`v6/sprefa-extract/src/0_query.rs`](../v6/sprefa-extract/src/0_query.rs);
- ast-grep patterns and captures in
  [`v6/sprefa-extract/src/lang/astgrep.rs`](../v6/sprefa-extract/src/lang/astgrep.rs);
- typed ast-grep composition in
  [`v6/sprefa-extract/src/lang/1_ast_rule.rs`](../v6/sprefa-extract/src/lang/1_ast_rule.rs);
- syntax, module, compiler-semantic, SCIP, type, call, dataflow, and TSI facts.

The existing [`examples/gen-plans-index.dl`](../examples/gen-plans-index.dl)
already proves one narrower document loop: Markdown todo comments become live
task facts and regenerate [`PLANS.md`](../PLANS.md). The end goal generalizes
that pattern through the DL7 source-intelligence and output boundaries.

The systems currently meet through JSONL files prepared before DL7 compilation.
The missing path turns a grounded DL7 query value into an external observation,
loads the resulting facts, and resumes the same compiler fixpoint.

```text
CURRENT

DL7 source ──► SWI compiler ◄── prepared TSI JSONL

extract ──► CST / ast-grep / module / semantic JSONL
```

```text
END GOAL

DL7 rules
   │ derive a grounded source-intelligence operation
   ▼
compiler effect boundary
   │ C ABI, local socket, subprocess, or HTTP adapter
   ▼
sprefa-extract + Soopy
   │ syntax, semantic, Git, and filesystem observations
   ▼
DL7 facts
   │
   └──► compiler fixpoint resumes
              │
              ├──► checked relational program
              └──► staged output artifacts
```

## Decisions

1. DL7 retains a small relational kernel. Source intelligence, Git, filesystem,
   HTTP, shell, and emission are libraries or host capabilities around it.
2. External transport does not alter the logical model. C ABI, local sockets,
   subprocess streams, and HTTP carry the same operation and observation data.
3. Tree-sitter queries and ast-grep composed rules remain separate query
   languages behind one source-intelligence capability.
4. Ast-grep supplies structural composition. Tree-sitter supplies exact CST
   selection with its native S-expression query language.
5. Source matches join semantic facts through immutable source identity and
   half-open byte spans. Textual names remain display or fallback data.
6. Read-like compiler effects return observations to the compiler fixpoint.
   Write-like effects remain staged until output time.
7. The common type graph carries language intersections. Language-specific
   facts preserve semantics that do not belong in the common vocabulary.
8. Structural typing is edge-set comparison. Nominal typing is reachability
   through declared identity edges. Declared implementations can require both
   nominal evidence and a structurally valid witness.
9. Rejected for this planning arc: implementation of a new DL7 special form.
10. Rejected for this planning arc: a source-specific schema added without a
    concrete consumer and prior discussion.
11. Authored Markdown can be source data for compiler rules. Generated Markdown
    carries enough provenance to locate and verify the authored source.

## Language kernel

The current kernel vocabulary remains the baseline:

```text
atom / ref / var / const
:
*
+
call
<-
```

Positive recursion, stratified negation, and aggregation remain checked
Datalog properties. External tools enter through relations whose calls are
ground before dispatch. Query engines, transports, caches, and emitters do not
become parser forms merely because the compiler invokes them.

### Kernel change gate

Any proposal that requires one of the following pauses before specification or
implementation:

- a new reader token or top-level form;
- a new evaluator primitive;
- an implicit input, output, return, scope, or phase rule;
- a privileged relation unavailable to userland rules;
- source-specific vocabulary in the common type graph.

<!-- todo(decision): Discuss and approve the smallest effect boundary only after an executable adapter-shaped prototype demonstrates which current relation or value representation is insufficient. -->

## Compiler time

The compiler-time loop has a pure half and an external-observation half:

```text
compiler rows
    │
    ▼
pure least fixpoint
    │ emits grounded external operations
    ▼
effect runner
    │ returns observations carrying operation identity and provenance
    ▼
pure least fixpoint
```

The operation identity must be stable for equal capability, operation, and
arguments. That identity provides deduplication and a cache key. Every returned
observation carries enough provenance to identify its source content, tool,
mode, and run. A compiler round dispatches each newly grounded operation once.

Transport is selected by the runner:

```text
same logical operation
    ├── direct Rust call
    ├── C ABI
    ├── Unix-domain socket
    ├── subprocess JSONL
    └── HTTP
```

<!-- todo(decision): Select the first transport after measuring call overhead, process lifetime, memory ownership, failure reporting, and reuse by the Rust runtime. -->

## Source intelligence

### Host query facade

[`v6/sprefa-extract/src/lang/2_source_query.rs`](../v6/sprefa-extract/src/lang/2_source_query.rs)
provides one `query_source(path, content, query)` library entrypoint. Its input
sum has three arms:

```text
SourceQuery
  ├── TreeSitter(TreeSitterQuery)
  ├── AstPatterns(list(AstPatternQuery))
  └── AstRule(AstRuleRequest)
```

The output sum retains the corresponding existing result type. This boundary
selects and runs an engine. Normalization into shared source occurrence, match,
and capture facts follows the review gate below.

### Tree-sitter path

DL7 preserves the native tree-sitter query as an S-expression value. The host
parses the selected source once, executes the query, and returns match and
capture rows with source identity and spans.

### Ast-grep path

DL7 represents ast-grep's existing typed composition tree as ordinary values:

```text
pattern | kind | regex | matches
all | any | not
inside | has | follows | precedes
```

The host converts those values into the existing `AstRule` model and executes
the ast-grep library. Fixes remain proposals carrying expected content identity
and spans, suitable for Soopy's staging boundary.

### Semantic join

```text
syntax match
  (content, start, end, capture)
             │
             │ join by source occurrence
             ▼
semantic facts
  (symbol, type, module, call target, origin)
```

This supports questions such as "the ast-grep pattern occurs and the captured
callee resolves to a symbol exported by this module" without giving ast-grep
responsibility for module or type semantics.

### Pending shared fact review

The current proposal uses the existing TSI span value
`(content identity, byte start, byte end)` directly as the source-location key:

```text
source.file(Content, Path, Language)
source.query(Query, Engine)
source.match(Match, Query, Span)
source.capture(Match, Position, Label, Span, Text)
source.replacement(Match, Span, Replacement)
```

`Query` is interned from the complete engine-specific query value. `Match` is
interned from the exact engine result. `Position` preserves repeated and ordered
captures. A replacement remains data until output time. The semantic join uses
the same `Span` value directly:

```text
source.capture(Match, Position, Label, Span, Text)
tsi.has_type(Span, Type)
```

This proposal adds no separate occurrence identity. Reifying an occurrence node
would duplicate the content and byte coordinates already carried by `Span`.
Names, arities, identity rules, and direct-span versus reified-occurrence shape
remain pending review.

<!-- todo(decision): Approve the canonical match, capture, and source-occurrence relations before wiring either query engine into DL7. -->

### Live Markdown documents

Markdown enters through the same source-intelligence path as code. Extract
provides structural document facts and source spans. DL7 rules recognize
project-owned patterns and derive task, documentation, cross-reference, or
index facts.

```text
authored Markdown
    │ headings, lists, task items, links, code blocks, spans
    ▼
sprefa-extract facts
    │
    ▼
DL7 document-pattern rules
    │
    ├──► live task and documentation facts
    └──► generated Markdown artifact
             │ source path, content identity, producing rule
             ▼
          drift check or synchronized output
```

The generated document can identify its source with a relative repository
link, a repository URL, or structured provenance rendered into a comment or
header. Selection of that representation and the direction of synchronization
remain discussion decisions.

<!-- todo(decision): Approve generated-Markdown provenance and synchronization policy: source link shape, content identity, generated-file ownership, drift behavior, and whether any reverse synchronization is permitted. -->

## Common type graph

The common graph contains types, symbols, values, applications, parameters,
ordered edges, callable inputs and outputs, and semantic relationships. Foreign
language analyzers emit that common subset plus language-specific facts.

```text
structural conformance
    required contract edges
        anti-join
    source edges

nominal conformance
    declared implementation / extension edges
        transitive closure
    target contract

declared implementation
    nominal evidence
        +
    structural witness validation
```

Go and TypeScript can emit structural relationships where their semantics
permit them. Rust and Kotlin can emit declared implementation evidence. Every
language keeps its own additional relations for ownership, associated types,
type sets, variance, platform nullability, or other semantics outside the
intersection.

<!-- todo(decision): Decide the user-authored DL7 vocabulary for choosing structural, nominal, or declared-plus-witness conformance before adding a checker rule. -->

## Relational program

After compiler convergence, the target-neutral checked program remains the
authority for relational execution:

```text
relations + seeds + rules
    │
    ├── dependency and polarity graph
    ├── SCCs and strata
    ├── read/write/layout planning
    └── target emitters
          ├── SQL
          ├── Rust runtime
          └── other host languages
```

Source intelligence and type computation can produce declarations, rules, and
facts before this freeze. The relational program remains independent from any
single SQL database or host runtime.

## Output time

Compiler rules describe artifacts and mutations as data. After convergence, an
output runner validates expected content identities, checks overlapping edits,
orders writes, and applies them atomically where the selected host supports it.

```text
compiler fixpoint
    │ artifact and mutation descriptions
    ▼
output validation
    │
    ▼
Soopy / filesystem / Git / process / network adapter
```

<!-- todo(decision): Approve the artifact and mutation data model before connecting shell, filesystem, Git, or network writes. -->

## Bootstrap

The bootstrap target uses the same source-intelligence and type-graph path on
the compiler's implementation:

```text
Rust implementation types
    ▼
sprefa-extract semantic type facts
    ▼
common type graph
    ▼
DL7 generation rules
    ▼
generated declarations
    ▼
DL7 compiler
    ▼
stage-zero equivalence comparison
```

The stage-zero schema stays checked in until generated output has deterministic
identity, stable ordering, complete diagnostics, and an equivalence receipt.

<!-- todo(decision): Define the bootstrap equivalence projection and the stage-zero removal criteria before generated declarations become authoritative. -->

## Remaining tasks by section

The labels below distinguish implementation from discussion and optional
language adapters. They add no kernel form or source-specific schema.

1. **Language kernel**
   1. Build one host-side compiler-observation prototype using the existing
      call, value, and relation representations.
   2. Record the first concrete limitation, if the prototype encounters one.
   3. Discuss that limitation before proposing a reader, evaluator, scope,
      phase, or privileged-relation change.

   <!-- todo(feature): Language kernel section: prove one compiler observation through existing calls, values, and relations, then present any concrete representation failure before proposing a kernel change. -->

2. **Compiler time**
   1. Define a host-internal operation envelope for the prototype, outside the
      DL7 surface contract.
   2. Measure direct Rust calls, C ABI calls, Unix-domain sockets, and
      subprocess JSONL for startup cost, repeated-call cost, memory ownership,
      cancellation, and diagnostics.
   3. Select the first transport at the existing decision gate.
   4. Collect newly grounded operations after each pure fixpoint round.
   5. Intern operation identity from capability, operation, and arguments.
   6. Dispatch each operation identity once per compiler generation.
   7. Convert successful results into provenance-carrying observation facts.
   8. Convert failures and timeouts into named compiler diagnostics.
   9. Resume the pure fixpoint until neither rows nor operations grow.
   10. Add cache ownership, invalidation, and tracing after the uncached loop is
       correct.

   <!-- todo(feature): Compiler-time section: implement the grounded-operation loop, stable operation identity, one-dispatch-per-generation, observation injection, convergence, diagnostics, tracing, and measured transport selection. -->

3. **Source intelligence**
   1. **Complete:** Put the existing tree-sitter query runner, ast-grep
      pattern runner, and composed `AstRule` runner behind one Rust host facade.
      Preserve each engine's current result shape in a distinct output variant;
      canonical match facts remain behind task 4's review gate.
   2. **Complete:** Preserve tree-sitter's native S-expression query value
      without translating it into the ast-grep rule algebra.
   3. Design the ordinary DL7 product and sum values corresponding to the
      existing `AstRule` tree, then review their names before implementation.
   4. Approve canonical source-occurrence, match, and capture facts.
   5. Normalize both query engines onto those facts while retaining engine and
      query provenance.
   6. Feed those facts through the compiler-observation loop.
   7. Join one ast-grep capture to one compiler-confirmed symbol, type, and
      module through content identity and byte span.
   8. Carry ast-grep replacement proposals to output time without applying
      them during compiler time.
   9. Add batching so several queries share one source read and parse.
   10. Project Markdown headings, lists, task items, links, code blocks, and
       source spans into stable document facts.
   11. Express project document patterns as ordinary DL7 rules over those
       facts.
   12. Derive live task rows from authored Markdown and compare them with the
       existing `gen-plans-index.dl` behavior.
   13. Render one generated Markdown document from the derived rows.
   14. Carry source path, source content identity, producing rule, and optional
       repository URL into the generated artifact's provenance.
   15. Add a check mode that reports generated-document drift without writing.
   16. Approve ownership and synchronization policy before adding any reverse
       update from generated output to authored Markdown.

   <!-- todo(feature): Source-intelligence section: host both code-query engines, project semantic Markdown facts, approve and emit common occurrence/match/capture facts, join syntax captures to semantic identities, derive live task rows, render provenance-carrying Markdown, stage fixes, and batch source parsing. -->

4. **Common type graph**
   1. Keep the current TSI intersection as the source-neutral ingestion
      boundary.
   2. Decide the DL7 vocabulary selecting structural, nominal, or
      declared-plus-witness conformance.
   3. Add nominal reachability over declared implementation and extension
      edges.
   4. Reuse current structural edge comparison to validate declared witnesses.
   5. Reconcile associated capabilities, generic applications, and variance
      with both conformance algorithms.
   6. Add a Go semantic adapter that emits common TSI rows plus `go.*` facts
      for type sets and embedding.
   7. Add a Kotlin semantic adapter when a concrete Kotlin consumer selects
      the required Kotlin-only facts.
   8. Add cross-language fixtures that compare the shared projection and pin
      intentional asymmetries.

   <!-- todo(feature): Common-type-graph section: approve conformance policy vocabulary, add nominal reachability and witness validation, then add Go and selected Kotlin semantic adapters with cross-language projection receipts. -->

5. **Relational program**
   1. Define target-neutral layout and representation rows without naming a
      database or host language in compiler relations.
   2. Derive read, write, dependency, polarity, SCC, and stratum facts from the
      checked Datalog program.
   3. Define the representation bridge between logical values and stored or
      transported values.
   4. Produce the first DBSP-style relational execution plan as data.
   5. Feed that plan to the SQL and Rust emitters.
   6. Prove that source-intelligence-produced declarations and authored DL7
      declarations reach the same checked program representation.

   <!-- todo(feature): Relational-program section: define target-neutral layout and representation rows, emit the first DBSP-style plan, feed SQL and Rust emitters, and prove equal checked programs for authored and generated declarations. -->

6. **Output time**
   1. Approve canonical artifact, source mutation, process, Git, and network
      intention data.
   2. Separate recoverable observations from externally visible mutations.
   3. Validate expected source content identities before applying edits.
   4. Detect overlapping or contradictory mutations before any write.
   5. Deterministically order independent outputs.
   6. Connect Soopy as the filesystem and Git staging adapter.
   7. Add shell and HTTP adapters with explicit capability policy, limits,
      provenance, stdout, stderr, status, and response facts.
   8. Record applied outputs so retries and compiler restarts can identify
      completed work.
   9. Treat generated Markdown as an ordinary artifact with source provenance,
      deterministic bytes, drift checking, and the approved ownership marker.

   <!-- todo(feature): Output-time section: approve intention data, validate and order mutations, connect Soopy staging, add bounded shell and HTTP adapters, and record applied-output identity. -->

7. **Bootstrap**
   1. Approve the projection used to compare stage-zero and generated schemas.
   2. Extract the Rust implementation types that define the source-analysis and
      TSI protocols.
   3. Generate DL7 declarations from their common type graph.
   4. Compile those generated declarations through the ordinary DL7 compiler.
   5. Compare identities, edges, order, diagnostics, and public relation shapes
      against stage zero.
   6. Keep stage zero authoritative until the equivalence and failure-mode
      receipts are stable.
   7. Decide stage-zero replacement or continued checked-in bootstrap status.

   <!-- todo(feature): Bootstrap section: approve the equivalence projection, generate and compile DL7 declarations from Rust implementation types, compare them with stage zero, and decide the authoritative bootstrap source. -->

8. **Delivery and verification**
   1. Split work at the boundaries above so each slice has one input shape, one
      output shape, and one deterministic receipt.
   2. Run focused tests for each slice and one full V7 battery at each merged
      boundary.
   3. Measure operation dispatch count, parse count, cache behavior, fixpoint
      rounds, wall time, and peak memory.
   4. Preserve the current full-battery baseline before connecting an external
      effect runner.
   5. Update this inventory and `PLANS.md` whenever a slice closes or a decision
      changes its branch.
   6. Update this document in the same commit as every implementation slice.

   <!-- todo(feature): Delivery and verification section: land boundary-sized slices with deterministic receipts, focused tests, full V7 gates, dispatch and parse counts, fixpoint metrics, wall time, and peak memory. -->

## Delivery sequence

1. Freeze this vision as the boundary map.
2. Demonstrate the current ast-grep and tree-sitter engines through one
   host-side interface, retaining their current output shapes.
3. Define and review canonical source occurrence, match, and capture facts.
4. Prototype one grounded compiler observation without changing the parser or
   evaluator kernel.
5. Review the prototype and decide whether the existing call/value machinery
   is sufficient.
6. Connect observations to the compiler fixpoint with deduplication,
   provenance, and a convergence test.
7. Join one syntax match against one semantic module or symbol fact.
8. Define staged artifact data and connect one recoverable output adapter.
9. Add nominal and structural conformance policies after their userland
   vocabulary is approved.
10. Attempt the implementation-schema bootstrap and compare it with stage zero.

## Verification

Each delivery slice must prove one boundary with a deterministic fixture:

1. Equal grounded operations dispatch once.
2. Cached and uncached observations produce equal compiler rows.
3. Tree-sitter and ast-grep matches carry stable content identity and spans.
4. A syntax capture joins the compiler-confirmed symbol and module identity.
5. External failures become named diagnostics with operation provenance.
6. Compiler convergence leaves no undispatched grounded operation.
7. Output descriptions perform no mutation before the output phase.
8. Conflicting edits stop before writes.
9. Structural conformance reports missing edges.
10. Nominal conformance follows only declared identity edges.
11. Generated bootstrap declarations equal the approved stage-zero projection.
12. Equal authored Markdown produces byte-identical generated documents.
13. A changed source document produces a named drift result containing the
    source and generated-document identities.

Current implementation receipts:

- `cargo test --manifest-path v6/sprefa-extract/Cargo.toml --test 30_ast_rule`:
  6 passed. The added test dispatches all three facade arms and checks their
  distinct result variants.
- `cargo test --manifest-path v6/sprefa-extract/Cargo.toml --test 9_query_cli`:
  8 passed. Existing tree-sitter CLI output, predicates, grammars, errors, and
  Git blob reads remain covered.

The current slice adds one focused facade test and changes no full-suite gate.

## Staffing

- Current branch: `feature/dl7-source-intelligence`.
- Baseline vision commit: `3f89ac108`.
- Current slice: host query facade, followed by the match-fact design review.
- Implementation proceeds one reviewed boundary per delivery-sequence item.
