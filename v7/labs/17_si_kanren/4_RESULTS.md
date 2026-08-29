# sī-Kanren capability results

## Environment and commands

| Field | Receipt |
| --- | --- |
| Host | macOS arm64 |
| Common Lisp | SBCL 2.6.7 |
| Library install | isolated Quicklisp 2026-01-01 at `/private/tmp/sprefa-si-kanren.B7fXyx/quicklisp/` |
| Upstream source pin | `93f051fcc2b46649d214eab951cdd4ed1de869da` |
| Recursive bounds | path: four fixed edge counts and `run 16`; fairness: `run 4` |

Probe command:

```sh
cd /Users/chrishafley/projects/sprefa
sbcl --noinform --no-sysinit --no-userinit --disable-debugger \
  --load /private/tmp/sprefa-si-kanren.B7fXyx/quicklisp/setup.lisp \
  --eval '(ql:quickload "si-kanren")' \
  --load v7/labs/17_si_kanren/2_PROBE.lisp \
  --eval '(quit)'
```

Build command:

```sh
cd /Users/chrishafley/projects/sprefa
SI_KANREN_QUICKLISP=/private/tmp/sprefa-si-kanren.B7fXyx/quicklisp/setup.lisp \
SI_KANREN_OUT=/private/tmp/sprefa-si-kanren.B7fXyx/si-kanren-lab-review \
  sbcl --noinform --no-sysinit --no-userinit --disable-debugger \
    --load v7/labs/17_si_kanren/3_BUILD.lisp
```

Final image command:

```sh
/private/tmp/sprefa-si-kanren.B7fXyx/si-kanren-lab-review
```

## Raw public-API probe output

```text
PROBE library=si-kanren quicklisp=2026-01-01 upstream=93f051fcc2b46649d214eab951cdd4ed1de869da
UNIFY ((NODE (ALPHA BETA)))
SUBSTITUTION ((ALPHA (NODE ALPHA)))
OCCURS
ORDER ((LEFT))
 ((RIGHT))
 ((CENTER))

DUPLICATES ((DUPLICATE))
 ((DUPLICATE))

FAIRNESS_LIMIT_4 ((RIGHT))
 ((FIVE))
 ((FIVE))
 ((FIVE))

PATH_DEPTH_4_LIMIT_16 ((B))
 ((C))
 ((A))
 ((D))
 ((B))

DISEQUALITY_RESIDUAL (_.0) WHERE ((=/= (_.0 . BLOCKED)))
DISEQUALITY_VIOLATION
NUMBERO_RESIDUAL (_.0) WITH ((NUM _.0))
NUMBERO_VIOLATION
FIXTURE_SYMBOL_A (A)
FIXTURE_NUMBER_A_REJECT
SYMBOLO_RESIDUAL (_.0) WITH ((SYM _.0))
SYMBOLO_VIOLATION
ABSENTO_RESIDUAL (_.0) WITH ((ABSENTO (FORBIDDEN _.0)))
ABSENTO_VIOLATION
FIXTURE_ABSENTO_D_FROM_A (A)
CONSTRAINT_STORE (((s) . c) (d) (t) (a))
```

Empty records are zero-answer results. `OCCURS` rejects `X = (F . X)`, using the library occurs check. The path record enumerates proof paths, with `B` repeated. Its sorted set is `(A B C D)`. The fixture adapter has fixed depths one, two, three, and four, so it terminates independently of the `a -> b -> c -> a` cycle. `FAIRNESS_LIMIT_4` lets `RIGHT` appear beside the recursively productive `FIVE` branch.

The documented constraint store is `(((s) . c) (d) (t) (a))`:

| Slot | Content | Public reification receipt |
| --- | --- | --- |
| `s` | substitution | `SUBSTITUTION ((ALPHA (NODE ALPHA)))` |
| `c` | fresh-variable counter | allocation is used by every `fresh`; no public raw-state accessor |
| `d` | disequality store | `DISEQUALITY_RESIDUAL (_.0) WHERE ((=/= (_.0 . BLOCKED)))` |
| `t` | type store | `NUMBERO_RESIDUAL (_.0) WITH ((NUM _.0))`, `SYMBOLO_RESIDUAL (_.0) WITH ((SYM _.0))` |
| `a` | absento store | `ABSENTO_RESIDUAL (_.0) WITH ((ABSENTO (FORBIDDEN _.0)))` |

## Capability classification

| Capability | Classification | Probe receipt |
| --- | --- | --- |
| Nested-term unification | native | `UNIFY ((NODE (ALPHA BETA)))` |
| Reified substitutions | native | `SUBSTITUTION ((ALPHA (NODE ALPHA)))` |
| Occurs check | native, structural occurs check | zero answers for `X = (F . X)` |
| Multiple answers | native | `ORDER` preserves `LEFT`, `RIGHT`, `CENTER` branch sequence |
| Answer ordering | native | `ORDER` and `FAIRNESS_LIMIT_4` show stream scheduling order |
| Fair search | native, bounded probe | `RIGHT` arrives within the four-answer bound beside recursive `FIVE` production |
| Duplicate answers | native behavior: proof-path duplicates retained | two `DUPLICATE` answers |
| Cyclic transitive closure | adapter | `path-at-most-4o` is an unrolled four-hop closure; sorted values `(A B C D)` |
| General recursive closure / tabling | absent-from-probe | no call or answer table public API; no general recursive path query was run |
| Datalog least fixpoint | absent-from-probe | no fact-store or seminaive evaluator API in this library |
| Disequality | native | residual store and contradiction are both observed |
| `numbero` | native | residual `NUM`, ground symbol rejection, fixture `A` rejection |
| `symbolo` | native | residual `SYM`, ground number rejection, fixture `A` acceptance |
| `absento` | native | residual `ABSENTO`, nested forbidden-symbol rejection, fixture `absento D A` acceptance |
| Raw constraint-state accessor | absent-from-probe | state layout is documented and source-traced; exported API reifies residual constraints only |
| Dynamic facts / retraction | absent-from-probe | no public assertion, update, or retract API was exercised |
| Standalone image | native, built | SBCL `save-lisp-and-die` receipt below |

## Image receipt

```text
42539296 bytes /private/tmp/sprefa-si-kanren.B7fXyx/si-kanren-lab-review
SHA-256 16645c52a71f72caaaad3185344d4400961eda62611254a9f0da19376fe86890
Mach-O 64-bit executable arm64

/usr/lib/libSystem.B.dylib
/opt/homebrew/opt/zstd/lib/libzstd.1.dylib
```

The final image starts and prints the complete raw probe output reproduced above.

Five independent `/usr/bin/time -l` samples:

| Sample | Real | User | Sys | Maximum RSS | Peak memory footprint |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1 | 0.01 s | 0.00 s | 0.01 s | 47,300,608 bytes | 46,974,144 bytes |
| 2 | 0.01 s | 0.00 s | 0.01 s | 47,185,920 bytes | 46,826,560 bytes |
| 3 | 0.01 s | 0.00 s | 0.01 s | 47,136,768 bytes | 46,810,304 bytes |
| 4 | 0.01 s | 0.00 s | 0.01 s | 47,136,768 bytes | 46,777,408 bytes |
| 5 | 0.01 s | 0.00 s | 0.01 s | 47,300,608 bytes | 46,974,144 bytes |

Timing command:

```sh
/usr/bin/time -lp /private/tmp/sprefa-si-kanren.B7fXyx/si-kanren-lab-review
```

## Source ownership map

| Required operation | Source trace |
| --- | --- |
| Substitution and unification | `src/si-kanren.lisp:23-58` |
| Search scheduling | `src/si-kanren.lisp:103-124` |
| Disequality constraints | `src/si-kanren.lisp:130-187` |
| Type constraints | `src/si-kanren.lisp:192-330` |
| Absento constraints | `src/si-kanren.lisp:334-510` |
| State layout and answer bounds | `src/wrappers.lisp:3-45` |
| Reification | `src/wrappers.lisp:47-123` |
| Public query forms | `src/wrappers.lisp:125-177` |
| Constraint-answer normalization | `src/wrappers.lisp:324-472` |
| Library rule evaluation / caching | no separate rule evaluator or cache source file; stream scheduling is `mplus` and `bind` |

## DL7 execution surface not supplied by this library

| Surface | Classification |
| --- | --- |
| DL7 reader, parser, binding, and lowering | implement |
| Function-free Datalog rule store and least-fixpoint evaluator | implement |
| Recursive call/answer tables and cycle completion | implement |
| Stratified negative goals | implement |
| Incremental update repair | implement |
| Target-neutral engine-plan encoding | implement |
