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
 0.90 s  preserve lower tables, omit dormant reference state, table cycles
 0.68 s  merge stratum closures, reuse complete source checks, prime checks
 0.52 s  bucket strata and index stratification, origins, binds, and names
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

### Call-subsumptive native tables

Declaring the nine recursive native predicates with `as subsumptive` crashed
SWI-Prolog 10.0.2 in `$tbl_wkl_add_answer/4` while evaluating the shared
four-argument `:` relation for `2_partial.dl7`. Variant tables remain the
verified mode for these generated predicates.

### Per-stratum dynamic-clause compilation

Calling `compile_predicates/1` after each stratum became immutable increased
the representative cold wall time from about 533ms to 547ms. Compilation cost
exceeded the remaining proof-time reduction for these short-lived predicates.

### Per-aggregate relation indexes

Building an association index over the 6,000-plus completed lower rows before
each aggregate stratum increased cold compilation from about 534ms to 705ms
and from 5.46M to 6.98M inferences. Aggregate proof scans are smaller than the
cost of rebuilding five full indexes per evaluator snapshot.

### Native aggregate proofs

Calling native Prolog predicates for aggregate bodies exposed duplicate proof
paths for the same tuple. Datalog aggregates consume a set of tuples, so raw
Prolog proof counts changed the result from 2 to 4. Wrapping each native call
with `distinct/2` restored the exact 6,774-row closure, but increased cold
compilation from about 534ms and 5.46M inferences to 664ms and 6.48M
inferences. The row-list aggregate evaluator retains tuple-level set semantics
at lower cost for this workload.

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

### Native table lifetime and scope

The first native evaluator tabled every relation, then abolished every
completed lower table before each higher stratum. Each relation receives all
of its clauses in exactly one stratum. Retaining completed lower tables reduced
the representative cold inference count from 10,531,346 to 8,772,458.

The generic reference evaluator now installs its dynamic rules, seeds, and
lower rows only when `DL7_VERIFY_EVALUATOR=1`. Native negative goals call the
already-completed lower native predicate. Restricting tabling to the nine
relations on positive dependency cycles produced this checkpoint:

```text
cold wall               about 895 ms
cold inferences          8,628,977
warm wall                    6 ms
exact rows                   6,774
exact rounds                     7
```

### Ordered stratum closure

Each stratum previously appended its current rows to the complete lower
closure and sorted all rows again. Lower rows are already an ordered set.
Sorting only current rows and applying `ord_union/3` reduced the representative
cold wall time from about 875ms to about 826ms. Prolog inference count rose
because ordered-set merge is visible Prolog recursion; wall time is the metric
for this allocation and sorting change.

### Complete source and rule-check reuse

A strict authored-environment probe now distinguishes complete bootstrap
lowerings from source that genuinely waits for generated callables. The
complete case reuses the initial checked source and appends generated relation
declarations. `2_partial.dl7` final source checking fell from about 115ms to
4ms.

The initial checked program also primes the resolved-rule cache after its
relation and rule lists are canonicalized. Round 1 now reuses the initial
dependency and stratum proof instead of spending another 49ms and 608,406
inferences.

```text
cold wall               about 682 ms
cold inferences          6,913,124
exact rows                   6,774
exact rounds                     7
```

Generated-program assembly reads only its semantic input relations: kernel
`def`, `head`, `body`, and `:` rows plus rows of the bound `Apply`, `Literal`,
and `Variable` relations. This replaces repeated scans over unrelated compiler
closure rows while preserving the public assembler input contract.

### Indexed stratum planning and source checking

The evaluator originally filtered every rule and every seed once for each of
seven strata. Bucketing both lists once per snapshot reduced the representative
cold compiler from about 671ms to 638ms. Positive-cycle discovery costs about
4.6ms for a new rule graph and is cached by canonical rules.

Stratification relaxation originally scanned every dependency for every
relation and linearly searched the complete level vector. Grouped dependency
constraints plus association indexes reduced isolated `stratify_rules/3` from
about 44.7ms and 550,110 inferences to about 10.3ms and 113,355 inferences.

The source checker now builds immutable association indexes for:

```text
source origin key             -> reader node
(owner, bind name)            -> target
child owner                   -> parent owner
(owner, bind name) seen set   -> duplicate detection
(owner, position) seen set    -> duplicate detection
owner                         -> edge count
```

The initial check fell from about 77ms to about 29-43ms on the representative
fixture. The current cold checkpoint varies around 520-540ms with 5,462,404
inferences, 6,774 rows, and seven rounds.

### Precise generated-program slice cache

A prototype chased head/body applications to only their argument edges and
literal/variable nodes, then cached assembly by that slice. Carrier rows kept
changing through the final compiler tick, so no useful cache hit occurred.
Repeated closure scans doubled assembly inferences from about 63,000 to
137,000 per round without reducing its 4-6ms wall time. The single-pass
semantic relation filter remains in use.

### Direct HistoryV1 variable interning

Replacing HistoryV1's `Variable/3` calls with direct canonical list and
`intern/3` construction preserved 6,774 rows and 129 runtime rules but did not
reduce the seven-round snapshot chain. The delayed edge or intern lies on a
different dependency path, so the duplicated construction was removed.

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

Current normal-mode gates are 750ms cold wall, 7,000,000 cold inferences,
50ms warm wall, and 50,000 warm inferences. The generic reference oracle is a
semantic check and intentionally exceeds normal-mode performance budgets.
