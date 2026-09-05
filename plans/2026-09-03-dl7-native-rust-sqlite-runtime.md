# DL7 native Rust and SQLite runtime

## Context

DL6 contained three separate implementation paths that were discussed as one
Rust runtime:

1. `v6/prolog/use_resolve.pl` recursively loaded `use` and `pub use` files,
   canonicalized paths, parsed each file once, spliced their declarations, and
   added module and mount declarations. Nested relation blocks and dotted
   relation paths were flattened before checking by
   `v6/prolog/compile/parse_dl_dcg.pl` and `v6/prolog/0_dot_expand.pl`.
2. `v6/prolog/emit_rust.pl` emitted Rust source containing a `PROGRAM_JSON`
   string. `v6/sprefa-engine-rs/src/run.rs` extracted that string,
   deserialized it, and passed the resulting plan to the generic Rust and
   SQLite engine. The generated Rust contained no compiled tick body.
3. The deleted lab emitter at commit `2d86b1bda` emitted a concrete Rust
   reachability implementation with `FxHashSet`, `FxHashMap`, and an unrolled
   semi-naive delta loop. Its generated `main.rs` remains at
   `v6/labs/exec_shootout/mono/src/main.rs`. This path was benchmarked and then
   deleted in commit `688a72523`; it was never connected to the dynamic loader
   or generalized across DL6 programs.

Commit `beb8e55b3` measured a generated Rust `cdylib` at 2.07 seconds for a
large repaired source file, warm `dlopen` at 0.334 milliseconds, and the first
JSON decode at 3.476 to 5.646 milliseconds. The measured library still exported
the JSON plan. A later corrected shootout receipt in
`chat_log/20260812.3.emit-rust-ir-280-to-285-lane-forensics-claudemd-rewrite.md`
records generated Rust builds at 0.77 to 0.95 seconds cold and 0.27 to 0.46
seconds warm.

DL7 already retains file and nested declaration identity:

- `v7/src/2_comptime/0b_filesystem_grapher.pl` represents project roots,
  directories, and source files as product nodes connected by ordinary `:/4`
  edges.
- `v7/src/2_comptime/0_lowerer.pl` lowers each source file under
  `module(file(CanonicalPath))`. A nested product or sum receives an owner
  identity and its nested binds become `:/4` edges from that owner.
- `v7/src/0_reader/1_expander.pl` rewrites prefix and infix colon spellings to
  the same canonical form.
- `v7/src/3_emit/0_logical_program_reifier.pl` reifies a checked program as
  relations, rules, goals, calls, arguments, dependencies, and strata.
- `v7/src/3_emit/1_artifact_emitter.pl` gives host-Prolog emitters the complete
  compiler view and runs a dependency-sliced second comptime evaluation for
  DL7 emitters over the reified checked program.
- `v7/src/3_emit/1a_dbsp_plan_emitter.pl` currently builds a JSON dictionary in
  Prolog. This is a prototype endpoint rather than the planned DL7-authored
  lowering seam.

The target is generated Rust logic, SQLite-backed relation state, one Rust
source module corresponding to each loaded DL7 source file, and generation
boundary reload. JSON remains available for external tool protocols and debug
artifacts; it does not carry the runtime program between the DL7 compiler and
the Rust/SQLite executor.

## Decisions

1. `:/4` remains the sole binding and ordered-edge relation. Module membership,
   nested declaration ownership, field membership, variant membership, aliases,
   annotations, and filesystem containment all use it.
2. Dots remain opaque identifier content. The module resolver and runtime
   relation identity do not split an atom on dots.
3. DL7 receives no `import` or `export` reader forms in this arc. A source file
   is a module product. Its top-level binds are module edges. A local alias is
   another bind to an existing identity. Visibility, when required, is an edge
   on the reified binding edge.
4. A path is an edge walk. It is retained as a sequence of graph identities and
   labels rather than flattened into an internal name.
5. The checked Datalog program is reified into declared DL7 relations before
   target lowering. DL7 rules derive the DBSP operator graph from those rows.
6. Rust/SQLite lowering is a DL7 program over the DBSP graph and the type graph.
   It is an emitter/lowering package, outside the language kernel.
7. Each loaded `.dl7` file emits one generated `.rs` module. One generated
   project root wires those modules into one content-addressed `cdylib` for a
   compiler generation.
8. The resident host owns the watcher, compilation process, loaded-library
   handles, arrival queue, and SQLite connection. Generated code receives an
   opaque host context plus a versioned function table.
9. Generated Rust contains compiled operator control flow and embedded SQL
   statements. It does not contain a serialized runtime program that a generic
   plan walker interprets.
10. SQLite is the durable relation trace. Generated Rust owns bounded per-call
    working data. The runtime does not retain a full second relation store in
    RAM.
11. Reload occurs at a completed generation boundary. A new library is loaded
    and validated before the current library is retired.
12. The TS reload catalog rules remain the first migration contract:
    schema change `recreate`, producer change `refill`, equality `keep`, and
    removed relation `drop` when authorized.
13. The Rust/SQLite application lowering consumes generation identity, signed
    row differences, level-rule closure, edge-rule occurrences, state samples,
    and an atomic commit boundary. These are relations in that userland runtime
    model. The DL7 type graph kernel remains identities and edges. Clocks,
    retries, HTTP, servers, CLIs, filesystem watching, and history policy are
    further programs and hosted implementations over the same graph.
14. Wall-clock time enters through hosted rows. Generated code has no implicit
    wall-clock read. Fixpoint rounds, runtime generations, and wall-clock values
    remain separate identities.
15. The same hosted-callable contract serves macrotime, comptime, and residual
    application evaluation. Its phase follows the evaluation root that demands
    it. A callable reachable from several evaluators has phase-scoped demand
    and response state.
16. Reader forms are reified as graph data before executable lowering. DL7
    expansion rules map one input form to zero or more ordered output forms.
    `<+`, `match`, and `scan` are definitions in the DBSP/Rx application over
    that protocol rather than additional Prolog rule forms.
17. Phase is derived from evaluation roots. Expansion outputs select macrotime,
    compiler artifact outputs select comptime, and residual application outputs
    select target time. Pure helper relations may participate in several phases.
18. The DL6 relation-value model becomes one DL7-authored storage application.
    Product and sum values are structurally interned, product- or sum-typed
    fields lower to references, and text fields lower to one shared string
    dictionary. These are derived storage representations rather than graph
    type-system primitives.
19. JSON is a boundary codec annotation over ordinary graph types. Full JSON
    Schema import and emission remain a later emitter package and are not an
    acceptance criterion for the macrotime or relational-runtime foundation.
20. `Console.Stdout` and `Console.Stderr` are occurrence-sensitive hosted sinks.
    Their rows commit after logical closure, in deterministic per-stream order,
    so temporary or retracted derivations do not print.

Disposition of earlier paths:

| Path | Disposition |
| --- | --- |
| `ProgramJson` inside generated Rust | V6 compatibility and debug output |
| actual Differential Dataflow | optional backend after it enters the same shootout |
| one dylib per DL7 file | deferred until a measured need for independent linking |
| one project dylib containing one Rust module per DL7 file | first native reload artifact |
| source-level `import` and `export` keywords | deferred until colon-edge composition proves insufficient |

## Vocabulary boundary

The DL7 graph layer contains identities, `node`, ordered `:/4` edges, products
`*`, sums `+`, references, literals, applications, and rules. An edge identity
can itself own further edges. Dots remain opaque label content at this layer.

The checked Datalog evaluator currently calls each callable product a
`relation`: its ordered edges define tuple positions and its calls are rows.
That word describes one evaluator over the graph. Keyed state, append history,
event boundaries, DBSP operators, SQLite layouts, and the DL6 relation model are
userland schemas and lowerings over graph identities. They do not define what a
DL7 type is.

The target userland temporal vocabulary keeps a maximum path depth of three:

```text
Change.Assert
Change.Retract

Time.Current
Time.Previous
Time.Next

Event.Enter
Event.Exit

Key.Replace
History.Append
History.Window.Count
History.Window.Time
```

`Change.Assert` and `Change.Retract` are signed multiplicities. Applying them to
a stored set produces its current snapshot; that storage fold stays implicit.
`History.Window.Count` and `History.Window.Time` describe the two history
removal boundaries.
`Time.Next` places a derived change in generation `G + 1`; it carries no task,
thread, timer, or wall-clock scheduling semantics. `Event.Enter` and
`Event.Exit` are the positive and negative differences observed at a selected
relation boundary.

DBSP names remain available for the target-neutral lowering graph, for example
`DBSP.Map`, `DBSP.Join`, `DBSP.Antijoin`, `DBSP.Reduce`, and `DBSP.Feedback`.
They identify backend algebra rather than surface language semantics.

## Phase model

The compiler and target share one graph and one rule algebra. The phases differ
by their input roots, available host boundary, output roots, and lifetime:

```text
Read:
  Text -> SyntaxGraph

Macro:
  MacroRules x SyntaxGraph -> ExpandedSyntaxGraph

Compile:
  CompilerRules x ExpandedSyntaxGraph x HostedFacts
    -> CheckedGraph x ResidualGraph x Artifacts

Target:
  ResidualGraph x StoredState x InputChanges
    -> StoredState x OutputChanges x HostDemands
```

`Target` is the residual or emitter phase. A one-shot CLI can evaluate its
residual graph with the Prolog evaluator and exit. A Rust/SQLite emitter can
translate the same graph into a resident executable. Physical execution inside
the compiler process does not change the graph's phase.

Phase membership is derived by dependency reachability:

```text
Expansion output root       -> macrotime
compiler artifact root      -> comptime
residual application root   -> target time
```

The dependency direction is `Macro -> Compile -> Target`. A shared pure helper
can be evaluated from roots in more than one phase. Phase-scoped calls derive
separate demand, response, cache, and occurrence identities.

### Reader graph terminology

For `(foo Bar ?x)`, a **form** is the parenthesized ordered container. An
**atom** is a name token such as `foo`, `Bar`, `<-`, or `->`; a symbol literal
such as `'foo` is immediate data and does not enter name resolution. Each
textual child is a syntax-node occurrence with a source identity and position.
Named `?x` occurrences within one top-level form share one logical variable
identity while retaining separate source occurrences. Each `?_` is fresh.

The current reader stores form order in a Prolog list:

```prolog
node(FormId,
     form([node(FooId, atom(foo)),
           node(BarId, atom('Bar')),
           node(XOccurrenceId, variable(XIdentity, 'x'))]))
```

Syntax graph reification exposes the same order through ordinary edges:

```text
:(FormId, item, FooId, 0)
:(FormId, item, BarId, 1)
:(FormId, item, XOccurrenceId, 2)
```

`reader_node(Path, Index)` is parse-local provenance. Binding identities,
interned applications, and generated semantic identities carry longer-lived
meaning. A macro-introduced identity is derived from its macro definition,
invocation identity, output ordinal, and template path. Spliced user syntax
retains its existing identities.

### Macro expansion relation

One expansion invocation has zero or more ordered output-form edges:

```text
:(Invocation, expansion, OutputForm0, 0)
:(Invocation, expansion, OutputForm1, 1)
```

Zero edges remove the invocation from the active syntax frontier. One replaces
it. Several splice forms in ordinal order. This finite ordered sequence uses
the graph's indexed edges and does not require a first-class cons list.

The compiler reserves declaration identities, evaluates expansion rules, and
derives the active frontier until no active macro invocation remains. Expansion
records remain as provenance. The generated identity formula makes repeated
rounds idempotent and makes separate invocations hygienic. A later quasiquote
surface only needs template preservation, one-node splice, and sequence splice;
those operations lower to the same syntax-node and indexed-edge graph.

The first executable slice is present at:

- `v7/src/0_reader/1a_syntax_grapher.pl`: reader occurrences, source rows,
  top-level frontier order, and form-child `:/4` edges;
- `v7/src/1_libtime/0a_syntax_macro_program.pl`: conversion between those rows
  and an already-checked DL7 macro program;
- `v7/src/1_libtime/0b_syntax_rewriter.pl`: zero-, one-, and many-output graph
  replacement at top-level and nested positions;
- `v7/src/1_libtime/1_syntax_expander.pl`: repeated evaluation to an unclaimed
  frontier, with cycle and round-limit diagnostics;
- `v7/src/0_reader/1b_syntax_materializer.pl`: checked reconstruction of the
  active graph for the current tree lowerer;
- `v7/test/fixtures/14_syntax_macros.dl7`: DL7-authored `drop` and `splice2`
  claims. Ordered outputs are ordinary
  `:(Invocation, expansion, Output, Ordinal)` edges.

`syntax_claim(Invocation, Macro)` is the only extra expansion-control row. It
distinguishes a zero-output expansion from an unclaimed form. The receipt
deletes a top-level form, splices nested children, performs a second expansion
round, preserves reused reader identities, and records claim/output provenance.
`compile_unit_with_macros/4` runs this graph phase before ordinary lowering;
its receipt expands one source declaration form into two declarations and
removes another form before either reaches the lowerer. Normal single-file and
project entry points now compile `v7/macrotime/0_standard.dl7` through the
ordinary checked compiler, cache it by prelude and macro source content, and
expand every source unit before module lowering. The project receipt expands
`<+` and then resolves `accounts:User` through the existing filesystem module
graph. `compile_dl7_macro_program/3` is the bootstrap entry point that compiles
a macro library without applying the standard library to its own source.
Project-authored macro visibility still needs module-edge resolution. The
static tree callback remains only for infix-colon normalization. The
`emit_atom` receipt derives
`application(GeneratedSyntax, [Invocation, OutputOrdinal, TemplatePath])`
through `nil/1`, `cons/3`, and `intern/3`; two source invocations receive
distinct identities, repeated expansion receives equal identities, and each
generated node receives its invocation span through a DL7 rule.
Macro evaluation slices the checked program to claim writers, syntax
constructors, `item` and `expansion` edge writers, and their non-input helper
dependency cone. On the 18-node two-round receipt, expansion alone used
0.051328 CPU seconds on the local SWI 10.0.2 run. The complete focused command,
including compilation of the macro fixture and shared prelude, took 12.867 wall
seconds.

The normal compiler now caches the sliced checked macro program instead of the
complete compiler prelude and compiler rows. The standard library owns its
ordinal closure directly over kernel `predecessor`, so its bootstrap compile
uses one macrotime unit and the kernel relation set. Claim rules whose bodies
join invocation item zero to a literal `syntax_atom` form a resolver-style
dispatch index. A graph containing none of those heads returns exactly without
closure evaluation; the focused no-invocation receipt completed in 0.037
seconds. General claim predicates retain closure evaluation.

## Bind and namespace model

The proposed source is already accepted by the DL7 reader:

```dl7
(A:
  (* (a:
        (+ (X: ())
           (Y: ())))))
```

Its semantic shape is:

```text
:(FileModule, A, ANode, 0)
product(ANode)

:(ANode, a, AFieldNode, 0)
sum(AFieldNode)

:(AFieldNode, X, Unit, 0)
:(AFieldNode, Y, Unit, 1)
```

`A.a.X` remains one opaque atom. Structural traversal is the join:

```dl7
(: ?File A ?ANode ?AIndex)
(: ?ANode a ?AFieldNode ?FieldIndex)
(: ?AFieldNode X ?XTarget ?VariantIndex)
```

The current read-only spelling uses `:/4` in a rule body. An anonymous variable
discards the ordinal:

```dl7
(<- (Found ?Target)
    (: Container payload ?Target ?_))
```

Several steps currently require one goal per edge:

```dl7
(<- (Found ?Target)
    (: A B ?AB ?_)
    (: ?AB c ?ABC ?_)
    (: ?ABC d ?Target ?_))
```

The ordinary relation used for edge projection can expose the target as a
return value:

```dl7
(: Member
   (* (: owner type)
      (: label any)
      (: return type)))

(<- (Member ?Owner ?Label ?Target)
    (: ?Owner ?Label ?Target ?Index))
```

Constant labels in generic calls currently lack the colon relation's special
label lowering. The path surface below supplies labels directly to generated
`:/4` goals and avoids that gap.

An export policy can address the binding edge itself:

```dl7
(<- (: ?Binding visibility Public 0)
    (PublicBinding ?Owner ?Label)
    (edge_ref ?Owner ?Label ?Binding))
```

The edge identity remains available for any other annotation. The resolver can
join the visibility relation when a project chooses restricted traversal.

### Inline and anonymous bind continuity

DL6 assigned each inline product or sum a semantic identity from three inputs:

```text
owning semantic identity
recursive site path
specialized inline shape
```

The site path appended field names while descending through products and
variant fields, and appended argument ordinals while descending through
wrappers or constructor applications. Inserting an unrelated declaration did
not change that identity. Nested declaration blocks and dotted declaration
spelling produced the same authored declaration path before DL6 flattened it.

DL7 retains this behavior as graph structure. Generated relation names remain
diagnostic or rendering artifacts. Every inline `*` or `+` receives an
identity; the binding edge from its owner supplies the next authored path
segment:

```dl7
(Container:
  (* (payload:
        (* (id: int)
           (result:
             (+ (Ok: text)
                (Error: text))))))))
```

```text
File -Container-> ContainerType
ContainerType -payload-> PayloadType
PayloadType -id-> int
PayloadType -result-> ResultType
ResultType -Ok-> text
ResultType -Error-> text
```

The semantic site key is the owning binding-edge identity followed by nested
labels and wrapper/application ordinals. Reader-node identity remains source
origin evidence. The specialized shape distinguishes applications at the same
authored constructor site.

Two equal inline shapes at different binding sites retain different identities.
An explicit alias bind points to the existing identity. A named intermediate
bind exposes any step without changing the target identity:

```dl7
(Payload: (Member Container payload))
(Result: (Member Payload result))
(Ok: (Member Result Ok))
```

Dots remain opaque atoms. `Container.payload.result.Ok` can be an authored label
independent of the four-edge walk above.

### Colon path reads

`A:B:c:d` is reserved as read-only path syntax:

```text
A:B       = follow edge B from A
A:B:c     = follow B from A, then c
A:B:c:d   = follow B, c, and d; return the final target
```

A colon path is an expression. It has no source-visible `return` argument.
The value of `A:B` is the target reached by following `B` from `A`. Lowering
may introduce existential variables while translating a path to `:/4` joins,
but those variables are absent from the path's surface arity.

Using a path directly creates no alias or binding edge:

```dl7
(: Holder
   (* (: value A:B:c:d)))
```

The path supplies a type identity. If its resulting identity is callable, it
can separately occupy call position:

```dl7
(A:B 7 "Ada")
```

The arguments belong to that ordinary call. They are not an output slot for
the path. Binding the path result remains explicit:

```dl7
(BType: A:B)
(DType: A:B:c:d)
```

These binds add `BType` or `DType` edges from the current owner to the resolved
targets. They do not mint copies of those targets.

Path lowering expands to fresh intermediate variables and ordinary `:/4` body
goals. Labels are compile-known atoms. Dynamic owner or label traversal keeps
the explicit relation form:

```dl7
(: ?Owner ?Label ?Target ?Index)
```

The reader currently rejects internal-colon tokens such as `A:B:c:d`. The
receipt must add tokenization and expression lowering for this path form while
preserving the existing trailing-colon bind rewrite `A:`. A path expression is
valid in type, value, callable, and bind-right-hand-side positions. Rule heads
continue to write edges only through explicit `:/4` calls or bind generation.

The continuity receipts are:

1. Nested product and sum sites derive the exact owner/label walk.
2. Adding an unrelated sibling bind preserves existing semantic site keys.
3. Equal shapes at separate sites have separate identities.
4. An alias bind preserves the target identity while gaining its own binding
   edge identity.
5. Inline shapes under `List`, `Option`, or another constructor include argument
   ordinals in their site keys.
6. Generic specializations at one site include their concrete arguments.
7. Named and inline products use the same field-value representation at the
   Rust, SQLite, JSON, and hosted boundaries.
8. `A:B:c:d` lowers to three ordered edge reads and creates no binding edge.
9. `(Alias: A:B:c:d)` creates one alias edge to the final target.

## Type signatures

The first implementation signatures are grouped by layer.

```prolog
% Reader nodes and list positions to the ordinary indexed syntax graph.
reify_syntax(
    +ReaderForms,
    +SourceRows,
    -SyntaxGraphRows,
    -Diagnostics
).

% Evaluate DL7 expansion rules to a closed active syntax frontier.
expand_syntax(
    +SyntaxGraphRows,
    +MacroProgram,
    -ExpandedSyntaxRows,
    -ExpansionProvenanceRows,
    -Diagnostics
).

% Checked program to ordinary DL7 rows.
logical_program_calls(
    +CompilerView,
    -Relations,
    -Seeds,
    -Diagnostics
).

% Second compiler evaluation over reified program calls.
evaluate_dl7_emitter(
    +EmitterIdentity,
    +CompilerView,
    -Artifacts,
    -Diagnostics
).

% Target-neutral relational lowering.
lower_dbsp(
    +ProgramRows,
    +TypeGraphRows,
    -DbspRows,
    -Diagnostics
).

% Storage and execution selection expressed as DL7 rules.
lower_rust_sqlite(
    +DbspRows,
    +TypeGraphRows,
    -RustSqliteRows,
    -Diagnostics
).

% Derive local IDs, durable digests, scalar dictionaries, and reference fields
% from the graph types selected by the relational storage application.
derive_value_storage(
    +TypeGraphRows,
    +StorageApplicationRows,
    -InternPlanRows,
    -ReferencePlanRows,
    -Diagnostics
).

% Normalize one boundary batch child-first into scalar and relation IDs.
intern_boundary_values(
    +InternPlanRows,
    +SignedLogicalRows,
    +StoreHandle,
    -SignedPhysicalRows,
    -IdentityRows,
    -Diagnostics
).

% Reify temporal dependencies without adding a second type vocabulary.
temporal_program_rows(
    +CheckedProgramRows,
    -ClockDependencyRows,
    -ClockDiagnosticRows
).

% Execute one externally visible generation.
run_generation(
    +ProgramHandle,
    +GenerationIdentity,
    +SignedArrivals,
    +StoreHandle,
    -SignedOutputs,
    -HostDemandDifferences
).

% Derive protocol boundaries from ordinary hosted relation and type rows.
boundary_plan(
    +CompilerView,
    +HostedRows,
    -BoundaryRows,
    -Diagnostics
).

% Dispatch committed occurrence-sensitive stdout and stderr sink rows.
dispatch_console(
    +PhaseIdentity,
    +CommittedConsoleRows,
    +DeliveredOccurrenceRows,
    -NextDeliveredOccurrenceRows,
    -Diagnostics
).

% One source module per DL7 source module plus one project root.
emit_rust_modules(
    +CompilerView,
    +RustSqliteRows,
    -OwnedArtifacts,
    -Diagnostics
).
```

The dynamic-library boundary uses a C-compatible root descriptor. Exact cell
and diagnostic representations remain an implementation decision after the
first generated tick body exists.

```rust
#[repr(C)]
pub struct Dl7ModuleV1 {
    pub abi_version: u32,
    pub program_digest: Dl7Bytes,
    pub schema_digest: Dl7Bytes,
    pub rule_digest: Dl7Bytes,
    pub open: unsafe extern "C" fn(*const Dl7HostV1) -> Dl7Status,
    pub tick: unsafe extern "C" fn(*const Dl7HostV1, Dl7Batch) -> Dl7Status,
    pub close: unsafe extern "C" fn(*const Dl7HostV1),
}

#[no_mangle]
pub unsafe extern "C" fn dl7_module_v1() -> *const Dl7ModuleV1;
```

`Dl7HostV1` contains versioned calls for SQLite statement execution, row
iteration, interned values, diagnostics, and cancellation. Rust-owned structs,
`rusqlite::Connection`, and allocator-owned strings do not cross the boundary.

## Instance timelines and lifetimes

### Compiler and source membership

The stage-zero trusted inputs are the DL7 prelude, one entry file, and the
project root supplied by the command. The entry program derives desired source
membership. A hosted filesystem relation returns immutable source revisions.

```text
entry + root
  -> desired source paths
  -> hosted watch/read operation
  -> SourceRevision(path, digest, content) differences
  -> selected compilation units
  -> file products and colon edges
  -> checked project
```

One `SourceRevision` lives until the watcher retracts it. A content edit is one
negative old revision and one positive new revision. File removal retracts its
revision and the module rows derived from it.

### Generated project

Each compiler generation owns:

- a closed checked program;
- one DBSP graph;
- one Rust/SQLite lowering;
- one generated Rust module per loaded DL7 file;
- one project root module;
- one content-addressed dynamic library.

The resident runtime owns one active generation and may hold one validated next
generation. An old library remains loaded until its active tick count reaches
zero. No pointer, callback, or buffer owned by the old library survives unload.

Dynamic loading is currently a researched boundary rather than an executable
DL7 path. `v7/labs/19_rust_dynamic_loading/0_RESEARCH.md` specifies the ABI,
artifact identity, reload sequence, and benchmark matrix. DL7-generated Rust
types and generated Rust DBSP construction modules exist; a generated project
`cdylib`, resident loader, and generation-boundary swap do not yet exist.

### Tick

```text
begin generation
  -> read signed arrivals
  -> apply base-relation changes in one SQLite transaction
  -> close level rules by stratum and SCC
  -> process ordered edge-rule occurrences
  -> materialize relation differences
  -> commit
  -> publish output differences
  -> execute positive hosted demand differences outside the transaction
  -> enqueue hosted responses for a later generation
end generation
```

The generated operator functions borrow the host context for one call. SQLite
owns durable rows after commit. Temporary join batches and decoded values end
with the call.

HTTP, Git, process, and filesystem effects run after commit. Their responses
enter a later transaction as signed arrivals. A database write lock therefore
has no network, subprocess, or checkout lifetime.

### Reload

```text
watcher observes source delta
  -> compile the next closed project
  -> emit changed Rust modules and project root
  -> incremental Rust build to a fresh digest path
  -> dlopen and validate ABI plus catalog digests
  -> finish the active tick
  -> begin SQLite migration transaction
  -> keep, refill, recreate, or drop affected relations
  -> install the next module pointer
  -> commit migration
  -> retire the old library after active calls reach zero
```

A compile, load, ABI, or migration failure leaves the active generation and its
SQLite state unchanged.

## Storage, reads, writes, and uniqueness

### Stable identities

| Item | Identity |
| --- | --- |
| source revision | canonical path plus content digest |
| file module | canonical source path |
| nested node | source file identity plus reader node identity |
| binding edge | owner identity plus label, under the current functional edge key |
| checked call | structural call id emitted by `0_logical_program_reifier.pl` |
| DBSP operator | normalized rule and operator-position identity |
| SQLite relation | semantic relation identity digest |
| generated Rust module | file module plus relevant lowering digest |
| project library | ordered module artifact digests plus ABI version |

Labels remain presentation and lookup data. Physical SQLite names and Rust
identifiers derive from semantic identity digests, with authored labels retained
for diagnostics.

### SQLite state

SQLite stores base rows, derived rows selected for materialization, interned
values, relation catalogs, source revision catalogs, and applied-generation
receipts. Transient frontier tables are emitted only for operators whose
lowering needs them. The database page cache is the principal resident cache.

The generated runtime reads signed arrivals and current relation slices. It
writes relation changes, catalog changes during reload, and one generation
receipt in the same transaction. A successful receipt uniquely identifies the
program digest and input generation.

### Module state

The host stores the active library handle, its root descriptor pointer, active
call count, and content-addressed path. Build products live outside the SQLite
transaction and become eligible for loading only after Rust compilation
succeeds.

## DL6 relation values as a DL7 storage application

DL6 coupled four behaviors under its relation declarations:

1. A field whose declared type was another relation accepted a nested relation
   term as its logical value.
2. `0_relation_pattern.pl` recursively canonicalized those nested terms before
   execution.
3. `sprefa-engine-rs/src/struct_plane.rs` interned distinct child values before
   parents and replaced each child value with the child's SQLite `__id`.
4. `sprefa-engine-rs/src/text_plane.rs` interned every `text` column through one
   shared `__str(__id, content)` dictionary when the emitted plan selected
   dictionary mode.

That behavior becomes one application over the DL7 graph. A product or sum
constructor application has a structural identity:

```text
SemanticIdentity = Intern(ConstructorIdentity, OrderedArguments)
```

The current comptime kernel already implements this operation as
`intern(constructor, arguments, return)`. Its Prolog result is the structural
term `application(Constructor, Arguments)`, and `intern_snapshot` exposes
requests frozen from the preceding compiler round. This currently names
comptime applications; it does not yet generate persistent Rust/SQLite value
interning.

The storage application derives physical reference fields from the ordinary
type graph:

```text
product(Span)
:(Span, start, int, 0)
:(Span, end, int, 1)

product(Node)
:(Node, file, File, 0)
:(Node, at, Span, 1)
:(Node, kind, text, 2)

Storage.Reference(Node, file, File)
Storage.Reference(Node, at, Span)
Storage.Text(Node, kind)
```

`Storage.Reference` and `Storage.Text` are derived layout rows emitted by the
DL7 Rust/SQLite package. They are not additional source type constructors. A
logical arrival can retain its nested value shape while boundary lowering does:

```text
collect distinct nested values
  -> intern scalar text contents
  -> intern deepest product and sum values
  -> replace child values with local child IDs
  -> intern parents
  -> replace top-level reference fields with local IDs
  -> apply signed physical rows
```

The reverse boundary receives an explicit projection graph. It can return a
local reference identity, selected child fields, or a recursively rendered
value. This avoids making every reference read recursively serialize its whole
reachable graph.

### Hashes and intern IDs

If “half and intern” meant “hash and intern”, both identities have separate
jobs:

```text
semantic digest:
  hash(canonical constructor identity, child digests, scalar values)

local intern ID:
  dense identity assigned within one SQLite catalog or in-memory evaluation
```

The digest can survive process restarts, artifact builds, and database exchange
when its canonical encoding and version are fixed. The local ID is used for
joins, indexes, and compact reference columns. One catalog records the mapping:

```text
Identity(LocalId, SemanticDigest, Constructor)
```

Domain identities remain ordinary declared keys. A Git object ID, source
revision digest, or external protocol identifier is not replaced by the local
intern ID.

### Text and source-content interning

The Rust/SQLite relational application derives one shared text dictionary for
all physical text fields:

```text
Text(LocalId, Content)
```

Rows and interned product values store `LocalId`. Host and rendered boundaries
read `Content`. Rule literals are looked up through the same dictionary so a
literal comparison and a column comparison use the same physical ID space.

Large immutable source contents use a separate content-addressed table:

```text
Content(Digest, Bytes)
SourceRevision(PathId, Digest)
```

The relational graph carries `PathId`, `Digest`, spans, symbols, and references.
Raw source bytes are fetched by digest when parsing, rendering diagnostics, or
serving a requested slice. This keeps repeated text and source bodies out of
every relation row while retaining their logical value at boundaries.

### Spans

A span remains an ordinary product, commonly:

```dl7
(: Span
   (* (: source SourceRevision)
      (: start ByteOffset)
      (: end ByteOffset)))
```

Offsets are half-open: `start` is inclusive and `end` is exclusive. Structural
interning means repeated use of the same `(source, start, end)` stores one span
value and several reference IDs. A syntax or semantic node that needs `kind` as
part of its identity includes `kind` in that node's own constructor or declared
domain key; two nodes may therefore share one span without becoming one node.

The colon read surface can follow logical values through these reference
fields. The SQLite emitter lowers each reference step to a join on the stored
ID. A read of the identity itself remains a projection of the ID and performs
no child join. Repeated identical paths in one rule share their normalized join
prefix in the DBSP graph.

## Relational lowering flow

```text
DL7 files
  -> reader trees and source rows
  -> indexed syntax graph
  -> DL7 macro expansion to a closed active syntax frontier
  -> colon-owned type and module graph
  -> checked Datalog
  -> reified program calls
  -> DL7-authored DBSP/Rx application rules
  -> DBSP relation and operator rows
  -> DL7 Rust/SQLite rules
  -> value interning, references, layout, SQL, schedule, and Rust-item rows
  -> DL7 Rust emitter
  -> one .rs per DL7 file plus lib.rs
  -> content-addressed cdylib
  -> resident Rust host plus SQLite
```

The initial DBSP vocabulary needs only enough rows to preserve checked-program
meaning:

- relation identity, arity, keys, input role, and output role;
- operator identity and kind;
- ordered reads and one write;
- variable equality and literal predicates;
- ordered projection;
- aggregate and grouping metadata;
- dependency, stratum, SCC, and feedback membership.

Storage layout, SQL spelling, Rust identifier spelling, dynamic-library ABI,
watch policy, and host transports enter later lowering relations.

## Reactive kernel and userland

### Application-runtime temporal basis

Current implementation status:

- `<-` is accepted by the reader and recognized by the Prolog lowerer as the
  only source rule form.
- `<+` is accepted as one reader atom. The checked macro fixture constructs a
  fresh form and `<-` operator, copies the original head and body children, and
  removes `<+` before the existing lowerer. The lowerer has no `<+` branch.
  `compile_unit_with_macros/4` exercises the explicit path. Normal file and
  project entry points load the standard checked macro program and exercise
  the same expansion before module lowering.
- `->` is accepted only as an atom token. It has no rule, pattern, arm, or
  direction semantics in the lowerer.

The existing DL6 fixtures provide historical input/output receipts for this
userland runtime model:

| Construct | Runtime meaning | Dependency row |
| --- | --- | --- |
| `<-` | close a level relation within the current generation | positive or negative, grade 0 |
| `<+` | fire once for each ordered trigger occurrence and write keyed state or history | trigger grade 0 or 1 |
| bare positive edge-body atom | trigger source | read ring `z` |
| `latest(Rel(...))` | sample current visible state without becoming a trigger | state sample, grade 0 |
| `pre(Rel(...))` | sample persistent state after earlier writes in this generation, with prior level rows frozen | previous sample, grade -1 |
| signed boundary difference | add, replacement pair, retention removal, or departure | generation boundary |

The old ring names `b`, `z`, and `n` are fixture representation details. The
DL7 application lowering reifies their meanings as relation plane, occurrence
plane, and history plane.
The reified dependency row carries source relation, target relation, read plane,
write plane, sign, grade, and role. Clock checking is a DL7 analysis over those
rows.

The temporal identities have an algebraic dependency rather than independent
operator semantics. For a signed multiset relation at generation `G`:

```text
Current[G]  = Previous[G] ⊕ Change[G]
Change[G]   = Current[G]  ⊖ Previous[G]
Previous[G] = Current[G]  ⊖ Change[G]
```

Any two determine the third. `⊕` and `⊖` are signed-weight addition and
subtraction. `Change.Assert` and `Change.Retract` are the positive and negative
parts of `Change`. `Event.Enter` and `Event.Exit` are zero-boundary crossings
derived by comparing `Previous` and `Current`; they differ from another `+1`
or `-1` applied to a row whose consolidated weight remains nonzero.

The runtime must supply one generation boundary with durable prior state. The
remaining temporal views can be DL7 rules over signed changes, previous state,
and current state. A keyed replacement expands to one retraction of the prior
keyed row and one assertion of its replacement. A history append expands to an
assertion carrying an occurrence identity.

Within one generation:

```text
ordered outside arrivals
  -> base state update
  -> level closure
  -> newly true level occurrences
  -> ordered edge firing
  -> keyed replacement or history append
  -> post-write level closure
  -> signed boundary differences
  -> next-generation carry
```

An edge write to a keyed relation replaces the row sharing its declared key.
An equal replacement has zero boundary difference. Multiple different writes
for one key from one occurrence are a conflict. History appends preserve an
occurrence stamp. Carry contains boundary-visible positive writes and subscribed
departures, so intermediate fold states do not trigger another generation.

During comptime, time stratification queries the same reified dependency graph
already used for ordinary stratification. It derives weighted strongly connected
components, inferred clocks, boundary facts, and diagnostics before DBSP
lowering. Grade-zero positive level cycles are constructive.
Every cycle in a delayed recurrence must have positive total grade. Cross-plane
uses such as `latest` or `pre` inside a level rule produce diagnostics. The DL6
path-conflict and nonconstructive-cycle walk was pinned off on the production
compile path; DL7 receives it only after its cost and acceptance set are covered
by receipts.

### Generic form expansion

Lisp forms remove the dedicated statement grammar that DL6 used for `match`.
The reader already produces one nested form tree for the operator, scrutinee,
and arm forms. The remaining compiler mechanism is a generic pre-check expansion
protocol:

1. Reify source forms as ordinary application nodes and ordered argument edges.
2. Mark a callable such as `match` as compile-time syntax with one ordinary
   annotation edge.
3. Let DL7 prelude rules inspect its application and emit `head`, `body`,
   `Apply`, and `:/4` rows through the existing generated-program carrier.
4. Omit the syntax application from the checked runtime program after its
   generated rows reach compiler fixpoint.

This is one compiler protocol shared by `match`, `scan`, future syntax
constructors, and project-defined forms. Individual forms require no Prolog
branch and no reader production beyond their ordinary list shape.

### Rule graph and bootstrap boundary

DL7 can already generate rules as data, but the current carrier is split
between kernel relations and prelude relations:

```dl7
(def Relation 2)
(head Rule HeadCall)
(body Rule 0 "positive" BodyCall)
(Apply HeadCall Relation)
(Apply BodyCall Source)
(: HeadCall value HeadArgument 0)
(: BodyCall value BodyArgument 0)
```

`def/2`, `head/2`, and `body/4` are currently hardcoded into the kernel graph
in `v7/src/2_comptime/1_checker.pl` and interpreted by
`v7/src/2_comptime/1a_generated_program_assembler.pl`. `Apply/2`, variable
nodes, literal nodes, and call-argument edges are declared in the DL7 prelude.
The compiler already evaluates authored rules to a fixpoint, assembles these
rows, then evaluates again with the generated rules.

`v7/src/3_emit/0a_logical_program_grapher.pl` now supplies the parallel
graph-normalized view without changing the compatibility carrier:

```text
product(Rule)

:(Rule, head, HeadCall, 0)
:(Rule, body, BodyOccurrence0, 1)

product(HeadCall)
:(HeadCall, apply, Relation, 0)
:(HeadCall, argument, HeadArgument0, 1)

product(BodyOccurrence0)
:(BodyOccurrence0, polarity, positive, 0)
:(BodyOccurrence0, call, BodyCall, 1)

product(BodyCall)
:(BodyCall, apply, Source, 0)
:(BodyCall, argument, BodyArgument0, 1)

product(BodyArgument0)
:(BodyArgument0, variable, VariableIdentity, 0)
```

The head occupies rule edge zero; body goal `N` occupies rule edge `N + 1`.
The callable occupies call edge zero; argument `N` occupies call edge `N + 1`.
The body-occurrence node carries explicit polarity now and can receive clock,
trigger, and sampling edges without changing the shared call node. Write mode
can use the same occurrence pattern for the head. Generated relation arity
comes from the product's field edges, so `def/2` is a derivable compatibility
view. The assembler's irreducible operation becomes "freeze and check the graph
rooted at each rule head" rather than interpreting three unrelated kernel
relations.

`logical_program_graph_calls/3` converts this view directly to kernel `node`,
`product`, and `:` calls and filters them by an emitter's dependency cone.
`v7/src/3_emit/1_artifact_emitter.pl` now includes those calls when a DL7
emitter reads the corresponding kernel relations. Existing emitters continue
to receive the compatibility `program_*` relations in parallel; the DBSP
artifact receipt remains byte-for-byte equal after the additional seed path.

The bootstrap basis is then:

1. The reader produces ordinary nested forms and preserves their identities and
   order.
2. `<-` lowers one monotone rule. Body goals are conjunction. Multiple `<-`
   rules sharing a head are disjunction. Stratified negative goals provide
   antijoin.
3. The generic form protocol exposes raw forms to already-compiled DL7 prelude
   rules.
4. Those prelude rules emit the normalized rule graph for a later compiler
   round.
5. The runtime supplies signed input changes and a generation boundary with
   prior state.

With that basis, `<+` can be a DL7 macro. Its expansion is an ordinary `<-`
rule whose body reads an occurrence such as `Event.Enter` and whose head emits
a keyed-replacement, history-append, or next-generation action row. The
resident runtime interprets committed action rows at the generation boundary.
`<+` therefore requires no second source rule kind in the parser or Prolog
lowerer. Clock, trigger, and write-plane rows are derived annotations on the
expanded rule graph.

The first executable `<+` receipt performs the syntax transformation and then
passes the generated form through the ordinary lowerer. Its head and body calls
share the authored variable identity. The fixture uses neutral `Action` and
`Event` relations; `Key.Replace`, `History.Append`, `Time.Next`, and
`Event.Enter` schemas remain part of the DBSP/Rx application cut.
The macro evaluator derives predecessor rows from the active syntax item edges
and excludes the checked macro program's retained compiler predecessor seeds.
The latter contained 448 rows in this fixture and made unrelated type-graph
owners enter the recursive item-copy SCC. The complete 13-test reader and
macrotime command passes under SWI's default 1 GB stack in 13.919 wall seconds.

This mirrors the boolean basis already present in rules:

```text
AND = several goals in one body
OR  = several rules with the same head
NOT = stratified negative goal / antijoin
```

`AND` and `OR` come from the shape of `<-`; only stratified negation adds a
non-monotone primitive.

### `match`

The DL6-compatible surface can contain complete rule fragments as arms:

```dl7
(match (Source ?Key ?Value)
  (<- (Accepted ?Key)
      (GreaterEqual ?Value 10))
  (<+ (Latest ?Key ?Value)))
```

Expansion prepends the scrutinee to every arm body:

```dl7
(<- (Accepted ?Key)
    (Source ?Key ?Value)
    (GreaterEqual ?Value 10))

(<+ (Latest ?Key ?Value)
    (Source ?Key ?Value))
```

This form is relational fan-out. Every satisfied arm emits. DL6 used this
meaning.

Rust-style matching over a closed sum uses pattern arms:

```dl7
(match ?Response
  ((Page ?Body)
   (<- (PageBody ?Body)))
  ((Redirect ?Url)
   (<- (RedirectTarget ?Url))))
```

Inside `match`, each arm is an ordered pair of a pattern form and a rule
fragment. It introduces no arm-arrow token. The earlier draft's `->` spelling
was only a proposed delimiter. In the current implementation the reader emits
`node(Id, atom('->'))`, and the generic call lowerer attempts to resolve it as
an ordinary callable named `->`. Since no such callable is declared, lowering
would report `undeclared_relation('->')` when the form reaches call lowering.

Each variant pattern lowers to its ordinary constructor/deconstructor relation.
Variant disjointness supplies exclusivity. A compile-time query joins arm
patterns against the matched sum's `:/4` variant edges to report missing or
duplicate variants.

Guarded first-match semantics add one derived `Matched(Occurrence, ArmOrdinal)`
relation. Arm zero reads its pattern and guard. Every later arm reads its own
pattern and guard and antijoins earlier successful ordinals for the same
scrutinee occurrence. This supplies ordered Rust guard fallthrough without a
runtime match operator.

After expansion, DBSP and Rust/SQLite lowering see only ordinary rules, joins,
antijoins, and variant operations.

### `scan`

`scan` names ordered state accumulation in the Rust iterator and RxJS sense.
Its reducer is an ordinary callable relation:

```dl7
(: AddStep
   (* (: previous int)
      (: input int)
      (: return int)))

(: RunningTotal
   (Scan NumberEntered 0 AddStep))
```

The `Scan` constructor generates one keyed state relation and rules equivalent
to:

```text
Event.Enter(NumberEntered, occurrence, input)
Time.Previous(RunningTotal, key, previous)
AddStep(previous, input, next)
Key.Replace(RunningTotal, key, next)
emit RunningTotal(key, next) for this occurrence
```

Occurrence order is `(generation, ordinal)`. Several inputs in one generation
fold in ordinal order, so each intermediate state remains observable as RxJS
`scan` output. A reducer returning zero rows suppresses output for that input.
A reducer returning a `Continue(state, output) | Stop` sum covers Rust
`Iterator::scan` termination without changing the temporal basis.

`Scan` is a userland constructor over `Event.Enter`, `Time.Previous`,
`Key.Replace`, and generated rules. The compiler kernel supplies form expansion,
time-stratification queries, and the public generated-program carrier.

`latest` and `pre` remain body operations until a DL7 program demonstrates a
smaller relation-only expansion with identical ordering behavior. `files` and
`files_at` remain hosted relations for live-worktree and pinned-revision
enumeration. `next`, `await`, and conditional routing remain ordinary registered
relations over generation-scoped rows.

### Clock inputs

A clock host emits rows such as `ClockTick(clock, bucket, instant)` and retracts
generation-scoped tick rows on the following generation. Period, deadline,
debounce, cadence, and retry policies are ordinary relations joining those rows.
Generated Rust reads the supplied instant or bucket value. Replay therefore uses
the recorded clock relation and does not consult the machine clock.

## Hosted relations as graph annotations

The current `(Host Implementation Inputs Outputs)` form has a dedicated branch
in `0_lowerer.pl`. That branch combines both products into one callable product
and synthesizes `Hosted` and `HostPort` rules. It also explicitly bypasses the
ordinary expression-bind and generated-program path. The host planner later
validates the resulting rows after compiler fixpoint and removes only the
planning relations from the emitted program.

The graph already supports the verbose userland representation:

```dl7
(: ExtractTsi
   (* (: source Source)
      (: mode ExtractMode)
      (: record TsiRecord)))

(Hosted ExtractTsi SprefaExtract)
(HostPort ExtractTsi source Input)
(HostPort ExtractTsi mode Input)
(HostPort ExtractTsi record Output)
```

The target source form declares the callable product normally, annotates its
reified declaration edge with the implementation, and annotates output field
edges. Unmarked callable fields are inputs. In expanded rows:

```text
:(File, ExtractTsi, ExtractTsiRelation, declaration_index)
edge_ref(File, ExtractTsi, ExtractTsiDeclarationEdge)
:(ExtractTsiDeclarationEdge, host, SprefaExtract, 0)

:(ExtractTsiRelation, record, TsiRecord, field_index)
edge_ref(ExtractTsiRelation, record, RecordFieldEdge)
:(RecordFieldEdge, direction, Output, 0)
```

DL7 rules derive `Hosted(Relation, Implementation)` from the first annotation,
derive output `HostPort` rows from the second annotation, and derive input
`HostPort` rows from the remaining callable field edges. Source relations mark
all fields as output. Sink relations have no output-field marks.

The accepted short surface for annotating a declaration or inline field edge
still needs a compiler receipt. The implementation cut is:

1. Add the short nested-colon annotation surface and preserve the reified edge
   identity it targets.
2. Express host-marker and port-direction normalization as prelude rules.
3. Feed those derived rows to the existing post-fixpoint host validation.
4. Remove the `Host` target branch and its expression-bind exclusion from the
   lowerer after the old fixture has an equivalent graph-authored fixture.

The exact current `Host` expression could instead become a userland constructor.
`HistoryV1` proves that DL7 rules can intern a result identity, copy ordered
edges, derive `node` and `product`, emit `def`, and emit executable `head` and
`body` rows through the generated-program carrier. The normal-declaration form
uses fewer generated identities and makes hosting an annotation on existing
graph objects.

### Console stdout and stderr sinks

The relational application declares two ordinary one-field products:

```dl7
(: Console.Stdout
   (* (: line text)))

(: Console.Stderr
   (* (: line text)))
```

Their declaration edges are annotated as hosted sinks implemented by the
resident console host. One logical row means one complete UTF-8 line; the host
adds the line terminator. Exact byte writes can be a later separately named
boundary without changing line logging.

Surface use is an ordinary rule head:

```dl7
(<- (Console.Stdout ?Rendered)
    (ReportLine ?Rendered))

(<- (Console.Stderr ?Rendered)
    (TraceLine ?Rendered))
```

The source arity stays one. Lowering attaches an occurrence envelope derived
from the phase, rule-head occurrence, generation, and triggering occurrence:

```text
ConsoleDemand(
  phase,
  generation,
  occurrence,
  stream,
  line
)
```

This preserves repeated equal lines caused by distinct occurrences while
consolidating duplicate proofs of the same logical occurrence. Dispatch is an
irreversible terminal effect:

```text
logical closure
  -> consolidate positive and negative sink differences
  -> commit phase or generation
  -> sort by (generation, occurrence ordinal)
  -> write each undelivered positive occurrence
  -> flush each stream
  -> record delivered occurrence
```

A retraction before commit cancels the pending line. A retraction after the OS
write cannot erase bytes already observed. The delivered-occurrence ledger
prevents repeats within the live evaluation and reload lifetime. The contract
makes no cross-crash exactly-once claim because a terminal file descriptor has
no transactional acknowledgement shared with SQLite.

The evaluator supplies the phase identity. Macro-expansion trace rows therefore
print after one closed expansion round, compiler rows print after comptime
closure, and residual application rows print after an application generation
commits. Compiler diagnostics and tracing continue to use stderr; stdout remains
available for deliberate program or artifact output.

## Comptime and application-host execution

The evaluator consuming a hosted callable determines its phase:

```text
compiler evaluation demands ExtractTsi or Soopy
  -> compiler host executes the process
  -> response rows re-enter compiler evaluation
  -> compiler closure and generated artifacts include the result

generated application demands ExtractTsi or Soopy
  -> committed demand difference reaches the resident host
  -> application host executes the process
  -> response rows enter a later application generation
  -> application rules close again
```

Today, source extraction and Soopy region application are direct Prolog process
calls in their mainer modules. They sit outside the hosted relation contract.
The application artifact emitter performs one dependency-sliced Datalog
evaluation to fixed point. It has no host-demand execution loop.

Application run-to-completion means repeating committed generations until the
host-response queue and `Time.Next` carry are empty:

```text
close pure rules
  -> commit signed outputs and hosted demand differences
  -> execute host demands outside the transaction
  -> enqueue responses as later-generation assertions
  -> repeat while responses or carry remain
```

The persistent SQLite watch path has transactional generations and signed
retractions. Host-demand derivation, execution, response ingestion, and the
quiescence loop remain implementation work for the app emitter and resident
host.

## Lists

DL7 comptime currently exposes `nil(return)` and
`cons(head, tail, return)` as kernel relations. The evaluator represents their
values as finite proper Prolog lists. A ground list can be deconstructed; a
ground head and ground proper tail can construct the next list. Prelude rules
already use these calls for constructor argument lists, closed name sets, and
recursive `contains`.

Macro output sequences and form children use repeated indexed edges. They do
not require a cons value. A first-class list is reserved for programs that need
recursive head/tail values, sharing, or list pattern rules.

The graph has no declared recursive `List` sum/product type yet. Its userland
shape is one empty variant and one product carrying `head` and `tail`. Proper
finite values can be proven relationally from the empty value:

```text
Proper(Nil)

Proper(Value) <-
  Cons(Value, Head, Tail)
  Proper(Tail)

Invalid(Value) <-
  ClaimedList(Value)
  not Proper(Value)
```

A tail cycle with no path to `Nil` never enters `Proper`. Product cardinality
and sum-variant checks separately guarantee one tail and one constructor per
value. The current `nil` and `cons` calls can bootstrap these constructor rules.
Rust/SQLite layout, dylib ABI cells, equality, and structural interning then
derive from the same product/sum storage application described above.

JSON array conversion is a codec edge on a host or artifact boundary. It is not
part of list formation, properness, equality, or storage identity.

## Hosted HTTP, servers, and CLIs

The current `Host` bootstrap lowers a callable product into `Hosted` and
`HostPort` rows. Empty input or output products extend that contract to sources
and sinks:

```dl7
(: HttpFetch
   (Host HttpClient
      (* (: request HttpRequest))
      (* (: response HttpResponse))))

(: HttpRequestArrived
   (Host HttpServer
      (*)
      (* (: request HttpRequest))))

(: HttpResponseReady
   (Host HttpServer
      (* (: response HttpResponse))
      (*)))

(: CliInvocationArrived
   (Host CliServer
      (*)
      (* (: invocation CliInvocation))))

(: CliResultReady
   (Host CliServer
      (* (: result CliResult))
      (*)))
```

The paired source and sink relations share a correlation field in their ordinary
request/response products. Route, method, status, header, query, body, argument,
environment, standard-input, standard-output, standard-error, and exit-code data
remain fields or relations. HTTP client execution during compiler evaluation is
a compile-time host call. The same relation retained in the checked runtime
program is a runtime host call. The host implementation, cancellation contract,
and cache identity are annotations on reified relation or port edges.

HTTP server lifecycle:

```text
resident listener accepts request
  -> enqueue +HttpRequestArrived
  -> run and commit one or more generations
  -> observe +HttpResponseReady with the same correlation identity
  -> write response outside SQLite transaction
  -> enqueue acknowledgement or disconnect rows when subscribed
```

CLI lifecycle uses the same source/sink pairing. A short command waits for one
terminal result. A long-lived command subscribes to output rows until an exit or
cancellation row is committed.

## JSON boundary

JSON compatibility is a codec capability on an ordinary type, field edge,
variant edge, hosted port edge, or artifact-output edge. Core products, sums,
lists, text, numbers, identities, and references retain their graph meaning.

```text
BoundaryEdge -codec-> Json.Value
ListEdge     -codec-> Json.Array
ProductEdge  -codec-> Json.Object
VariantEdge  -codec-> Json.Tagged
```

The codec package derives encode, decode, validation, and diagnostic rows at
the selected boundary. Unannotated graph values acquire no JSON representation.
Host input is validated before logical rows enter an evaluation. Host output is
encoded after its phase or generation commits.

JSON Schema 2020-12 import, emission, and category-corpus coverage remain a
later DL7 emitter package over these codec and constraint edges. They do not
gate syntax reification, macros, relation-value interning, clock checking, or
the first Rust/SQLite residual application.

## Delivery sequence

1. Reify reader forms and their ordered children as the ordinary syntax graph,
   retaining source-node, logical-variable, binding, and generated identities.
2. Add the generic expansion relation, active syntax frontier, deterministic
   generated identities, and repeated-round termination diagnostics.
3. Normalize generated rule, head occurrence, body occurrence, application,
   argument, polarity, clock, trigger, and write metadata onto reified graph
   nodes and edges. Retain `def/head/body/Apply` as derived compatibility views
   while the existing assembler consumes them.
4. Define `<+`, `match`, and `scan` in the DL7-authored DBSP/Rx application and
   prove their expansions contain only `<-`, stratified negation, ordinary
   applications, and occurrence annotations.
5. Reify temporal dependency sign, grade, and plane from occurrence-edge
   annotations. Implement the clock checker as DL7 comptime rules and compare
   its complete diagnostic rows with the existing Prolog checks.
6. Complete SCC, recursion, aggregate, negation, and retraction DBSP cases.
   Every supported operator enters the relational shootout before the
   Rust/SQLite emitter uses it.
7. Derive the DL6-style relation-value application: structural product and sum
   identities, bottom-up reference flattening, the shared text dictionary,
   content-addressed source bytes, and span references.
8. Add `Console.Stdout` and `Console.Stderr` hosted sinks to the Prolog
   comptime runner and the Rust/SQLite target, with committed occurrence order
   and duplicate-delivery receipts.
9. Define the minimal target-neutral relation layout rows: stored role, ordered
   columns, semantic representation, references, and keys. Derive SQLite table
   identities, DDL, statements, and reload catalog hashes in DL7.
10. Restore a generalized form of the compiled Rust technique represented by
   `v6/labs/exec_shootout/mono/src/main.rs`. Emit one Rust module for each DL7
   file and direct operator functions rather than a runtime-plan string.
11. Build one static executable first, then place the same generated project
   behind the `Dl7ModuleV1` boundary.
12. Add a resident host that watches sources, incrementally compiles a fresh
   content-addressed library, validates it, and swaps at a generation boundary.
13. Feed `sprefa-extract` watch changes into the active target and persist their
   relational state in SQLite.
14. Move source-membership selection from the CLI path list into DL7 rules over
    hosted filesystem observations, retaining the entry file and prelude as the
    bootstrap roots.
15. Add generic hosted source and sink relations, then exercise HTTP client,
    HTTP server, and CLI request/response lifecycles over them.
16. Add JSON codec annotations at selected hosted and artifact boundaries.
    JSON Schema import, validation, and emission remain a later emitter package.

## Implementation receipts

The first executable cuts landed on `feature/dl7-source-intelligence`:

| Commit | Receipt |
| --- | --- |
| `3b5e019fe` | checked programs re-enter a dependency-sliced second DL7 comptime evaluation as `program_*` calls |
| `1dc9b0696` | `v7/emitters/0_dbsp.dl7` derives relation, operator, read, and projection rows |
| `17a379d05` | clock dependencies are queryable during comptime with sign, grade, and role |
| `7f3d446df` | zero-input hosted sources and zero-output hosted sinks retain ordinary relation types and erase planning nodes |
| `b4cd5459b`, `db8265af7`, `f2fd4e18e` | argument occurrences, nested aggregates, calls, and classifications form an opaque node and generic-edge graph |
| `b726df258` | authored DL7 products and sums generate serde Rust types through a Soopy-owned region |
| `fdb4fa5ac`, `6e7f19c61`, `bc6fba7ea` | `dd-runner` exposes a native construction API and generated Rust modules construct plans without an embedded or decoded program string |
| `1b9683bb6` | a DL7-generated transitive-closure module entered the full runtime shootout |
| `f487b5c12` | `extract watch` snapshot rows reach a resident DL7 program and derive an exact source-intelligence result |
| `eaf1954e3` | checked DL7 types and positive map/join operators lower to executable SQLite DDL, decoded reads, grouped SQL rules, and the complete tick order |
| `211231cda` | generated Rust carries both RAM constructors and SQLite statements; the SQLite arm entered the chain/ring shootout with exact closure counts |
| `1fe5406b9`, `e216bf4ca` | a persistent SQLite runner accepts snapshot and delta watch generations transactionally, survives process restart, suppresses duplicate additions, and emits derived retractions |
| `bc5807cbe` | the persistent store catalogs generated DDL, reads, rules, and tick phases and rejects drift before retained rows change |
| `bae03a4a6` | normal single-file and project compilation load the checked standard macro library, expand `<+` before module lowering, and retain filesystem colon traversal |
| `7d8e58cf6` | checked macro programs are compacted to their protocol and rule cone, standard bootstrap uses kernel relations alone, and indexed literal heads bypass empty expansion closure |

Current native construction uses the operational map/join lowering in
`1a_dbsp_plan_emitter.pl`; the DL7 emitter independently derives and tests the
structural DBSP graph. Moving binding selection, equality predicates, literal
filters, aggregate operators, and projections from that Prolog renderer into
DL7 relations is the next lowering cut. Generated source constructs the RAM
plan types plus SQLite DDL, decoded reads, SQL rule bundles, and tick phases.
The resident SQLite path has transactional generations, persistent state, and
a strict runtime catalog. Per-source Rust module partitioning,
content-addressed compilation, catalog migration, and generation-boundary
library reload have no implementation receipt yet.

The 2026-09-05 `just compiler-perf` run passed diagnostics and warm output
equality. The warm compile used 2,339 inferences and 418 ms. The cold run used
172,420,501 inferences and 48,831 ms, returned 15,542 compiler rows, and recorded
8 closure rounds. The pinned rails remain 88,000,000 inferences, 12,716 rows,
and 7 rounds, so the command exits 1. The indexed macro no-op receipt is
separate from this compiler-fixpoint regression.

<!-- todo(feature): Move operational binding, equality, literal-filter, aggregate, and projection lowering from 1a_dbsp_plan_emitter.pl into the existing DL7 DBSP graph. -->

<!-- todo(feature): Move infix-colon normalization into v7/macrotime/0_standard.dl7, retire dl7_syntax_rewrite/3 after parity receipts, and derive project-authored macro visibility from module graph edges. -->

<!-- todo(feature): Extend the proven macro-definition/invocation/output/template identity formula from a generated atom to quasiquoted form trees with node and sequence splices. -->

<!-- todo(feature): Derive the Rust and SQLite layout, SQL statements, catalog hashes, and generated Rust items from the DBSP graph in DL7. -->

<!-- todo(feature): Replace the lowerer-special `Host` target with normal callable declarations plus hosted declaration-edge and output field-edge annotations derived by DL7 rules. -->

<!-- todo(feature): Define the recursive userland List graph and prove its comptime values, Rust/SQLite cells, interning, equality, and JSON array boundary. -->

<!-- todo(feature): Emit one Rust module per loaded DL7 file and compile the generated project into a content-addressed dynamic library. -->

<!-- todo(feature): Implement generation-boundary dynamic-library reload while retaining the resident SQLite connection and arrival queue. -->

<!-- todo(feature): Express desired source membership as DL7 rules over hosted filesystem revisions; the signed sprefa-extract watch wire and resident runtime receipt are complete. -->

<!-- todo(feature): Extend the current comptime level-clock query with generation, latest, pre-state, edge-trigger, carry, and ordered difference receipts. -->

<!-- todo(feature): Derive HTTP client, HTTP server, and CLI boundaries from the proven generic hosted source and sink contracts. -->

<!-- todo(feature): Define the JSON-capable constraint graph and gate JSON Schema 2020-12 import, validation, and emission against the official category corpus. -->

<!-- todo(perf): Add content-addressed Rust compile and dynamic-load measurements beside the generated-Rust and SQLite runtime arms now present in the relational shootout. -->

## Verification

The arc closes with these executable receipts:

1. A nested colon fixture proves the exact `File -> A -> a -> X|Y` graph and
   proves that `A.a.X` remains one atom.
2. A two-file fixture resolves one module and one selected declaration through
   colon edges without source-level import syntax.
3. A DL7 emitter derives output from `program_relation`, `program_rule`,
   `program_goal`, `program_apply`, and `program_argument` input calls.
4. DBSP rows round-trip one seed, map, join, recursive SCC, aggregate, negation,
   addition, and retraction case.
5. The generated Rust arm and SQLite arm produce the same relation snapshots
   and signed differences for every DBSP fixture.
6. Generated files have a one-to-one source-module manifest and deterministic
   bytes across two clean emissions.
7. A source edit builds a fresh library, completes the active tick, applies the
   catalog action, swaps, and emits the next generation without restarting the
   host.
8. A failed source edit leaves the prior library and database state active.
9. A watcher run over a temporary checkout observes create, edit, rename, and
   removal as signed source and TSI relation changes.
10. The generated Rust and Rust/SQLite arms appear in the existing shootout
    result table with current measured statistics.
11. Temporal fixtures compare ordered signed differences for level closure,
    keyed replacement, same-generation `pre` folding, `latest` sampling,
    delayed recurrence, departure, and retention.
12. A compile-time HTTP request and runtime HTTP request use the same hosted
    relation declaration with phase-scoped demand identities, and the runtime
    reaches quiescence after all host responses and `Time.Next` carry drain.
13. HTTP server and CLI fixtures correlate ingress with egress and prove that no
    socket or process wait holds a SQLite write transaction.
14. Reader forms reify to exact indexed syntax edges, named variable occurrences
    share one logical identity, anonymous variables remain distinct, and macro
    output identities remain stable across repeated expansion rounds.
15. A `match` form expands through the generic form protocol into exact ordinary
    rules, leaving no match operation in the checked runtime program.
16. Closed-sum match fixtures prove exhaustive disjoint variants; guarded
    first-match fixtures prove ordinal fallthrough through antijoin.
17. A `Scan` fixture folds three same-generation inputs in occurrence order and
    emits all three intermediate states, then reproduces them through generated
    Rust and SQLite execution.
18. A nested `SourceRevision -> Span -> Node` arrival interns children before
    parents, stores reference IDs, shares equal span values, retains distinct
    node kinds at one span, and reconstructs an explicitly selected projection.
19. Repeated text across unrelated relations occupies one shared dictionary row;
    relation rows store the same local ID and boundary rendering returns the
    original content.
20. `Console.Stdout` and `Console.Stderr` print equal text once per distinct
    committed occurrence, suppress a derivation retracted before commit, retain
    deterministic order within each stream, and do not repeat after a compatible
    generated-library reload.
21. A proper cons chain derives `Proper`; a tail cycle, improper tail, duplicate
    tail, and conflicting sum variant each produce the exact graph diagnostic.
22. A JSON-annotated host boundary round-trips its selected value while the same
    unannotated type remains free of JSON storage or validation semantics.

CI coverage added by this arc consists of DL7 PLUnit compiler/emitter tests,
Rust ABI and reload integration tests, SQLite migration tests, and shootout
parity tests. Live filesystem and GitHub tests remain separately gated.

## Staffing

- Planning and first compiler seam: primary Codex session, current checkout,
  base `29a7761b9`.
- Independent research: Sol agent, no worktree, for the dependent DL7 ghcacher
  plan.
- Implementation slices: one branch or Boop lane per delivery step after the
  type signatures and fixture for that step are reviewed.
- Suite budget per compiler slice: focused PLUnit file, then the V7 battery.
- Suite budget per Rust slice: focused crate tests, relevant shootout cells,
  then the owning crate battery.
- Repository-wide formatting runs once immediately before each Rust commit.
