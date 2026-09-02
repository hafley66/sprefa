# DL7 compiler performance model

## Contents

1. Representative checkpoint
2. Cost topology
3. Wins already banked
4. Measured rejected designs
5. Requirements for the next evaluator
6. Commands and gates

## 1. Representative checkpoint

Fixture: `v7/test/fixtures/2_partial.dl7`.

Input size:

```text
DL7 prelude        1,023 lines
user fixture          52 lines
compiler rows      6,774
runtime relations     92
runtime seeds         320
runtime rules         129
compiler rounds         7
```

Checkpoint at commit `c8a03c19b` on 2026-09-02:

```text
DL7 cold compiler       about 1,190 ms   10,740,881 inferences
DL7 warm compiler       about    13 ms        1,826 inferences
DL6 flagship compiler             269 ms    3,584,046 inferences
DL6 flagship process wall         470 ms
```

Wall time varies. Inference counts are the deterministic regression meter.

## 2. Cost topology

Representative cold DL7 phase costs:

```text
parse and expansion             45-65 ms
initial lowering                  23 ms
initial check                    110 ms
seven evaluator calls        about 500 ms
two distinct round checks     about 100 ms
seven assembly passes          70-80 ms
final source lower and check     145 ms
final key validation               6 ms
type graph projection            2-3 ms
final runtime check                 2 ms
```

The closure contains about 6,121 level-zero rows. Snapshot deltas are small,
but `edge_snapshot/4` feeds the shared `:/4` type-edge relation. A
relation-granularity dirty cone therefore reaches 107 to 122 of 129 rules.

## 3. Wins already banked

Performance history for the same fixture:

```text
14.20 s  original checkpoint
 9.83 s  evaluator state reuse across strata
 4.75 s  native relation-specific tabled predicates
 2.56 s  query only the current stratum
 1.70 s  indexed source expansion
 1.20 s  repeated rule-check caching and indexed validation
 1.12 s  reuse the environment-independent prelude during source refreeze
```

Two dominant defects were removed:

1. Expansion appended the complete available source prefix through siblings
   and rewrite waves. `append/3` and garbage collection dominated. A
   root-first source index reduced prelude expansion from about 940ms and
   13.5M inferences to about 46ms and 410K inferences.
2. Each compiler round evaluated seven strata, and every stratum queried all
   completed lower relations. This formed 49 oversized closures. Querying only
   current-stratum relations removed the repeated lower-row enumeration.

## 4. Measured rejected designs

Do not reintroduce these shapes without a materially different mechanism.

### Generic incremental evaluator

```text
result       6,773 rows, semantically wrong
cold         13.44 s
warm            566 ms
```

### SWI monotonic or incremental level-zero tables

```text
monotonic cache       about 1.43 s cold, 40 ms warm
incremental cache     about 1.43 s cold, 29 ms warm
warm inferences       216,850 in the incremental version
```

Incremental dependency-graph maintenance and cleanup cost more than rebuilding
this closure.

### Reusable compiled native clauses

Rules were compiled once with evaluation identity as a hidden first argument.
The result was exact under the reference oracle.

```text
before        about 1.20 s
after         about 1.10 s
source cost   152 added lines
```

Rule assertion owns about 100ms. The lifecycle and cache complexity did not
address the dominant row recomputation.

### Relation-level dirty cone

New snapshot rows dirty the shared edge relation. The transitive relation cone
contained 107 to 122 rules, so slicing by relation did not reduce evaluation.

### Interpreted row-level semi-naive loop

An exact prototype retained prior level-zero rows, drove rule variants from
new rows, and recomputed strict strata. `DL7_VERIFY_DELTA=1` matched all seven
full snapshot closures after transient demand rows were excluded.

```text
full evaluator round 2       about  67 ms
delta prototype round 2      about 229 ms
full cold compiler           about 1.19 s
delta cold compiler          about 1.37 s
```

The prototype asserted about 6,100 prior rows into SWI, interpreted every rule
variant, then asserted the rows again for strict-stratum evaluation. Correct
semi-naive algebra alone did not provide a faster execution substrate.

## 5. Requirements for the next evaluator

A useful row-delta implementation needs both:

1. compiled indexes or a persistent dataflow instance, so prior closure rows
   remain resident across compiler ticks;
2. demand-call support, so bound `cons`, `intern`, `contains`, constructors,
   and similar relations remain callable without materializing transient proof
   rows.

Snapshot behavior still requires:

```text
positive level-zero additions  -> delta propagation
seed removal                   -> exact retraction or full rebuild
rule graph change              -> program rebuild or full snapshot fallback
negation and aggregation       -> recompute affected strict strata against a
                                  completed lower snapshot
```

The full evaluator remains the correctness oracle. Compare complete closures
and diagnostics before banking a performance result.

### Prelude refreeze reuse

The initial prelude basement and origins are retained in `compile_context`.
Final source refreeze reuses those rows and strictly relowers importer units
against the prelude plus generated environment.

```text
before final source check     about 145 ms
after final source check      about 118 ms
before cold inferences        10,740,881
after cold inferences         10,531,346
exact rows                    6,774
exact rounds                  7
```

## 6. Commands and gates

Run the representative checkpoint:

```bash
cd v7
just compiler-perf
```

Run the evaluator oracle on a fixture:

```bash
cd v7
DL7_VERIFY_EVALUATOR=1 \
  swipl -q -s bench/0_compiler_performance.pl -- \
  test/fixtures/2_partial.dl7
```

Inspect detailed compiler steps:

```bash
cd v7
DL7_TRACE=steps swipl -q -g \
  "use_module(src/'2_comptime'/'2_compiler'), \
   use_module(src/'2_comptime'/'1c_compiler_cacher'), \
   dl7_compiler_cacher:clear_compiler_caches, \
   dl7_compiler:compile_dl7('test/fixtures/2_partial.dl7',_,_,_), \
   halt"
```

The checkpoint gate must retain exact row and round counts. Tighten wall and
inference budgets only after a measured implementation lands.
