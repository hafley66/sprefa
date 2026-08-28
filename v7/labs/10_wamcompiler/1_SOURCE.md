# WAMCompiler source record

## Upstream

- Repository: https://github.com/matsud224/wamcompiler
- Pinned commit: `d46a2665734d1ed6c7e73ecda9c4e860631cd858` (2019-12-18T17:55:20+09:00, `add defgeneric declarations`)
- History at pin: 49 commits.
- License: Unlicense, public domain (`LICENSE`).
- Implementation: one source file, `wamcompiler.lisp`; no ASDF system and no third-party Common Lisp dependency.

The external checkout for this lab is
`/private/tmp/sprefa-wamcompiler-lab/wamcompiler`. The lab receives that path
through `WAMCOMPILER_SRC`; no checkout, FASL, or executable is retained in Git.

## Install/load route

```sh
git clone https://github.com/matsud224/wamcompiler.git \
  /private/tmp/sprefa-wamcompiler-lab/wamcompiler
git -C /private/tmp/sprefa-wamcompiler-lab/wamcompiler checkout \
  d46a2665734d1ed6c7e73ecda9c4e860631cd858
WAMCOMPILER_SRC=/private/tmp/sprefa-wamcompiler-lab/wamcompiler \
  sbcl --noinform --disable-debugger --no-sysinit --no-userinit \
  --script 2_PROBE.lisp
```

The source has no package declaration. The probe loads it under `CL-USER`,
where it defines `repl`, `prolog-eval`, compiler tables, and WAM VM globals.

## Source trace

| Concern | Source location | Mechanism |
| --- | --- | --- |
| Scanner and parser | `wamcompiler.lisp:88-420` | character scanner plus Pratt-style operator parser into Lisp terms |
| Clause and query classification | `wamcompiler.lisp:1129-1142`, `1719-1798` | `divide-head-body`; REPL compiles facts/rules and executes `?-` queries |
| WAM instruction generation | `wamcompiler.lisp:1203-1410` | `compile-clause` emits head/body code, environments, calls, and cut instructions |
| Optimization and variable allocation | `wamcompiler.lisp:848-1146`, `1647-1656` | body analysis, register allocation, instruction cleanup, and optimization |
| Predicate indexing | `wamcompiler.lisp:1428-1645` | `compile-indexing-code`, constant/structure switch tables, dispatch table per functor/arity |
| VM execution | `wamcompiler.lisp:2077-2545` | `send-query` fetches and executes WAM instruction lists over register, heap, stack, and trail areas |
| Unification | `wamcompiler.lisp:1955-2035` | iterative pair stack over dereferenced WAM cells; binds references via `bind` |
| Trail and backtracking | `wamcompiler.lisp:1961-1995`, `2418-2489` | choicepoint restore plus `set-to-trail`, `unwind-trail`, and `tidy-trail` |
| Cut | `wamcompiler.lisp:1367-1400`, `2519-2532` | compiler emits `neck-cut` or `cut`; VM discards newer choicepoints |
| Occurs check | no matching check in `unify` / `bind` | variable binding permits cyclic terms unless another VM limit intervenes |
| Dynamic database | `wamcompiler.lisp:1713-1714`, `1769-1798`, `2969-2976` | REPL appends clauses then regenerates dispatch code; `consult/1` loads source; no `assert`, `retract`, or deletion API |

The accepted surface includes facts, Horn clauses, `?-` queries, lists,
`append/3`, arithmetic builtins, `!`, and `\+`. The shared cyclic path program
parses and executes, but its depth-first WAM search has no table or visited set.
