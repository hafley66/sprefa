# Logadat Lab Results

Date: 2026-08-29. Upstream pin:
`23fc43cc918e0aaac2aace1410e7283ef675153a` (2025-12-20T22:18:25+07:00,
detached `HEAD`). License: MIT, `LICENSE`, Copyright (c) 2025 taarotman.
Runtime: SBCL 2.6.7 on Darwin arm64. Dependency install route: none.

The checkout, image, and temporary probe data are under
`/private/tmp/sprefa-logadat.EzvCip/`. The upstream contains no ASDF system
or dependency declaration, so no Quicklisp state was created. `2_PROBE.lisp`
loads the unchanged source into `SPREFA-LOGADAT-UPSTREAM`, an isolated package.

## Commands and raw probe results

```sh
LAB_TMP=/private/tmp/sprefa-logadat.EzvCip
git clone --filter=blob:none https://github.com/taarotman/logadat "$LAB_TMP/upstream"
git -C "$LAB_TMP/upstream" checkout --detach 23fc43cc918e0aaac2aace1410e7283ef675153a
LOGADAT_UPSTREAM="$LAB_TMP/upstream" LOGADAT_OUT="$LAB_TMP/logadat-lab" \
  sbcl --noinform --disable-debugger --script 2_PROBE.lisp
```

```text
PROBE library=logadat commit=23fc43cc918e0aaac2aace1410e7283ef675153a
UNIFY present=NIL
OCCURS present=NIL
PATH termination=naive-fixed-point timeout-seconds=2 answers=("A" "B" "C" "D")
FACTS adapter=declaration-rebuild assert=("A" "B" "C" "D") update=("A" "B" "C" "E") retract=("A" "B" "C")
UPDATE adapter=declaration-rebuild-after-retraction answers=("A" "B" "C")
DUPLICATES facts-count=2 facts=("B" "B") derived-count=1 derived=("B")
NEG host-predicate-domain=(A B C) answers=("A" "C")
DYNAMIC-API assert=NIL update=NIL retract=NIL
BINARY blocker=LOGADAT_OUT-missing-or-unreadable
```

`PATH` is the shared finite fixture: `edge(a,b)`, `edge(b,c)`, `edge(c,a)`,
`edge(c,d)`, and the two `path/2` rules. The probe wraps this cyclic query in
`sb-ext:with-timeout 2`; the completed canonical set is `A`, `B`, `C`, `D`.
The library termination mechanism is the source-level naive fixed point in
`naive-evaluation`, not an answer limit or table.

The facts receipt runs three public `LOGADAT` declarations. The second replaces
`edge(c,d)` with `edge(c,e)`; the third omits the final edge. Those receipts
show declaration rebuilding. There is no library-owned runtime assert, update,
or retract operation. Duplicate EDB rows survive a direct fact query in order;
duplicate derived rule output is removed by `eval-rules`.

`NEG` uses a finite three-node domain and a host Common Lisp boolean qualifier
inside a rule body. It returns `A` and `C` for `not-edge(a,Y)` when only
`edge(a,b)` is denied. It is not a Datalog negation, stratification, or delayed
goal facility.

## Capability classification

| Capability | Result | Runtime or source receipt |
| --- | --- | --- |
| nested term unification | absent-from-probe | `UNIFY present=NIL`; the source trace has list-comprehension pattern matching rather than a unifier. |
| occurs check | absent-from-probe | `OCCURS present=NIL`; no occurs-check function is defined. |
| multiple answers | native | `DUPLICATES facts-count=2 facts=("B" "B")`; EDB row order is retained for direct queries. |
| fair search | absent-from-probe | Evaluation is finite relation materialization in `naive-evaluation`; no lazy goal stream or search scheduler is defined. |
| cyclic transitive closure | native | `PATH` completes the four-value set under the two-second hard bound through naive fixed-point iteration. |
| Datalog fixpoint | native | `naive-evaluation` rebuilds predicate values until `predicate=` reports unchanged rows. |
| tabling | absent-from-probe | `predicate` stores only rule lists, rewritten rules, and current values; no call/answer-table or consumer code is present. |
| constraints | absent-from-probe | No constraint domain, propagation store, or attributed-variable code is defined. |
| dynamic facts and retraction | adapter | `FACTS` and `UPDATE` rebuild declarations; `DYNAMIC-API` reports `NIL` for all three mutation names. |
| standalone image | built | External SBCL executable receipt below. |

## Source trace

| Concern | Pinned source location | Behavior used by the probe |
| --- | --- | --- |
| facts | `logadat.lisp:203-223` | `facts` calls `collect-facts`; facts are validated same-arity tuple lists in an EDB hash table. |
| rules | `logadat.lisp:161-201` | `predicate` objects and `rules`/`collect-preds` retain predicate arity and rule heads/bodies. |
| evaluation | `logadat.lisp:226-291` | rewrite current relation values into body atoms, generate list comprehensions, then `remove-duplicates` derived tuples using `equal`. |
| recursion | `logadat.lisp:312-316` | `naive-evaluation` calls itself with the next predicate map until `predicate=` returns true. |
| termination comparison | `logadat.lisp:293-305` | `predicate=nilerr` compares each relation with `set-exclusive-or`; `predicate=` converts the mismatch error to `NIL`. |
| query | `logadat.lisp:325-353` | `query-eval` gets EDB or completed IDB rows and applies generated pattern matching. |
| DSL entry | `logadat.lisp:361-378` | `logadat` collects `:facts`, `:rule`, `:query`, and optional `:eval`, then expands the evaluation/query forms. |

The commented `seminaive-evaluation` draft at `logadat.lisp:318-322` calls
`naive-evaluation`; no seminaive worklist implementation executes. No library
source defines a unifier, occurs check, mutation API, table manager, answer
subsumption, or constraint store.

## Standalone image receipt

```sh
LOGADAT_UPSTREAM=/private/tmp/sprefa-logadat.EzvCip/upstream \
LOGADAT_OUT=/private/tmp/sprefa-logadat.EzvCip/logadat-lab \
  sbcl --noinform --disable-debugger --load 3_BUILD.lisp
LOGADAT_OUT=/private/tmp/sprefa-logadat.EzvCip/logadat-lab \
  /private/tmp/sprefa-logadat.EzvCip/logadat-lab
wc -c /private/tmp/sprefa-logadat.EzvCip/logadat-lab
shasum -a 256 /private/tmp/sprefa-logadat.EzvCip/logadat-lab
file /private/tmp/sprefa-logadat.EzvCip/logadat-lab
otool -L /private/tmp/sprefa-logadat.EzvCip/logadat-lab
```

```text
BINARY path=/private/tmp/sprefa-logadat.EzvCip/logadat-lab bytes=42277112
IMAGE load-function=T compile-function=T cli-eval=not-exposed
42277112 /private/tmp/sprefa-logadat.EzvCip/logadat-lab
7f92a0e11699b178595b3159505396113c8a3d1cefc58fcc0a684ed392bf5b44  /private/tmp/sprefa-logadat.EzvCip/logadat-lab
/private/tmp/sprefa-logadat.EzvCip/logadat-lab: Mach-O 64-bit executable arm64
/private/tmp/sprefa-logadat.EzvCip/logadat-lab:
    /usr/lib/libSystem.B.dylib
    /opt/homebrew/opt/zstd/lib/libzstd.1.dylib
```

The executable invocation sets `LOGADAT_OUT` only. It does not set
`LOGADAT_UPSTREAM`, and the probe still runs, which receipts that the image
contains the loaded source. `LOAD` and `COMPILE` remain fbound inside the
image. The saved-image top level accepts no command-line source/evaluation
request, so CLI evaluation is not exposed by this lab executable.

## Measurements

```sh
for sample in 1 2 3 4 5; do
  LOGADAT_OUT=/private/tmp/sprefa-logadat.EzvCip/logadat-lab \
    /usr/bin/time -p /private/tmp/sprefa-logadat.EzvCip/logadat-lab >/dev/null
done
LOGADAT_OUT=/private/tmp/sprefa-logadat.EzvCip/logadat-lab \
  /usr/bin/time -l /private/tmp/sprefa-logadat.EzvCip/logadat-lab >/dev/null
```

Startup wall samples, seconds: `0.02, 0.02, 0.02, 0.02, 0.02`.

The `-l` sample reported `56426496 maximum resident set size` and
`55985536 peak memory footprint`.

## Report questions

1. Direct coverage: finite Datalog facts, rules, list-comprehension joins,
   duplicate elimination for derived rows, query projection, and naive
   recursive fixed-point evaluation.
2. Fact update and retraction use the declaration-rebuild adapter. The bounded
   negative example is a host predicate qualifier.
3. The finite cyclic closure completes through `naive-evaluation` and its
   equality comparison, under the probe's two-second hard bound.
4. SBCL created and ran the standalone Mach-O arm64 image.
5. The image is `42277112` bytes, SHA-256
   `7f92a0e11699b178595b3159505396113c8a3d1cefc58fcc0a684ed392bf5b44`,
   dynamically links `libSystem` and `libzstd`, starts in the recorded samples,
   and has the recorded RSS sample.
6. The source trace above identifies facts, rules, evaluation, recursion,
   equality-based termination, and query functions. No caching facility is
   implemented beyond each predicate's current completed value in a fixed-point
   iteration.
7. Before executing DL7 compiler rules, remaining receipts include first-order
   unification and occurs policy, a query scheduler, tabled completion,
   stratified negation, constraints, incremental update repair, source-location
   carrying terms, and query-state isolation.
