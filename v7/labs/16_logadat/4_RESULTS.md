# Lab 16 Results: logadat

## Environment

| Item | Value |
| --- | --- |
| Library | logadat, upstream commit `23fc43cc918e0aaac2aace1410e7283ef675153a` (2025-12-20, clean checkout) |
| License | MIT |
| Runtime | SBCL 2.6.7 (Homebrew arm64) |
| Host | macOS (Darwin, arm64) |
| Checkout | `/private/tmp/sprefa-v7-lab16/logadat` (temporary; not in git) |
| Dependency route | one route: direct source load, zero dependencies, no Quicklisp |
| Fixture | shared cyclic graph: `edge(a,b) edge(b,c) edge(c,a) edge(c,d)`; `path(X,Y) :- edge(X,Y)`; `path(X,Y) :- edge(X,Z), path(Z,Y)` |

## Commands

```sh
git clone https://github.com/taarotman/logadat /private/tmp/sprefa-v7-lab16/logadat
git -C /private/tmp/sprefa-v7-lab16/logadat log -1 --format='%H %ad %s' --date=iso
# 23fc43cc918e0aaac2aace1410e7283ef675153a 2025-12-20 22:18:25 +0700 now logadat feels somewhat like an actual language

LOGADAT_SRC=/private/tmp/sprefa-v7-lab16/logadat \
LOGADAT_COMMIT=23fc43cc918e0aaac2aace1410e7283ef675153a \
  sbcl --noinform --disable-debugger --script 2_PROBE.lisp

LOGADAT_SRC=/private/tmp/sprefa-v7-lab16/logadat \
LOGADAT_COMMIT=23fc43cc918e0aaac2aace1410e7283ef675153a \
LOGADAT_OUT=/private/tmp/sprefa-v7-lab16/logadat-lab-image \
  sbcl --noinform --disable-debugger --script 3_BUILD.lisp

/private/tmp/sprefa-v7-lab16/logadat-lab-image

/usr/bin/time -p /private/tmp/sprefa-v7-lab16/logadat-lab-image   # x5
/usr/bin/time -lp /private/tmp/sprefa-v7-lab16/logadat-lab-image  # peak RSS
otool -L /private/tmp/sprefa-v7-lab16/logadat-lab-image
file /private/tmp/sprefa-v7-lab16/logadat-lab-image
shasum -a 256 /private/tmp/sprefa-v7-lab16/logadat-lab-image
```

## Probe output (source run)

```text
PROBE library=logadat version=23fc43cc918e0aaac2aace1410e7283ef675153a
UNIFY nested-variable-pattern=UNSUPPORTED (evaluates the nested pattern; see 4_RESULTS.md)
UNIFY nested-constant pattern=(x '(b c)) -> ((A (B C)))
UNIFY flat-rule m(x,y) -> ((A (B C)) (D E))
OCCURS absent-from-probe (no unification; tuple matching via equal+destructuring)
PATH full=((A A) (A B) (A C) (A D) (B A) (B B) (B C) (B D) (C A) (C B) (C C) (C D))
PATH-FROM-A path(x a)=((A A) (B A) (C A)) rounds=4
DUPES raw-per-rule=(12 4) raw-total=16 stored-unique=12
UPDATE after-retract path(x a)=((A A) (B A) (C A))
BINARY blocked:not-built
```

## Image run

```text
PROBE library=logadat version=23fc43cc918e0aaac2aace1410e7283ef675153a image=built
PATH full=((A A) (A B) (A C) (A D) (B A) (B B) (B C) (B D) (C A) (C B) (C C) (C D)) rounds=4
```

## Notes on each line

- UNIFY: logadat has no unification. Tuple matching is zipped element binding
  through destructuring lambda lists (`inzip`, logadat.lisp:66-74) plus
  `equal` constraints for constant positions (`pm-quals`, 130-148). A flat
  pattern binds variables elementwise (`m(x,y) :- n(x,y)` binds through the
  nested tuple `(a (b c))` as a unit). A quoted constant inside a pattern
  position matches structurally by `equal`. A variable inside a nested list
  position is unsupported: `pm-quals` moves the whole nested list into an
  `(equal g (y z))` qualifier, which then evaluates `y` and `z` as Lisp
  variables/functions and fails with "The variable Z is unbound". The
  docstring itself scopes the mechanism as "primitive (non-recursive) pattern
  matching".
- OCCURS: no occurs-check concept exists; there is no unification and no
  cyclic-term representation.
- PATH: full cyclic closure computed to exactly the expected 12 tuples in
  4 fixpoint rounds. Termination mechanism: bottom-up naive evaluation to
  least fixpoint (`naive-evaluation`, logadat.lisp:312-316). Each round
  rewrites every rule body against the current value sets, evaluates all rules
  through list comprehensions, and compares old/new sets with
  `predicate=` = `set-exclusive-or` under `equal` (293-301). Values stay
  finite (dedup by `remove-duplicates`/`lunion`, 81-91, 287-290) and monotone
  growth saturates, so recursion over the cycle a→b→c→a terminates. Probe
  enforces an explicit round bound (100) and a 10-second wall bound; neither
  was hit. The query `path(x 'a)` selects the from-`a` pairs.
- DUPES: at the final iterate the recursive rule alone derives 12 rows and the
  base rule 4 (16 raw) against 12 stored unique rows; dedup is inside
  `eval-rules` per round. Stored results are set-valued; duplicate proof paths
  collapse. Raw per-rule ordering shows `collect-preds` consing rules onto
  `rule-list` in reverse definition order (191).
- UPDATE: retraction is host-side rebuild. Removing `edge(c,d)` and rerunning
  yields `path(x a) = {a-a, b-a, c-a}` (d unreachable, cycle intact); no
  incremental invalidation exists, each evaluation recomputes from scratch.
- BINARY: filled by the image below.

## Saved executable image measurements

| Metric | Value |
| --- | --- |
| Image | `/private/tmp/sprefa-v7-lab16/logadat-lab-image`, Mach-O 64-bit executable arm64, `sb-ext:save-lisp-and-die :executable t` |
| Executable bytes | 37,295,616 |
| SHA-256 | `51b8df460610c315132d08ba5f7576b9b2c45e1c7ce4a4024b2857775e7cf12b` |
| Dynamic deps (`otool -L`) | `/usr/lib/libSystem.B.dylib`, `/opt/homebrew/opt/zstd/lib/libzstd.1.dylib` (Homebrew zstd, non-system) |
| Startup samples (wall, `/usr/bin/time -p`, 5 runs) | 0.01, 0.01, 0.01, 0.01, 0.01 s |
| Peak RSS (`/usr/bin/time -lp`) | 44,400,640 bytes maximum resident set size |
| Smoke test | image exits 0 and reproduces the PATH line with `image=built` |
| Source/compilation in image | available: full SBCL save including the loaded logadat source and its macros |

## Capability classification

| Capability | Result | Detail |
| --- | --- | --- |
| nested term unification | absent-from-probe | flat tuple matching only; nested constant positions match by `equal`; a variable inside a nested position errors |
| occurs check | absent-from-probe | no unification at all |
| multiple answers | native (set-valued) | predicate values are deduplicated sets; ordering follows comprehension/union order |
| fair search | absent-from-probe | no backtracking search; bottom-up evaluation only |
| cyclic transitive closure | native | terminates by naive bottom-up fixpoint over finite deduplicated value sets; 4 rounds on the shared fixture |
| Datalog fixpoint | native | least fixpoint by set-comparison convergence; `predicate=` uses `set-exclusive-or` under `equal` |
| tabling | native (trivially) | the fixpoint result set is the table; no per-call variant/subsumptive tables, no answer subsumption |
| constraints | absent-from-probe | none |
| dynamic facts and retraction | adapter | no API; rebuild EDB + full re-evaluation |
| saved executable image | built | 37,295,616 bytes, deps libSystem + Homebrew libzstd, 0.01 s startup, 44.4 MB peak RSS |

## Report questions

1. **SWI coverage directly:** function-free Horn rules with recursive fixpoint
   evaluation (the Datalog core), set-valued answers, deduplication, and
   pattern queries with constants. No unification, no cut/search semantics, no
   tabling-as-SLG, no constraints, no negation.
2. **Adapters needed:** any Prolog-shaped requirement (backtracking, compound
   terms, occurs policy, negation, incremental updates) lives entirely outside
   the library.
3. **Cyclic recursion:** terminates; mechanism is naive bottom-up fixpoint
   (rewrite + evaluate + set-compare until stable), with deduplication keeping
   value sets finite. Not depth-first recursion, so no stack hazard.
4. **SBCL image:** yes; single-file source loads clean, `save-lisp-and-die`
   produced a working executable.
5. **Measurements:** 37,295,616 bytes; deps `libSystem.B.dylib` +
   `libzstd.1.dylib`; five startup samples all 0.01 s; peak RSS 44,400,640 bytes.
6. **Implementing files:** one file, `logadat.lisp` — facts 203-223, rule
   store 161-201, rewrite 226-250, comprehension generation 254-290, fixpoint
   293-316, queries 325-353, program macro 361-378.
7. **Before DL7 compiler rules could run:** add compound-term terms with a
   real unifier and occurs policy, an incremental update path (assert/retract
   without full recompute), a seminaive strategy (the library's commented-out
   `seminaive-evaluation` is a stub), column/arity indexing beyond the one
   hash key per predicate, and a package policy for the `*package*`-sensitive
   `rule-to-compr` eval. The comprehension-to-eval pipeline also means rule
   evaluation is Lisp `eval` per round per rule; a DL7 backend would want a
   compiled representation instead.
