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
5. Source identity has three independent axes: a revision-qualified source
   place, immutable content, and a half-open range in that content. Syntax
   facts use content spans. Semantic facts and mutations use located
   occurrences that pair a source place with a content span.
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

### Source identity and semantic join

The source model keeps location, bytes, range, parser interpretation, and
semantic occurrence as separate values:

```text
Repository + Revision + Path
              │
              ▼
           Source
              │ source_content
              ▼
           Content
              │ [start, end)
              ▼
        ContentSpan
              │
       Source + ContentSpan
              ▼
      LocatedOccurrence
```

Their logical relations are:

```text
source(Source, Repository, Revision, Path)
content(Content, ByteLength)
source_content(Source, Content)
content_span(Span, Content, Start, End)
located(Occurrence, Source, Span)

parse(Parse, Content, Grammar, ParserVersion, Configuration)
syntax_node(Node, Parse, Kind, Span)

source_query(Query, Engine, Specification)
source_match(Match, Query, Parse, Span)
source_capture(Match, Position, Label, Span)

tsi_has_type(Occurrence, Type)
tsi_refers_to(Occurrence, Symbol)
source_replacement(Edit, Occurrence, Replacement, Producer)
```

`ContentSpan` is reusable for work whose answer depends only on bytes, grammar,
parser version, and parser configuration. `LocatedOccurrence` is required for
module resolution and other semantics that can differ when identical bytes
appear at different repository paths. A replacement obtains all of Soopy's
precondition data from the occurrence: `Source` selects the file,
`ContentSpan.Content` is the expected content identity, and the span supplies
the byte range.

The model follows the evidence accumulated by the older implementations:

- V4's `WhereBytes` hashed text, repository, revision, file, and range into one
  located-text identity.
- V5 added repository/path attribution and syntax-kind salting after identical
  content at different paths and distinct syntax nodes at one range collided.
- V6 Extract separated the file-local `Span` from node identity
  `(family, span, kind)`, while its TSI wire promoted spans to
  `(content, start, end)`.
- V6's storage census measured 7,345,805 fact references to 2,073,233 distinct
  spans over 1,048 files. Its selected SQLite layout gave facts a dense
  `file_span_id`, but that surrogate remains an emitter decision.
- Soopy already provides `SourceRef`, `SourceEntry`, `SourceSpan`, and mutations
  guarded by an expected `ContentId`.

Canonical `Content` identity uses BLAKE3 over bytes. Git object identity is an
additive capability fact:

```text
git_blob(Content, Repository, ObjectId)
```

This gives Extract and every non-Git source one content key while retaining the
Git OID required for object reads. Soopy currently represents `GitBlob` and
`Blake3` as distinct `ContentId` variants, so the bridge must emit the BLAKE3
identity and a `git_blob` fact when both are known.

The storage emitter may intern the logical product without exposing its dense
key to compiler rules:

```text
located(Source, content_span(Content, Start, End))
                         │
                         ▼
file_span(file_span_id, rev_file_id, start, end)
rev_file(rev_file_id, Source, Content)
```

Text is derived from `(Content, Start, End)`. Capture rows do not repeat it.
Line and column coordinates are derived presentation data.

```text
syntax match
  (query, parse, content span, capture)
             │
             │ pair with source placement
             ▼
semantic facts
  (located occurrence, symbol, type, module, call target, origin)
```

This supports questions such as "the ast-grep pattern occurs and the captured
callee resolves to a symbol exported by this module" without giving ast-grep
responsibility for module or type semantics.

### Shared fact implementation boundary

The source identity, match, and capture model above is approved for the first
vertical slice. The host envelope carries source and canonical content once,
then stores compact `{start, end}` ranges under each match and capture. The DL7
loader expands each range into the structural
`content_span(Content, Start, End)` value and interns repeats. A later storage
emitter may replace those products with dense references.

The first golden must exercise one real repository source through Soopy, both
query engines through Extract, source-fact loading into DL7, a semantic join,
and a replacement projected back to Soopy's expected-content mutation shape.

[`v7/src/2_comptime/0d_source_fact_loader.pl`](../v7/src/2_comptime/0d_source_fact_loader.pl)
now performs the source-fact loading half. It reads one or more protocol-1 JSON
arrays, expands compact ranges into structural `content_span` and `located`
identities, sorts equal rows from separate query engines into one fact, and
installs fourteen ordinary relations under `module(source_intelligence)`. JSON
objects become sorted typed terms before entering parse and query identities,
so JSON object key order cannot mint a second logical query.

The first useful end-to-end generator extends that foundation:

```text
Rust source
    │ Soopy source + exact content
    ▼
Extract Rust semantic types
    │ common TSI type graph
    ▼
DL7 generation rules
    │ deterministic DL7 declarations
    ▼
owned marker region in a .dl7 file
    │ content-span replacement proposal
    ▼
Soopy check, stage, and apply
```

Only the generated marker region is owned by this pipeline. Text outside that
range is preserved byte for byte. Check mode renders and compares without
writing. Apply mode carries the target's expected content identity through
Soopy so an edit after analysis refuses the replacement. The golden uses a
representative Rust product with nested products, a sum, a generic, a trait,
and an implementation rather than one fixture per construct.

The pure marker and mutation projection now lives in
[`v6/sprefa-extract/src/lang/4_owned_region.rs`](../v6/sprefa-extract/src/lang/4_owned_region.rs).
An owned region is delimited by ordinary DL7 comments:

```text
; sprefa:auto-begin rust-types
generated bytes
; sprefa:auto-end rust-types
```

The region identifier selects exactly one begin and end marker. Rendering adds
one trailing newline when needed, reports whether the body changed, and emits
a Soopy `StageRequest` carrying the analyzed BLAKE3 content identity and exact
body span. Marker bytes and every byte outside the body remain authored data.

The first Rust projection lives in
[`v7/src/3_emit/2_rust_type_emitter.pl`](../v7/src/3_emit/2_rust_type_emitter.pl).
It consumes the mixed JSONL produced directly by `extract --witness --family
type`; the TSI loader now ignores accompanying base `node` and `sig` records
instead of diagnosing them as malformed TSI. The emitted DL7 has this shape:

```text
rust_types
  Mapper -> rust_type_0
  User   -> rust_type_4
  View   -> rust_type_12
  Shape  -> rust_type_19

rust_type_4 = product(id -> rust_type_5, name -> rust_type_7)
rust_type_19 = sum(Circle -> rust_type_20, Square -> rust_type_22)

rust_trait(type)
tsi_callable(type)
tsi_input(callable, position, target)
tsi_output(callable, position, target)
tsi_parameter(parameter, owner, position, variance)
rust_impl(implementation, self, trait)
rust_assoc(owner, name, target)
tsi_conforms(source, target, mode)
```

Wire IDs receive file-local generated names, while source spellings label only
the `rust_types` product. This avoids collisions with authored declarations and
retains graph identity. Syntax TSI currently leaves nested generic applications
and associated types opaque; semantic TSI supplies the additional application
and associated-type rows needed for that later projection.

[`v7/src/3_emit/3_rust_type_region_mainer.pl`](../v7/src/3_emit/3_rust_type_region_mainer.pl)
composes the complete syntax-tier path. It runs Extract once, loads the mixed
TSI stream from memory, renders the graph, and sends the generated bytes to
`extract region` over stdin. The latter command performs check or apply through
the owned-region and Soopy boundaries. No temporary TSI or generated source
file is part of the protocol.

From the repository root:

```text
just -f v7/justfile rust-types-check \
  path/to/types.rs path/to/target.dl7 rust-types

just -f v7/justfile rust-types-apply \
  path/to/types.rs path/to/target.dl7 rust-types
```

Check exits 0 for `current`, 1 for `drift`, and 2 for a malformed stream,
marker, process failure, or source refusal. Apply exits 0 after the Soopy stage
and commit complete. Both paths use the same generated bytes and marker range.

<!-- todo(feature): Extend the completed Rust-syntax-to-owned-DL7-region vertical with semantic generic applications, source-span joins, and compiler-loop observation dispatch. -->

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
   4. **Complete:** Approve canonical source, content, content-span, located
      occurrence, match, and capture facts.
   5. **Complete:** Normalize the tree-sitter, ast-grep pattern, and ast-grep
      composed-rule engines onto one source graph while retaining engine,
      grammar, complete query specification, branch, pattern, match order,
      capture order, and replacement provenance.
   6. **Complete:** Load prepared normalized source envelopes into a shared DL7
      module as deduplicated source, content, span, occurrence, parse, query,
      match, capture, and replacement relations.
   7. Feed freshly returned facts through the compiler-observation loop.
   8. Join one ast-grep capture to one compiler-confirmed symbol, type, and
      module through content identity and byte span.
   9. **Complete:** Carry generated-region replacement proposals to Soopy's
      output-time staging boundary without applying them during compiler time.
   10. Add batching so several queries share one source read and parse.
   11. Project Markdown headings, lists, task items, links, code blocks, and
       source spans into stable document facts.
   12. Express project document patterns as ordinary DL7 rules over those
       facts.
   13. Derive live task rows from authored Markdown and compare them with the
       existing `gen-plans-index.dl` behavior.
   14. Render one generated Markdown document from the derived rows.
   15. Carry source path, source content identity, producing rule, and optional
       repository URL into the generated artifact's provenance.
   16. Add a check mode that reports generated-document drift without writing.
   17. Approve ownership and synchronization policy before adding any reverse
       update from generated output to authored Markdown.
   18. **Syntax vertical complete:** Extract one representative Rust type graph,
       render deterministic compilable DL7 declarations, check one owned
       marker region, refresh it through Soopy, check it as current, and compile
       the resulting target. Semantic generic-application fidelity remains
       open.

   <!-- todo(feature): Source-intelligence section: connect loaded source facts to the compiler observation loop, project semantic Markdown facts, join syntax captures to semantic identities, derive live task rows, render provenance-carrying Markdown, stage fixes, and batch source parsing. -->

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
   3. **Complete for owned regions:** Validate expected source content
      identities before staging generated-region edits.
   4. Detect overlapping or contradictory mutations before any write.
   5. Deterministically order independent outputs.
   6. **Directory-region path complete:** Connect generated directory-file
      regions to Soopy staging. Git worktree routing remains open.
   7. Add shell and HTTP adapters with explicit capability policy, limits,
      provenance, stdout, stderr, status, and response facts.
   8. Record applied outputs so retries and compiler restarts can identify
      completed work.
   9. Treat generated Markdown as an ordinary artifact with source provenance,
      deterministic bytes, drift checking, and the approved ownership marker.

   <!-- todo(feature): Output-time section: approve intention data, validate and order mutations, connect Soopy staging, add bounded shell and HTTP adapters, and record applied-output identity. -->

7. **Bootstrap**
   1. Approve the projection used to compare stage-zero and generated schemas.
   2. **Representative syntax slice complete:** Extract Rust implementation
      types into a TSI stream. Whole-protocol and semantic-checker coverage
      remain open.
   3. **Representative syntax slice complete:** Generate DL7 declarations from
      the accepted common type graph.
   4. **Representative syntax slice complete:** Compile those generated
      declarations through the ordinary DL7 compiler.
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
3. **Complete:** Define and review canonical source occurrence, match, and
   capture facts.
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
14. A Rust type graph renders byte-identical DL7 declarations in two runs.
15. Check mode reports a stale generated marker region without writing.
16. Apply mode preserves every byte outside the marker region and refuses a
    target whose content changed after rendering.

Current implementation receipts:

- `cargo test --manifest-path v6/sprefa-extract/Cargo.toml --test 30_ast_rule`:
  6 passed. The added test dispatches all three facade arms and checks their
  distinct result variants.
- `cargo test --manifest-path v6/sprefa-extract/Cargo.toml --test 9_query_cli`:
  8 passed. Existing tree-sitter CLI output, predicates, grammars, errors, and
  Git blob reads remain covered.
- Source identity review: V4, V5, V6 Extract, the V6 file-span storage census,
  and Soopy's current source and mutation types were compared. The approved
  model separates `Source`, `Content`, `ContentSpan`, `LocatedOccurrence`, and
  `SyntaxNode`; canonical content uses BLAKE3 with Git OIDs retained as
  capability facts.
- `cargo test --manifest-path v6/sprefa-extract/Cargo.toml --test 30_ast_rule
  --no-default-features`: 7 passed in 0.02 s test time and 0.43 s warm command
  wall time. Its representative golden reads one Rust file through Soopy and
  projects native tree-sitter, ast-grep pattern, and composed ast-grep rule
  results into one content-addressed source graph. The graph contains two
  function matches, ordered captures, one structural pattern match, one fix
  proposal, and a stale-content refusal.
- `cargo test --manifest-path v6/sprefa-extract/Cargo.toml --test 9_query_cli
  --features cli`: 8 passed in 1.23 s test time. The command rebuilt the CLI
  feature graph and took 16.88 s wall time; this is compilation rather than
  query execution. The existing query output contract remains unchanged.
- `swipl -q -g "load_test_files([]),run_tests,halt" -t halt
  v7/test/5_source_fact_loader.test.pl`: 1 representative golden passed in
  0.015 s test time and 0.07 s command wall time. Three query engines sharing
  one Rust file load as 31 deduplicated rows: one source, one content, six
  content spans, six located occurrences, four matches, six captures, three
  query identities, and one replacement proposal.
- `cargo test --manifest-path v6/sprefa-extract/Cargo.toml --test
  31_owned_region --no-default-features`: 1 representative mutation golden
  passed. The warm command took 0.65 s wall time and the test body completed in
  0.00 s. It proves deterministic trailing-newline normalization, unchanged
  detection, exact outside-byte preservation, Soopy stage planning, and stale
  content refusal.
- `swipl -q -g "load_test_files([]),run_tests,halt" -t halt
  v7/test/6_rust_type_emitter.test.pl`: 1 representative Rust graph golden
  passed in 0.160 s test time and 0.33 s command wall time. One source contains
  a generic trait, generic product, implementation, recursive product,
  ownership-shaped fields, and a sum. Two renders are byte-identical, equal the
  checked-in DL7 golden, compile through the ordinary V7 compiler, and expose
  the four source root names through one generated product.
- Full V7 battery: 57 passed in 133.49 s wall time. The established slow cases
  remained the generated type-algebra and forwarding fixtures at 32.737 s and
  31.711 s. The source-fact golden took 0.002 s inside the battery; the
  Rust-to-DL7 golden took 2.988 s including an ordinary compiler pass.
- `just -f v7/justfile rust-types-e2e`: passed in 9.46 s warm wall time. One
  process invocation extracted the representative Rust type graph, the Prolog
  emitter rendered it, check mode returned `drift`, apply mode staged and
  committed through Soopy, the second check returned `current`, authored bytes
  around the marker remained exact, and the updated target compiled without
  diagnostics. Its final DL7 compile trace took 7.893 s of the command.

The current slices add one V7 source-fact golden, one Rust owned-region golden,
and one Rust-to-DL7 type graph golden. They change no full-suite gate.

## Staffing

- Current branch: `feature/dl7-source-intelligence`.
- Baseline vision commit: `3f89ac108`.
- Current slice: the Rust syntax type graph reaches a content-guarded generated
  DL7 region. Semantic application rows, syntax-to-symbol joins, and compiler
  observation dispatch are next.
- Implementation proceeds one reviewed boundary per delivery-sequence item.
