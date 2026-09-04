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
  compiler view. A DL7 emitter currently receives only `CompilerFacts`, so it
  cannot derive output from the reified checked program.
- `v7/src/3_emit/1a_dbsp_plan_emitter.pl` currently builds a JSON dictionary in
  Prolog for `v6/dd-runner`. This is a prototype endpoint rather than the
  planned DL7-authored lowering seam.
- `v6/dd-runner/src/kernel.rs` is a RAM-resident fixed-point evaluator over
  JSON values. It has no Differential Dataflow dependency. The command reports
  `--dd-rust-dd` as unbuilt in `v6/dd-runner/src/main.rs`.

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
13. Reactive execution has six kernel facts: generation identity, signed row
    differences, level rules, edge rules, current/pre-state sampling, and an
    atomic commit boundary. Clocks, retries, HTTP, servers, CLIs, filesystem
    watching, and retention are programs and hosted implementations over those
    facts.
14. Wall-clock time enters through hosted rows. Generated code has no implicit
    wall-clock read. Fixpoint rounds, runtime generations, and wall-clock values
    remain separate identities.
15. The same `Host` relation form serves compiler and runtime evaluation. Its
    phase follows the evaluator that consumes the hosted relation. A relation
    reachable from both evaluators has phase-scoped demand and response state.
16. DL7's JSON-capable type graph must represent every JSON Schema 2020-12
    validation shape used at HTTP and CLI boundaries. JSON Schema import and
    emission are DL7 programs over that graph.

Disposition of earlier paths:

| Path | Disposition |
| --- | --- |
| `ProgramJson` inside generated Rust | V6 compatibility and debug output |
| `dd-runner` RAM kernel | shootout arm and semantic oracle |
| actual Differential Dataflow | optional backend after it enters the same shootout |
| one dylib per DL7 file | deferred until a measured need for independent linking |
| one project dylib containing one Rust module per DL7 file | first native reload artifact |
| source-level `import` and `export` keywords | deferred until colon-edge composition proves insufficient |

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

The author may bind an intermediate result once the ordinary relation used for
edge projection has a `return` edge:

```dl7
(: Member
   (* (: owner type)
      (: label any)
      (: return type)))

(<- (Member ?Owner ?Label ?Target)
    (: ?Owner ?Label ?Target ?Index))
```

A derived bind such as `(User: (Member accounts 'User))` then lowers through
the existing expression-bind path in `0_lowerer.pl`. This spelling needs a
focused compiler receipt before it becomes documentation for module aliases.

An export policy can address the binding edge itself:

```dl7
(<- (: ?Binding visibility Public 0)
    (PublicBinding ?Owner ?Label)
    (edge_ref ?Owner ?Label ?Binding))
```

The edge identity remains available for any other annotation. The resolver can
join the visibility relation when a project chooses restricted traversal.

## Type signatures

The first implementation signatures are grouped by layer.

```prolog
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

## Relational lowering flow

```text
DL7 files
  -> reader trees and source rows
  -> colon-owned type and module graph
  -> checked Datalog
  -> reified program calls
  -> DL7 DBSP rules
  -> DBSP relation and operator rows
  -> DL7 Rust/SQLite rules
  -> layout, SQL, schedule, and Rust-item rows
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

### Minimal temporal basis

DL6 provides an executable specification for the initial DL7 semantics:

| Construct | Runtime meaning | Dependency row |
| --- | --- | --- |
| `<-` | close a level relation within the current generation | positive or negative, grade 0 |
| `<+` | fire once for each ordered trigger occurrence and write keyed state or history | trigger grade 0 or 1 |
| bare positive edge-body atom | trigger source | read ring `z` |
| `latest(Rel(...))` | sample current visible state without becoming a trigger | state sample, grade 0 |
| `pre(Rel(...))` | sample persistent state after earlier writes in this generation, with prior level rows frozen | previous sample, grade -1 |
| signed boundary difference | add, replacement pair, retention removal, or departure | generation boundary |

The old ring names `b`, `z`, and `n` are compiler representation details. DL7
reifies their meanings as relation plane, occurrence plane, and history plane.
The reified dependency row carries source relation, target relation, read plane,
write plane, sign, grade, and role. Clock checking is a DL7 analysis over those
rows.

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

### Surface disposition

The temporal basis does not require a family of stream operators:

- `match` expands each arm to an ordinary `<-` or `<+` rule with the scrutinee
  prepended to its body. Exhaustiveness is a checker over sum edges.
- Accumulation uses a keyed state relation, an optional history relation, and an
  edge rule reading `pre`. DL6 reserved the word `scan` and added no scan form.
- `latest` and `pre` remain body operations until a DL7 program demonstrates a
  smaller relation-only expansion with identical ordering behavior.
- `files` and `files_at` are hosted relations for live-worktree and pinned-revision
  enumeration. The old `scan` word does not name filesystem traversal.
- `next`, `await`, and conditional routing can be ordinary registered relations
  over generation-scoped rows. Retraction cancels pending work while an effect
  is still pending; an effect already performed needs an explicit compensation
  relation.

The DL7 reader spelling for match arms and temporal sampling remains outside this
runtime arc. The checked-program rows carry rule kind and body-operation identity,
which allows syntax experiments without changing the DBSP or Rust/SQLite schemas.

### Clock inputs

A clock host emits rows such as `ClockTick(clock, bucket, instant)` and retracts
generation-scoped tick rows on the following generation. Period, deadline,
debounce, cadence, and retry policies are ordinary relations joining those rows.
Generated Rust reads the supplied instant or bucket value. Replay therefore uses
the recorded clock relation and does not consult the machine clock.

## Hosted HTTP, servers, and CLIs

`Host` already lowers a callable product into `Hosted` and `HostPort` rows. Empty
input or output products extend that same contract to sources and sinks:

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

## JSON Schema coverage

JSON compatibility is a capability of an ordinary type node. The graph needs
one representation for each JSON Schema 2020-12 validation family:

| JSON Schema family | DL7 graph representation |
| --- | --- |
| object properties and required names | product edges plus required/optional constraint edges |
| open objects and additional properties | one rest-value edge carrying its value schema |
| arrays, tuples, `prefixItems`, and `contains` | collection node plus ordered item and containment constraints |
| `enum` and `const` | literal alternatives or a literal constraint edge |
| `anyOf` and `oneOf` | sum edges plus selection constraint |
| `allOf` and `not` | intersection and exclusion relations over schema identities |
| `$ref`, `$dynamicRef`, `$defs`, recursion | ordinary identity edges, retaining URI and anchor as data |
| number and string bounds, pattern, format | constraint edges on the scalar node |
| `if`/`then`/`else`, dependent schemas, dependent required | validation rules over product edges |
| `unevaluatedProperties` and `unevaluatedItems` | post-composition coverage constraints |
| `null` versus absence | `null` value node versus an optional product edge |

Constraint identity is reified, so annotations can target the constraint edge,
the field edge, the enclosing product, or the boundary relation. DL7-specific
capabilities may additionally describe relation keys, temporal retention, host
ports, provenance, and graph constraints. JSON Schema import produces these
ordinary nodes and edges. Emission succeeds for the representable JSON subset of
a DL7 graph and returns diagnostic rows for capabilities with no JSON Schema
encoding.

The compatibility gate uses the official JSON Schema test suite categories plus
round trips through imported and emitted schemas. HTTP request and response body
validation runs at the host boundary before rows enter the program or bytes leave
the host.

## Delivery sequence

1. Convert `logical_program_rows/2` output into declared DL7 relation calls and
   add a second emitter evaluation that receives the complete compiler view.
2. Re-express the current positive map/join projection from
   `1a_dbsp_plan_emitter.pl` as DL7 rules. Compare its rows with the current
   JSON prototype on the existing fixture.
3. Add SCC, recursion, aggregate, negation, and retraction cases. Every supported
   operator enters the `dd-runner` shootout before the Rust/SQLite emitter uses
   it.
4. Define the minimal target-neutral relation layout rows: stored role,
   ordered columns, semantic representation, and keys.
5. Derive SQLite table identities, DDL, statements, and reload catalog hashes in
   DL7.
6. Restore a generalized form of the compiled Rust technique represented by
   `v6/labs/exec_shootout/mono/src/main.rs`. Emit one Rust module for each DL7
   file and direct operator functions rather than a runtime-plan string.
7. Build one static executable first, then place the same generated project
   behind the `Dl7ModuleV1` boundary.
8. Add a resident host that watches sources, incrementally compiles a fresh
   content-addressed library, validates it, and swaps at a generation boundary.
9. Reify rule kind, temporal body operations, dependency sign/grade, and clock
   diagnostics; implement level closure, ordered edge firing, current/pre-state
   reads, and generation carry in the shootout.
10. Feed `sprefa-extract` watch changes into the active runtime and persist their
   relational state in SQLite.
11. Move source-membership selection from the CLI path list into DL7 rules over
    hosted filesystem observations, retaining the entry file and prelude as the
    bootstrap roots.
12. Add generic hosted source and sink relations, then exercise HTTP client,
    HTTP server, and CLI request/response lifecycles over them.
13. Add JSON Schema import, validation, and emission rows after the product, sum,
    reference, optional-edge, open-object, and constraint identities are stable.

<!-- todo(feature): Convert the checked-program reifier output into declared DL7 calls and evaluate DL7 emitters against the complete compiler view. -->

<!-- todo(feature): Re-express the current DBSP prototype as DL7 relations and rules, with source relation identities preserved independently of authored labels. -->

<!-- todo(feature): Derive the Rust and SQLite layout, SQL statements, catalog hashes, and generated Rust items from the DBSP graph in DL7. -->

<!-- todo(feature): Emit one Rust module per loaded DL7 file and compile the generated project into a content-addressed dynamic library. -->

<!-- todo(feature): Implement generation-boundary dynamic-library reload while retaining the resident SQLite connection and arrival queue. -->

<!-- todo(feature): Express desired source membership as DL7 rules over hosted filesystem revisions and feed sprefa-extract watch changes into the resident runtime. -->

<!-- todo(feature): Reify the minimal temporal dependency algebra and port the DL6 generation, latest, pre-state, edge-trigger, carry, and clock receipts to DL7. -->

<!-- todo(feature): Define generic hosted source and sink contracts, then derive HTTP client, HTTP server, and CLI boundaries without protocol-specific kernel syntax. -->

<!-- todo(feature): Define the JSON-capable constraint graph and gate JSON Schema 2020-12 import, validation, and emission against the official category corpus. -->

<!-- todo(perf): Add the generated Rust and Rust/SQLite arms to every relational shootout and record compile latency, load latency, peak RSS, tick latency, and SQLite file size. -->

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
10. The generated Rust, RAM kernel, and Rust/SQLite arms appear in the existing
    shootout result table with current measured statistics.
11. Temporal fixtures compare ordered signed differences for level closure,
    keyed replacement, same-generation `pre` folding, `latest` sampling,
    delayed recurrence, departure, and retention.
12. A compile-time HTTP request and runtime HTTP request use the same hosted
    relation declaration with phase-scoped demand identities.
13. HTTP server and CLI fixtures correlate ingress with egress and prove that no
    socket or process wait holds a SQLite write transaction.
14. JSON Schema category fixtures import into the type graph, validate boundary
    values, and emit a schema whose accepted JSON instances match the input.

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
