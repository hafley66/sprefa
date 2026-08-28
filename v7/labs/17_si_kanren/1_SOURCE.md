# Lab 17 Source: si-kanren

| Item | Value |
| --- | --- |
| Upstream | https://github.com/rgc69/si-kanren |
| Pinned commit | `93f051fcc2b46649d214eab951cdd4ed1de869da` ("Wrap playground demo queries with #+nil blocks", 2025-12-24 12:03:02 +0100) |
| Pin method | upstream `git clone`; commit found whose tree is byte-identical (modulo `.git`) to the Quicklisp tarball `si-kanren-20260101-git` under `.quicklisp/dists/quicklisp/software/` (diff -rq receipt in `4_RESULTS.md`) |
| License | MIT (LICENSE file; `si-kanren.asd` `:license "MIT"`) |
| Packaging | Quicklisp dist 20260101, system `si-kanren` version 0.1.0; ASDF system `si-kanren.asd`, serial module `src`: `si-kanren.lisp` (510 lines), `wrappers.lisp` (472 lines), plus an uncompiled `playground.lisp` (973 lines, not part of the system components) |
| Dependencies | none ("pure Common Lisp, without external libraries") |
| Install route | one route: lab-local Quicklisp at `/private/tmp/sprefa-v7-lab17/.quicklisp/` (installed with `--no-sysinit --no-userinit`; no global Quicklisp mutation, `~/.sbclrc` bypassed) |
| Checkout location | `/private/tmp/sprefa-v7-lab17/upstream` (git pin) and `/private/tmp/sprefa-v7-lab17/.quicklisp/dists/quicklisp/software/si-kanren-20260101-git` (Quicklisp load source); both temporary, outside the repository |
| Runtime | SBCL 2.6.7 (Homebrew arm64), macOS Darwin arm64 |
| API surface | no `defpackage`/exports: the library interns into whatever package loads it; no exported API. Probe loads it into `SIK-VENDOR` and dispatches through `find-symbol` |

## Implementation inventory

| Concern | Implementation | Lines |
| --- | --- | --- |
| Substitutions | association lists of `(lvar . value)`; state is a 4-element structure `(S/C D TY A)`: substitution+counter, disequality store, type store, absento store (`cs = '(((s) . c) (d) (t) (a))`) | si-kanren.lisp:23-33; wrappers.lisp:3-24 |
| Walk / reification | `walk` (si-kanren.lisp:27-30), `walk*` (wrappers.lisp:116-124); reification `reify-s`/`reify-name`/`reify-state/1st-var`/`mK-reify` — note `mK-reify` PRINTS answers and returns no values (wrappers.lisp:107-115) | as cited |
| Unification | `unify`: car/cdr recursion over conses, `lvar` = `(vector c)` identity | si-kanren.lisp:46-58 |
| Occurs-check policy | `occurs?` called unconditionally in both lvar branches of `unify`; occurs check on, failure is the `'((()))`-style marker `'(() )` | si-kanren.lisp:38-44, 50-51 |
| Streams / scheduling | `mzero`/`unit`; `mplus` interleaves at thunk boundaries (`(mplus $2 (funcall $1))`), `bind` is append-map over thunks; `pull`/`take`/`take-all` drive; recursion must be delayed with the `zzz` macro | si-kanren.lisp:34-36, 103-113; wrappers.lisp:26-42, 125-127 |
| Conjunction / disjunction | `conj`/`disj` closures; `conj+`/`disj+`/`conde`/`fresh` macros; `==` goal runs unify then the constraint checks (`reform-T`, `reform-A`, disequality normalization, a/t→d bridge) | si-kanren.lisp:60-128, 69-102; wrappers.lisp:128-147 |
| Disequality store | `disequality`/`=/=`; `subtract-s` builds the residual constraint; `normalize-d<s/t/a` normalizes against type/absento stores; `subsumed-d-pr/T?`/`subsumed-d-pr/A?` subsumption | si-kanren.lisp:145-172, 174-191, 290-328, 416-475 |
| Type store | `typeo` (tag + predicate), `symbolo` = `(typeo 'sym #'symbolp u)`, `numbero` = `(typeo 'num #'numberp u)`; `ty-merge`, `reform-T` | si-kanren.lisp:192-331 |
| Absento store | restricted absento (first argument must be a symbol): `absento`, `a-add`, `reform-A`, `subsumed-d-pr/A?`, `check-a/t->disequality` bridge | si-kanren.lisp:334-415, 476-510 |
| Drivers / queries | `run`/`run*`/`runno`/`runno*`/`runi` macros; answer normalization (`normalize`, `normalize-conde`, `normalize-subsumed`, ...) in wrappers | wrappers.lisp:149-178, 220-510 |

## Observed upstream bug (receipt in `4_RESULTS.md`)

At this commit, `unify`'s equal-value branch (si-kanren.lisp:57)

```lisp
((and (equalv? u v) (not (equal s '(())))  s))
```

returns `s` as the cond test value. With the empty substitution `s = '()`
= `NIL`, the clause value is `NIL`, `cond` treats it as failure and falls
through to `T '(() )`. Therefore `==` between two identical ground atoms
(or compound terms with identical ground heads) fails whenever the
substitution is still empty, e.g. `(== 'a 'a)` on the empty state. The probe
uses a `g==` adapter that binds one dummy variable first; every other probe
result relies on it for ground positions.
