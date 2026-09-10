# Sprefa V7

V7 is a fresh `.dl7` language and SWI-Prolog compiler. DL6 is a donor corpus for
semantic predicates, tests, rulings, and execution-plan contracts.

The first arc is a predicate-level DL6 reuse audit. Its receipts live under
`1_AUDIT/`. Source compatibility and a second maintained DL6 frontend are
outside this arc.

- [Donor audit index](1_AUDIT/results/0_INDEX.md)
- [Kernel reconciliation](2_DESIGN/0_KERNEL_RECONCILIATION.md)
- [Minimal programmable kernel plan](2_DESIGN/1_MINIMAL_VERTICAL_SLICE.PLAN.md)

The application target is local, ref-driven code analysis and effects across
repositories:

```text
Soopy observes repository + watched ref: old commit -> new commit
  -> compare path/content identities in those immutable revisions
  -> retract removed/replaced file facts; add new/replacement file facts
  -> DL7 rules maintain cross-repository results in SQLite IVM
  -> commit the generation, then dispatch its effect changes
```

An unchanged file retains its extracted facts. Its derived analysis can still
change when a dependency changes elsewhere. Extraction reuse must include the
extractor version and configuration; equal bytes alone do not identify results
from different extraction configurations. Repository, watched-ref membership,
path, and content identity remain explicit so two repositories can contain the
same paths or bytes without their source occurrences aliasing. Commit movement
also updates revision metadata even when the selected files are unchanged.

Soopy already exposes repository ref deltas, immutable revision reads, and
path/content snapshot differences. The existing `extract watch` adapter selects
the worktree and sends signed TSI rows to the historical runner below. The newer
extract SQLite exporter creates a snapshot database. Connecting named-ref
movement to that durable fact store and committed effect delivery remains
integration work. External effect retries require durable delivery state and
effect-specific idempotency; committing analysis alone does not provide that.

DL6's child-first interning is a selectable DL7 userland storage layout: rules
over the type graph select dictionaries and product references for a renderer.
Persistent local IDs, semantic identities, and source revision membership have
separate roles. This storage choice does not change product `apply` semantics.

The [DL6 userland catalog](applications/dl6/README.md) exercises selective
`Interned text` fields, shared dictionary identity, and reference layouts.
Run `cd v7 && just dl6-demo` to print its derived fields and wrapper mapping.
Its artifacts describe storage; this example does not allocate persistent IDs.

Initial boundary under examination:

```text
.dl7 source
    -> generated Tree-sitter C parser
    -> canonical syntax adapter
    -> evaluator
    -> V7 semantic facts and fixpoints
    -> execution-plan contract
    -> existing sprefa-engine-rs
```

The first build gate is `cd v7 && just build`. It regenerates and tests the
DL7 Tree-sitter parser. The generated parser exposes a C ABI usable from C,
C++, Zig, and a later compiler-host adapter.

Every compiler entry point writes one DL6-compatible `COMPILE-TRACE` summary
to stderr. Set `DL7_TRACE=steps` for cost-sorted compiler steps, including
comptime fixpoint row counts. Set `DL7_TRACE=json` and optionally
`DL7_TRACE_FILE=/path/to/compile-trace.jsonl` for one structured object per
compile.

Run `cd v7 && just compiler-perf` for the cold/warm compiler checkpoint. It
reports wall time and enforces inference, closure-round, compiler-row, and
warm-cache output budgets on `2_partial.dl7`.

The 2026-09-05 checkpoint after standard macrotime integration reports a clean
warm cache at 2,339 inferences and 418 ms. Its cold checkpoint currently fails
three pinned budgets: 172,420,501 versus 88,000,000 inferences, 15,542 versus
12,716 compiler rows, and 8 versus 7 closure rounds. Cold wall time was 48,831
ms. This receipt remains visible while compiler-fixpoint cost is separated from
the 0.037-second no-invocation macro dispatch path.

Normal single-file and project compilation loads the numbered standard
macrotime library under `v7/macrotime/`, compiles it through the ordinary DL7
prelude, and evaluates it over each source unit before module lowering. The
standard library currently defines `<+` as a graph rewrite to `<-`.
`compile_dl7_macro_program/3` is the bootstrap entry point for compiling a
macro library without applying that library to its own source.

Run a DL7 relation over the type facts extracted from one source file:

```bash
cd v7
just query \
  examples/0_rust_traits.dl7 \
  ../v6/sprefa-extract/tests/fixtures/tsi/probe_graph.rs \
  source_trait
```

The command runs `extract --witness --family type`, exposes its dotted
`tsi.*` relations to the DL7 source module, evaluates the program, and writes
one JSON array per result tuple. `0_rust_traits.dl7` derives
`source_trait(Name)` by joining `rust.trait(Identity)` with
`tsi.name(Identity, Name)`.

Query an existing extract SQLite fact database through the target-neutral
logical-program emitter:

```bash
cd v7
just sqlite-query \
  test/fixtures/sqlite_query/0_cst_edge_kinds.dl7 \
  test/fixtures/sqlite_query/1_cst_edge_kinds.layout.json \
  /path/to/extract.db --query
```

Omit `--query` to emit a JSON artifact containing the SQL and source bindings.
Use `--install cst_edge_kinds` to create a maintained
SQLite IVM result in that database. The install command requires the native
extension built from `sqlite_ivm/`; `--ivm-extension PATH` or `IVM_EXTENSION`
selects its library. `--sqlite3 PATH` or `SQLITE3` selects the SQLite CLI.
Existing result names are rejected. Future writers must load the extension and
enable the connection pragmas documented in [sqlite_ivm](../sqlite_ivm/README.md).

The layout binds authored relation names to existing tables and ordered
columns. The emitter also accepts equivalent relational layout rows directly.
Bindings assume the extractor has validated field values; this adapter does
not enforce DL7 field types on arbitrary SQLite contents.
This cut covers positive, nonrecursive projection, constant filters,
equijoins, and unions of output rules with set semantics. Mapped rows with NULL
in any mapped column are excluded from the input relation. Unsupported rule
shapes produce diagnostics before installation.

The example joins edges to matching node kinds by input path, content identity,
family, and span boundaries. Distinct kinds can share a span, so an edge can
produce multiple matching kind pairs. The exported input path must already
distinguish source occurrences; this example does not add repository or
watched-ref identity to an export that lacks those columns.

`just sqlite-query-test` exercises the compiler, SQL boundary, and native IVM
lifecycle. `just interned-storage-test` exercises the separate userland layout
derivation. Neither command starts the named-ref watcher or dispatches effects.

The storage library is `emitters/2_interned_storage.dl7`. Application-authored
policy products select `storage_root`, `storage_dictionary`, and optional
`storage_plain` edges. Its ordinary DL7 rules derive type layouts, ordered
field layouts, dictionary/reference dependencies, reverse projections, and
identity-domain metadata from the type graph. The fixture separates moving
commit SHAs in `RefTransition` from file content identity in `SourceRevision`.
It includes two references to the same `Span` type and an unselected product.
These are layout descriptions; persistent value interning is not implemented
by this library.

The layout test compiles those rules normally, then emits the compiler-closed
layout rows through a type-graph-only runtime view. It omits unrelated
executable rules from artifact reification. Full unsliced artifact emission
returns empty layout artifacts because the `edge_snapshot` inputs are not
retained for a second evaluation. Reading `:` instead also introduces logical
rule-occurrence edges that fail functional-key validation. The tested route
uses the already-derived layout rows; the general second-evaluation route
requires separate compiler work.

The local extract integration and prepared-statement cache measurements are
recorded in [the query receipt](receipts/0_sqlite_query.md).

Emit that source-visible relation graph as the operator JSON accepted by the
resident Rust RAM kernel:

```bash
cd v7
just dbsp-plan \
  examples/0_rust_traits.dl7 \
  ../v6/sprefa-extract/tests/fixtures/tsi/probe_graph.rs \
  > /tmp/rust-traits.plan.json
```

The emitter preserves relation edge labels verbatim. `tsi.name` remains one
opaque relation name. The current executable cut covers positive projection,
selection, joins, and positive recursion. A negative rule produces an emitter
diagnostic until the resident kernel has an anti-join operator.

`sprefa-extract` can keep a checkout open as a signed TSI input stream:

```bash
v6/sprefa-extract/target/debug/extract watch . \
  --pattern '**/*.rs' \
  --family type \
| v6/dd-runner/target/debug/dd-runner \
    /tmp/rust-traits.plan.json \
    --dd-diet-rust-rust \
    --watch-stdin
```

Generation 0 is a snapshot reset. Later generations contain deletions from the
SQLite receipt for the prior source content followed by additions extracted
from the replacement content. TSI local ids are paired with the content digest
at the runner boundary. Repository and worktree identities remain on every
watch row. State defaults to the platform state directory and can be selected
with `extract watch --state PATH`.

The same signed stream can run against a persistent SQLite relation store:

```bash
v6/sprefa-extract/target/debug/extract watch . \
  --pattern '**/*.rs' \
  --family type \
| v6/dd-runner/target/debug/dd-runner \
    /tmp/rust-traits.plan.json \
    --sqlite-state /tmp/rust-traits.runtime.sqlite3 \
    --watch-stdin
```

Snapshot generations clear the program relations before applying their rows.
Delta generations update the existing SQLite state. Restarting `dd-runner`
with the same `--sqlite-state` path retains the last committed relation rows.
The `__dl7_catalog` table records the generated DDL, relation reads, initial
rows, SQL rule bundles, edge operators, and tick order. A changed plan is
rejected before retained relation rows are modified; catalog migration remains
a later runtime step.

The watcher uses Soopy's event stream where filesystem registration succeeds.
If the platform rejects a recursive watch, for example because the checkout
contains a dangling symlink, it retains the same generation protocol through
Soopy snapshot diffs at `--poll-ms 500`.

The RAM kernel, native constructors generated from DL7, and SQLite SQL
generated from the same DL7 program participate in the chain and ring runtime
shootout:

```bash
cd v7
just runtime-shootout-smoke
```

DL7 products and sums can own a generated Rust region through Soopy:

```bash
cd v7
just dl7-rust-check \
  schema/0_runtime_types.dl7 \
  ../v6/dd-runner/src/0_dl7_types.rs \
  dl7-runtime-types
```

The checked program can also generate direct `dd-runner` constructors. This
path carries no serialized program string and performs no program decode:

```bash
cd v7
just dbsp-rust-check \
  test/fixtures/12_native_runtime.dl7 \
  ../v6/dd-runner/src/2_generated_fixture.rs \
  dl7-native-runtime
```

Replace `check` with `apply` in either recipe to submit the generated body
through Soopy's expected-content stage. Bytes outside the named marker region
remain authored. `just watch-e2e` builds both resident processes and proves a
tracked checkout snapshot derives the exact DL7 result row.

`dbsp-generated` is a separate arm in the full shootout. At N=48 its current
medians are 4.078458 ms for the 1,128-row chain closure and 6.935125 ms for the
2,304-row ring closure. The generated and hand-constructed arms use the same
RAM kernel, so this measurement isolates generated plan construction and
dispatch rather than a different closure algorithm.
