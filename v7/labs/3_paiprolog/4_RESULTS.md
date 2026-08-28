# paiprolog results

Probe: `2_PROBE.lisp` (compiled layer via `<-`/`<--`/`prolog-collect`/`prolog-first`
plus interpreter internals for traces). Build: `3_BUILD.lisp`.

```text
PROBE library=paiprolog version=012d6bb255d8af7f1c8b1d061dcd8a474fb3b57a
UNIFY (F B (G A))
OCCURS interp-default(t)=T interp-nil=T
OCCURS-CHECKED compiled unify-with-occurs-check: cyclic-case=CYCLIC-BINDING-CREATED ground-case=BOUND-OK
OCCURS-COMPILED default-compiled-= NIL
PATH raw=(C B D A C B) raw-count=6 sorted=(A B C D)
PATH-MECH adapter=depth-bound engine=dfs-sld-compiled no-tabling
PATH-CYCLE unbounded=timed-out
FAIR dfs-left-branch=starved later-answer=reachable
CUT answers=(A) (cut commits first clause, drops the rest)
APPEND as-split=(((A B) NIL) ((A) (B)) (NIL (A B)))
APPEND as-prefix=((A))
DUPES raw=(B D A C) sorted-dedup=(A B C D)
UPDATE after-retract sorted=(A B C)
UPDATE-MECH <-- replace-all tmpf-clauses=(((TMPF 2)))
NEGATION bounded-naf sorted=(D)
TRACE interp-unify calls=7 returns=7 records=14 (raw trace kept below)
BINARY 40769552
```

## Commands

```sh
git clone https://github.com/quek/paiprolog "$PAIPROLOG_SRC"
git -C "$PAIPROLOG_SRC" checkout --detach 012d6bb255d8af7f1c8b1d061dcd8a474fb3b57a
PAIPROLOG_SRC=... PAIPROLOG_LAB_BINARY=... sbcl --noinform --disable-debugger --script 2_PROBE.lisp
PAIPROLOG_SRC=... sbcl --noinform --disable-debugger --load 3_BUILD.lisp
PAIPROLOG_LAB_BINARY=/private/tmp/sprefa-v7-paiprolog-lab-012d6bb-20260828
mv ./paiprolog-lab "$PAIPROLOG_LAB_BINARY"
export PAIPROLOG_LAB_BINARY
"$PAIPROLOG_LAB_BINARY"
stat -c '%s' "$PAIPROLOG_LAB_BINARY"
shasum -a 256 "$PAIPROLOG_LAB_BINARY"
otool -L "$PAIPROLOG_LAB_BINARY"
for sample in 1 2 3 4 5; do /usr/bin/time -p "$PAIPROLOG_LAB_BINARY" >/dev/null; done
/usr/bin/time -lp "$PAIPROLOG_LAB_BINARY"
```

SBCL 2.6.7 (Homebrew arm64). No Quicklisp, no transitive dependencies.
The pinned checkout stays in `/var/folders/.../opencode/paiprolog`; the
generated executable was moved to
`/private/tmp/sprefa-v7-paiprolog-lab-012d6bb-20260828`.

## Cyclic transitive closure

The probe defines recursive `path/2` over the three-node cycle and wraps full
answer collection in an SBCL 0.001-second timeout. It prints
`PATH-CYCLE unbounded=timed-out`. The depth-bound adapter `pathd/3` uses
`(> ?d 0)` and `(is ?d1 (- ?d 1))`; depth 4 gives `{a b c d}` with 6 raw
answers. The engine has no tabling or fixpoint evaluation.

## Fair-search starvation

The probe installs a divergent `spin/0`, then puts `(starve blocked) :-
spin` before the finite fact `(starve reachable)`. An SBCL 0.1-second timeout
fires before `reachable` can be returned. The printed result is
`FAIR dfs-left-branch=starved later-answer=reachable`. This bounds the test
while leaving Paiprolog's left-to-right DFS search unchanged.

## Occurs-check policy (per layer)

| Layer | Policy | Receipt |
| --- | --- | --- |
| Interpreter `unify` (unify.lisp) | occurs check ON by default (`*occurs-check* t`), toggleable | `X = f(X)` fails by default; with `*occurs-check* nil` the binding is created and `subst-bindings`/`unifier` diverge on it |
| Compiler `=` (compile-unify, prologc.lisp:236 case 11) | STATIC compile-time occurs check: `find-anywhere` of the var in the term compiles the goal to failure | `(= ?x (f ?x))` collects NIL |
| Compiled runtime `unify!` (prologc.lisp:54) | none; `set-binding!` binds unconditionally | reachable via aliasing; cyclic binding then makes `deref-exp`/`print-var` diverge (stack exhaustion observed) |
| Compiled `unify-with-occurs-check/2` (prologcp.lisp:111) | none; despite ISO 8.2.2 intent it calls destructive `unify!`, so `*occurs-check*` has no effect | `unify-with-occurs-check ?v (f ?v)` created the cyclic binding (probe sentinel `CYCLIC-BINDING-CREATED`) |

## Cut: exact observed output and compiled expansions

Probe line, verbatim:

```text
CUT answers=(A) (cut commits first clause, drops the rest)
```

Fixture: `(first-edge ?x) (edge ?x ?) !` over the four edge facts. Without
the cut the same query yields 4 answers `(A B C C)` (observed during the
debug runs before the bare-symbol fix); with the cut exactly one `(A)`.

Cut compiled by the correct spelling (bare symbol `!`),
`(compile-clause '(?arg1) '((first-edge ?x) (edge ?x ?) !) 'cont)` expands
verbatim (with `*predicate*` nil in the direct call, `RETURN-FROM NIL`):

```lisp
(EDGE/2 ?ARG1 (?) #'(LAMBDA () (PROGN (FUNCALL CONT) (RETURN-FROM NIL NIL))))
```

Inside `compile-predicate`, `*predicate*` = `FIRST-EDGE/1`, so the cut is
`(RETURN-FROM FIRST-EDGE/1 NIL)`. With the parenthesized goal `(!)` the
expansion degrades verbatim to:

```lisp
(LAMBDA (#1=#:?ARG1 PAIPROLOG::CONT)
  (BLOCK PAIPROLOG::FIRST-EDGE/1
    (PAIPROLOG::EDGE/2 #1# (PAIPROLOG:?)
                       #'(LAMBDA () (PAIPROLOG::!/0 PAIPROLOG::CONT)))))
```

`!/0` is `(defun !/0 (cont) (funcall cont))` (prologcp.lisp:34): the cut is
silently dropped and all edge answers flow through (observed `4 answers`).

## Unification trace (interpreter, bounded: `(edge a ?x)`), exact

```text
  0: (PAIPROLOG::UNIFY (PAIPROLOG-LAB::EDGE PAIPROLOG-LAB::A PAIPROLOG-LAB::?X)
                       (PAIPROLOG-LAB::EDGE PAIPROLOG-LAB::A PAIPROLOG-LAB::B)
                       ((T . T)))
    1: (PAIPROLOG::UNIFY PAIPROLOG-LAB::EDGE PAIPROLOG-LAB::EDGE ((T . T)))
    1: PAIPROLOG::UNIFY returned ((T . T))
    1: (PAIPROLOG::UNIFY (PAIPROLOG-LAB::A PAIPROLOG-LAB::?X)
                         (PAIPROLOG-LAB::A PAIPROLOG-LAB::B) ((T . T)))
      2: (PAIPROLOG::UNIFY PAIPROLOG-LAB::A PAIPROLOG-LAB::A ((T . T)))
      2: PAIPROLOG::UNIFY returned ((T . T))
      2: (PAIPROLOG::UNIFY (PAIPROLOG-LAB::?X) (PAIPROLOG-LAB::B) ((T . T)))
        3: (PAIPROLOG::UNIFY PAIPROLOG-LAB::?X PAIPROLOG-LAB::B ((T . T)))
        3: PAIPROLOG::UNIFY returned ((?X . B))
        3: (PAIPROLOG::UNIFY NIL NIL ((PAIPROLOG-LAB::?X . PAIPROLOG-LAB::B)))
        3: PAIPROLOG::UNIFY returned ((?X . B))
      2: PAIPROLOG::UNIFY returned ((?X . B))
    1: PAIPROLOG::UNIFY returned ((?X . B))
  0: PAIPROLOG::UNIFY returned ((?X . B))
```

The trace contains 14 records: 7 call records and 7 matching return records.
The first edge clause matches, and the interpreter's `some` stops clause
enumeration. The remaining three edge facts are not tried by this query.

## Unresolved semantics (recorded, not fixed)

1. `unify-with-occurs-check/2` (prologcp.lisp:111) uses destructive
   `unify!`, so `*occurs-check*` never affects it. ISO 8.2.2 intent says it
   should perform the occurs check. Recorded as observed behavior with a
   probe sentinel; no library change made.
2. Cut placement inside `if-then-else` and cut across clause boundaries:
   the compiled cut abandons the entire predicate (`return-from`), which
   for `if`/`or`-embedded cuts is stricter than ISO commit semantics. The
   probe only verifies the top-level clause cut; nested-cut behavior is
   unprobed (bounded scope) and recorded here as unresolved.
3. `lisp/1` discards its result by construction (`progn`), so the `lisp`
   goal cannot express failure; boolean tests must use comparison
   builtins. Whether this is intended (side-effect bridge) or a bug is a
   fork-author question the repo does not answer; recorded as observed.
4. Interpreter answer enumeration for `prove` is not tail-recursive and
   materializes all solutions; combined with the missing runtime occurs
   check, cyclic bindings diverge `deref-exp`/`subst-bindings` (stack
   exhaustion observed twice during probing). Recorded; no fix attempted.

## Behavioral findings (fork-specific vs inherited)

Inherited PAIP behavior (also in Norvig's book sources):

- DFS SLD interpreter and continuation-passing compiler, clause DB on
  symbol plists, lazy `prolog-compile-symbols` over `*uncompiled*`.
- Cut compiles to `(progn (rest of body) (return-from pred nil))`:
  commits the current clause and abandons remaining clauses of the
  predicate. Verified: `(first-edge ?x) (edge ?x ?) !` yields one answer `(A)`.
- Compiled `unify!` has no runtime occurs check (Norvig 12.4, destructive
  unification; occurs check only in the book's interpreter version).
- `retract-clause` compares clauses with `equal`, so the caller must pass
  the full clause shape `((edge c d))`, head included; passing `(edge c d)`
  deletes nothing. Retraction then recompiles lazily; `pathd` answers
  correctly drop `d` afterwards (`{a b c}`).

Repository changes (quek fork + contributors):

- `<--` retracts ALL same-arity clauses before adding one. Sequential
  `<--` calls therefore leave only the last clause (verified:
  `(<- (tmpf 1)) (<-- (tmpf 2))` leaves `((TMPF 2))`). This is
  replace-predicate semantics, which the probe records as UPDATE-MECH.
- `prolog`/`prolog-collect`/`prolog-first` compiled query macros with the
  `lisp` bridge. `lisp/1` discards its result (`progn`): a `(lisp (> ?d 0))`
  test never fails the clause, and using it as a guard gives unbounded
  descent (observed: `?d` ran to -35002 before stack exhaustion). Boolean
  guards must use the arithmetic comparison builtins (`(> ?d 0)`).
- Gotchas found while probing: a parenthesized cut goal `(!)` compiles to
  the no-op `!/0` (prologcp.lisp:34), silently dropping the cut; the cut
  must be the bare symbol `!`. `prolog-first`/`prolog-collect` require at
  least one var (`must specify vars` error otherwise).
- alanruttenberg fix (2018): `compile-clause` special-cases `if`/`or`/`and`
  heads so `setof`-style disjunctive bodies compile.

## Capability classification

| Capability | Result | Detail |
| --- | --- | --- |
| nested term unification | native (library) | interpreter `unify` and compiled `unify!` both structural over conses; receipt `UNIFY (F B (G A))` |
| occurs check | native, split policy | interpreter on/toggleable; compiler static at compile time; compiled runtime absent; `unify-with-occurs-check` mis-implemented (cyclic binding created) |
| multiple answers | native | DFS SLD; depth-first order; `prolog-collect` returns all bindings, `(C B D A C B)` raw order for pathd |
| fair search | implement | starvation probe times out in the divergent left branch before the finite later answer |
| cyclic transitive closure | adapter (depth bound) | unbounded answer collection hits the in-probe timeout; no tabling |
| Datalog fixpoint | absent-from-probe | no fixpoint evaluation of any kind |
| tabling | absent-from-probe | no call/answer table, no variant/subsumptive/AS forms |
| constraints | absent-from-probe | no constraint store; arithmetic evaluation and comparisons only |
| dynamic facts and retraction | native | `<--` replacement and internal `retract-clause` exercised; exported `asserta`/`assertz`/`retract`/`abolish` found in source; lazy recompile verified |
| standalone image | built | SBCL `save-lisp-and-die`, see below |

## Standalone image measurements

- Bytes: 40,769,552.
- SHA-256: `3b60739f1ca822c7f97ded738c3f2943e7be33b935e55a91f64ae74e2f0525ab`.
- `file`: Mach-O 64-bit executable arm64.
- `otool -L`: `/usr/lib/libSystem.B.dylib`, `/opt/homebrew/opt/zstd/lib/libzstd.1.dylib` (SBCL core compression only).
- Startup wall time (5 runs of the full probe): 0.13, 0.13, 0.13, 0.13, 0.13 s. Each run includes the 0.1-second starvation bound, the 0.001-second cyclic-closure bound, and lazy predicate compilation.
- Peak RSS (`/usr/bin/time -l`): 52,969,472 bytes.
- Source loading/compilation: available in the image. The clause DB is compiled lazily during the probe; retracting `edge/2` causes one predicate redefinition.
- Probe records match the script run when both receive the same `PAIPROLOG_LAB_BINARY` path.

## Final report questions

1. Probe-covered facilities: first-order unification, backtracking with
   multiple answers, cut, internal single-clause retraction, `<--`
   predicate replacement, `fail-if` negation, and Lisp-bridge calls. Source
   inspection additionally found `if`/`or`/`and`, dynamic
   assert/retract/abolish, and `findall`; this probe did not execute them.
2. Adapters needed: depth/visited bounds for recursive compiler queries
   (`pathd/3`), answer dedup at the adapter boundary, `lisp` guards replaced
   by comparison builtins for boolean tests.
3. Cyclic recursion terminates only through an adapter (depth bound or
   visited set); the engine is plain DFS SLD and diverges on the 3-cycle.
4. Yes: `paiprolog` compiles into an SBCL image without modification.
5. 40,769,552 bytes; libSystem + libzstd only; startup samples 0.13, 0.13,
   0.13, 0.13, 0.13 s; peak RSS 52,969,472 bytes.
6. Unification: `unify.lisp` (interpreter), `prologc.lisp` 54-90 + 202-280
   (compiler). Search: `prolog.lisp` 84-97, `prologc.lisp` 373-392 + 458-520.
   Rule evaluation/lowering: `compile-clause`/`compile-predicate`
   (prologc.lisp 299-327, 55-80). Caching: none beyond compiled defuns and
   the `*uncompiled*` worklist; clause DB on symbol plists.
7. Before executing DL7 compiler rules, code that would remain: a call/answer
   tabling layer or SCC completion for recursive compiler queries, answer
   deduplication, occurs-safe compiled unification (fix
   `unify-with-occurs-check`), bounded negation with stated stratification
   semantics, and incremental invalidation of compiled predicates after
   assertion/retraction (currently whole-predicate recompile on next run).
