# cl-gambol source record

## Upstream

- Repository: https://github.com/wmannis/cl-gambol
- Pinned commit: `d4d53a1e29a360f8aaab9134da89b8c6966fe16e` (Mon Feb 12 14:41:34 2018 -0600, "markdown tweakery")
- Version string in `gambol.asd`: `0.03`
- License: README states BSD terms for University of Utah Frolic and MIT terms for the author's modifications; the repository has no standalone license file.
- ASDF system name: `gambol`; depends on no other libraries.

Checkout location for this lab: `/var/folders/z2/.../opencode/cl-gambol` (temporary,
outside the repository). The lab reads it through the `GAMBOL_SRC` environment
variable so no downloaded source is vendored into git.

Install/load route (the one dependency route used):

```lisp
(asdf:load-asd (merge-pathnames "gambol.asd" gambol-src-dir))
(asdf:load-system "gambol")
```

No Quicklisp needed: zero transitive dependencies.

## Exported API used by the probe

`*-`, `pl-assert`, `pl-asserta`, `pl-retract`, `pl-solve-one`, `pl-solve-next`,
`pl-solve-rest`, `pl-solve-all`, `do-solve-all`, `with-rulebase`, `make-rulebase`,
`clear-rules`, `=*` unification goal `(= lhs rhs)`.

## Internal trace (prolog.lisp, 995 lines)

| Concern | Where | Mechanism |
| --- | --- | --- |
| Logical variables | `calcify` (775), `var?` (164) | `?x` symbols calcified to `(*var* name index)` cells pointing into a per-rule vector environment |
| Environments | `make-empty-environment` (182), `pl-bind` (205) | simple vectors; bindings pushed to a global trail, undone by `untrail` (216) on backtrack |
| Unification | `unify`/`unify1`/`unify2` (684-713) | dereference via `x-view-var`/`y-view-var`, then car/cdr recursion over Lisp conses; `equalp` on atoms |
| Occurs check | absent | `pl-bind` binds unconditionally; `X = f(X)` creates a cyclic binding |
| Search | `pl-search` (446), `search-rules` (505), `match-rule-head` (580), `continue-on` (606) | depth-first with explicit continuation vectors; cut via `do-cut` (630) |
| Rule index | `*prolog-rules*` hash table, `add-rule-to-index` (397) | functor -> ordered rule list, insertion order preserved (append on assert) |
| Recursion / tabling | none | no call table, no answer table, no SCC completion |
| Dynamic facts | `pl-assert`/`pl-asserta`/`pl-retract` (807, 814, 896) | retract removes first matching fact only, cannot remove rules |
| Lisp bridge | `lop` / `lisp` / `is` (290-338, 245-256) | `apply` of a Lisp function; non-nil = success; `is` binds multiple values |
