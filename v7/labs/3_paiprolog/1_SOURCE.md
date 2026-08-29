# paiprolog source record

## Upstream and lineage

- Repository: https://github.com/quek/paiprolog
- Pinned commit: `012d6bb255d8af7f1c8b1d061dcd8a474fb3b57a` (2018-02-24, "Merge pull request #17 from alanruttenberg/master")
- 39 commits total; first commit 2011-02-22 "PAIProlog", last upstream activity 2018-02-24.
- License: Peter Norvig's PAIP terms (`license.html`, http://www.norvig.com/license.html).
- ASDF systems: `paiprolog` (package, auxfns, patmatch, unify, prolog, prologc, prologcp, prolog-ext) and `unifgram`.

## Relationship to Peter Norvig's PAIP Prolog

The repo is a packaging fork of the Prolog interpreter and compiler source
files from Peter Norvig's *Paradigms of Artificial Intelligence Programming*
(1991, chapters 11-12). Headers in `prolog.lisp`, `unify.lisp`,
`prologc.lisp`, `prologcp.lisp`, `interp*.lisp` all read "Code from
Paradigms of AI Programming, Copyright (c) 1991 Peter Norvig".

Lineage chain:

1. Norvig's PAIP book sources (1991), `prolog.lisp` (interpreter, ch 11.3),
   `unify.lisp`, `prologc.lisp`/`prologcp.lisp` (compiler, ch 12),
   distributed as individual chapter files from norvig.com.
2. `cl-user.net` packaging of those files (the fork parent per README).
3. Christophe Rhodes' PAIProlog (2005): modernization to SBCL, package
   system, README/TODO, fixes; README.Christophe_Rhodes remains in the repo.
4. quek's fork (2011-2018): split `auxfns` into its own package, added the
   `<--` replace-clause macro, the `prolog`/`prolog-collect`/`prolog-first`
   compiled-query macros with a `lisp` bridge into the surrounding Lisp
   lexical environment, string/atom helpers, `unifgram` system, and
   alanruttenberg fixes for `setof`/`or` compilation (2018) and
   print-case-insensitive interning.

Norvig originals also live at https://github.com/norvig/paip-lisp (book
sources, `lisp/prolog*.lisp`) and in the printed book; the quek repo is a
SBCL-focused maintained fork of the Rhodes packaging, not a rewrite.

## Quicklisp

Release `paiprolog` 2018-02-28 in the Quicklisp dist; that release matches
the pinned commit closely (commit is 4 days earlier). This lab installs
from the git checkout instead, via an env var, so the commit is exact.

## Checkout and load route (the one dependency route used)

Checkout at `/var/folders/z2/.../opencode/paiprolog` (temporary, outside
git). The lab reads it through the `PAIPROLOG_SRC` environment variable so
no downloaded source is vendored.

```sh
git clone https://github.com/quek/paiprolog "$PAIPROLOG_SRC"
git -C "$PAIPROLOG_SRC" checkout --detach 012d6bb255d8af7f1c8b1d061dcd8a474fb3b57a
```

```lisp
(asdf:load-asd (merge-pathnames "paiprolog.asd" (uiop:getenv "PAIPROLOG_SRC")))
(asdf:load-system "paiprolog")
```

No Quicklisp, no transitive dependencies.
`2_PROBE.lisp` runs `git rev-parse HEAD` and stops before loading ASDF when
the checkout does not match the pinned commit.

## Exported API used by the probe

Exported compiled layer (`prologc.lisp`, `prolog-ext.lisp`):

- `<-` clause macro; stores clauses on the predicate symbol plist.
- `<--` quek addition: retracts all same-arity clauses, then adds one.
- `prolog` macro: compiles an anonymous top-level query in the surrounding
  Lisp lexical environment; `lisp` goals close over Lisp variables.
- `prolog-collect (vars) goals`: all bindings of vars as a list.
- `prolog-first (vars) goals`: first binding.
- `run-prolog`, `prolog-compile-symbols` underneath.
- Probe-used compiled goals: `=`, `is`, `lisp`, `!` (cut), `fail-if`,
  `unify-with-occurs-check`, and arithmetic comparisons.
- Additional exported or installed compiled goals found in source: `if`, `or`,
  `and`, `asserta`, `assertz`, `retract`, `abolish`, and `findall`. These were
  source-inspected but were not exercised by this probe.

Interpreter layer (`prolog.lisp`, `unify.lisp`) used for traces:
`prove-all`, `prove`, `unify`, `unifier`, `subst-bindings`,
`no-bindings`, `*occurs-check*`.

## Internal trace (file, mechanism)

| Concern | Where | Mechanism |
| --- | --- | --- |
| Interpreter unification | unify.lisp 11-39 | assoc-list bindings, structural car/cdr recursion, `*occurs-check*` parameter defaults to t |
| Compiled unification | prologc.lisp 54-90 (`unify!`, `set-binding!`) | destructive var-binding slots with trail vector; no occurs check |
| Occurs check | unify.lisp 32-39, prologcp.lisp 111-113 | interpreter: on by default; compiled `=` rejects direct self-occurrence statically; the named `unify-with-occurs-check` predicate calls unchecked `unify!` |
| Variables | prologc.lisp 21-33 | `var` struct with name + binding slot; trail `*trail*` vector |
| Search | prolog.lisp 84-97 (`prove`), prologc.lisp 373-392 | interpreter: depth-first `some` over clauses, solutions as list; compiler: CL functions with continuation closures, choice via backtracking to alternatives inside each compiled function |
| Cut | prologc.lisp 464-467 | `!` compiles to `return-from pred` after continuing: commits current clause and abandons remaining clauses of the predicate |
| Clause DB | prolog.lisp 21-47, prologc.lisp 169-176 | symbol plists, `get pred 'clauses`; compilation on demand via `prolog-compile-symbols` over `*uncompiled*` |
| Update | prolog-ext.lisp 3-20 (`<--`), prologc.lisp `asserta`/`assertz`/`retract`/`abolish` | plist mutation; recompiled before next run |
| Recursion / tabling | none | no call table, no answer table, no SCC completion; pure DFS SLD |
| Lisp bridge | prolog-ext.lisp 33-41 + prologcp.lisp 752-775 | `lisp` goal closes over lexical Lisp vars; results dereferenced with `deref-exp` |
