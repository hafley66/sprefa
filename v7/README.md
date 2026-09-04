# Sprefa V7

V7 is a fresh `.dl7` language and SWI-Prolog compiler. DL6 is a donor corpus for
semantic predicates, tests, rulings, and execution-plan contracts.

The first arc is a predicate-level DL6 reuse audit. Its receipts live under
`1_AUDIT/`. Source compatibility and a second maintained DL6 frontend are
outside this arc.

- [Donor audit index](1_AUDIT/results/0_INDEX.md)
- [Kernel reconciliation](2_DESIGN/0_KERNEL_RECONCILIATION.md)
- [Minimal programmable kernel plan](2_DESIGN/1_MINIMAL_VERTICAL_SLICE.PLAN.md)

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

The watcher uses Soopy's event stream where filesystem registration succeeds.
If the platform rejects a recursive watch, for example because the checkout
contains a dangling symlink, it retains the same generation protocol through
Soopy snapshot diffs at `--poll-ms 500`.

The Rust DBSP kernel participates in the chain and ring runtime shootout:

```bash
cd v7
just runtime-shootout-smoke
```
