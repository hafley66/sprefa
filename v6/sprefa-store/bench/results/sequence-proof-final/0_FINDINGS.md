# Exact-state update validation

Code under test: `63d8b0034`. Production algorithms and compiler semantics were
not changed. [Complete accounting](accounting.json) and `logs/` preserve every
adapter's output, exit status, and the first failing prefix.

## Current accounting

Each general-runtime fixture checks complete ordered `root`, `edge`, and
`alive` sets after every update against an independently recomputed BFS.
Updates are legal keyed-set insertions/deletions. Multiple paths and multiple
roots test duplicate derivations; inserting an already-present key is outside
this fixture contract. The 39 fixtures include deterministic seeded sequences,
diamonds, disconnected graphs, cycles, self-loops, and final-root removal.

| Adapter | Complete fixture passes | Failing fixtures | Unsupported or partially supported fixtures | Verified tick states |
|---|---:|---:|---:|---:|
| native Differential Dataflow | 39 | 0 | 0 | 1129 |
| SWI incremental | 39 | 0 | 0 | 1129 |
| native PostgreSQL recomputation | 39 | 0 | 0 | 1129 |
| actual emitted TSV2 runtime | 39 | 0 | 0 | 1129 |
| actual emitted Rust runtime | 39 | 0 | 0 | 1129 |
| SQLite count | 10 | 0 | 29 | 32 |
| SQLite count-SCC | 18 | 0 | 21 | 61 |
| SQLite DRed loop | 18 | 0 | 21 | 61 |
| SQLite DRed CTE | 18 | 0 | 21 | 61 |
| SQLite signed-delta-v2 | 0 | 21 | 18 | 38 |

Total: 390 adapter/fixture cases, 259 complete passes, 21 failures, 110
unsupported or partially supported cases, and 5,898 verified tick states.
Unsupported fixtures may have a checked prefix before the unsupported update.
They are not counted as complete passes. PGlite and pg_ivm are outside this
update-sequence run; their one-deletion benchmark statuses are separate.

RelStore's specialized adapters accept static dependency graphs with initialized
support weights, then root deletions. The graph's source-root rows encode root
presence, and dependency rows are checked after every supported deletion.
Plain counting is DAG-only. Edge mutation and root reinsertion are explicitly
unsupported by these comparison adapters: `add_deps` and `assert` do not supply
the same general root/edge maintenance contract as the emitted runtimes.

## Exhaustive bounded DAG check

`perf_report`'s `exhaustive_four_node_dag_counting_states_and_weights` passed:
[raw test output](1_count-exhaustive.log).

```text
cases=2048 exact_states=6144 weight_checks=24576 nodes=4
DAG_edge_sets=64 root_sets=16 deletion_orders=2
```

It enumerates all 64 subsets of the six forward edges on four ordered nodes,
all 16 root subsets, and ascending/descending root-deletion orders. Every state
checks the complete alive set and each stored weight against:

```text
weight(node) = root_present(node) + number_of_alive_parents(node)
alive(node)  = weight(node) > 0
```

For this DAG model, the algorithmic argument follows the topological order:
removing one root contribution reduces its weight by one; an alive-to-dead
transition removes one contribution from each child. A child propagates only
when its weight crosses zero. A DAG has no self-supporting cycle, so this
induction yields the recomputed root-reachable set. The finite executable
checks validate the implementation on the enumerated inputs. They are not a
formal verification of the implementation for every graph or update contract.

## Two-node counterexamples for signed-delta-v2

[Repeated deletion fixture](fixtures/two-isolated-roots.json),
[actual output](logs/sqlite-signed-delta-v2-two-isolated-roots.stdout):

| Update | Expected alive | Actual alive |
|---|---|---|
| insert roots 0 and 1, no edges | {0,1} | {0,1} |
| delete root 0 | {1} | {1} |
| delete root 1 | {} | {0} |

[Disconnected zero-weight fixture](fixtures/disconnected-zero-weight-node.json),
[actual output](logs/sqlite-signed-delta-v2-disconnected-zero-weight-node.stdout):
two stored nodes, root 0 only, no edges; deleting root 0 yields `{1}` where
BFS requires `{}`. Removing the second node eliminates this counterexample;
no edges can be removed. These are explicit two-node reductions, without a
claim of globally minimal reproduction across all possible API inputs.

`src/engine.rs:925` implements this function with one full recursive walk.
Its seed query at lines 945 onward selects all zero-indegree rows except the
current deletion argument, without requiring positive weight or remembering
previously deleted roots. The final statement replaces every row's weight
with a Boolean alive mask. The original benchmark's one deletion over its
fully reachable graph passes. The generalized current-root sequence contract
fails. The benchmark metadata now states this restriction; the production
implementation is unchanged.

## Emitted Rust execution evidence

The call-tree skill directed inspection of the actual entrypoints and existing
instrumentation:

```text
drive_tick_transacted(program, seam, arrivals)
  -> GenProgram::run_tick(seam, arrivals)
     -> incremental evaluation
        -> reconcile_ref_count_statement
           -> expand_sql seed and alternating rounds [SQLite batches]
           -> support reconciliation [SQLite batch]
```

The emitted program carries `dred_sql`, whose Rust references are the type and
field definitions in `src/types.rs`. TSV2 reads that field and invokes its DRed
maintenance path. Rust's existing trace scopes confirm recount execution:
[cycle fixture trace](logs/sprefa-engine-rs-cycle-final-external-root.stdout),
tick 1, reports 58 tick SQL statements, 48 attributed to `recount/alive`.
The diagnostic trace is enabled only for sequence tests and is excluded from
the repeated benchmark runs. This establishes the executed path. It does not
establish that the missing DRed consumer causes any measured latency gap.

A subsequent lab refinement, `fab3e8f2c`, makes the DD sequence barrier wait for
both observed input collections as well as the output. All 39 fixtures and
1,129 states passed again in `../sequence-proof-dd-frontiers/`. It changes no
timed benchmark path or production algorithm.

## Reproduction

From `v6/sprefa-store`, with existing compiled examples:

```bash
BENCH_SEQUENCE_OUT=bench/results/NEW-SEQUENCE-DESTINATION \
  node --test bench/6_reach_sequence.test.mjs
```

The PostgreSQL arm requires the existing disposable cluster helper to be
started in the same shell and stopped with its EXIT trap. The checked run's
cluster logs are in `../sequence-proof-final-cluster/`. For only the two emitted
runtimes, set `BENCH_SEQUENCE_FILTER='tsv2-runtime sprefa-engine-rs'`.
The full suite currently exits nonzero for the retained signed-delta-v2 failures.

```bash
cargo test --offline --release --example perf_report \
  exhaustive_four_node_dag_counting_states_and_weights -- --nocapture
target/release/examples/perf_report --sequence sqlite-signed-delta-v2 \
  bench/results/sequence-proof-final/fixtures/two-isolated-roots.json
```

The earlier `sequence-proof-first` receipts retain a SWI JSON serialization
defect in the new test adapter. That lab-only defect was corrected before
`sequence-proof-all` and this final run; those historical receipts remain intact.
