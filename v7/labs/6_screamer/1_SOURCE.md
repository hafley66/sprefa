# Screamer Lab Source

## Upstream

| Field | Value |
| --- | --- |
| Library | Screamer 4.0.0 (nikodemus fork, based on released 3.20) |
| Repository | https://github.com/nikodemus/screamer |
| Pinned commit | `ce50614024de090b376107668da5e53232540ec7` (2021-07-11, master, "Merge pull request #24 from ajberkley/master") |
| License | MIT-style permission notice, MIT/UPenn/Toronto copyright 1991-1993; `.asd` declares `"MIT"`; GitHub shows `NOASSERTION` for the LICENSE file |
| Docs | https://nikodemus.github.io/screamer/ ; in-repo `doc/`, `README`, `ChangeLog.old` |
| Dependencies | none (pure portable Common Lisp, no Quicklisp needed) |
| ASDF system | `:screamer` (`package.lisp` + `screamer.lisp`, serial) |

## Install commands

```sh
git clone https://github.com/nikodemus/screamer.git /private/tmp/screamer-checkout/screamer
git -C /private/tmp/screamer-checkout/screamer checkout --detach ce50614024de090b376107668da5e53232540ec7
test "$(git -C /private/tmp/screamer-checkout/screamer rev-parse HEAD)" = ce50614024de090b376107668da5e53232540ec7
```

The probe requires `SCREAMER_SRC`, validates its current commit, and loads the
checkout by its `.asd` pathname directly:

```lisp
(asdf:load-asd #p"/private/tmp/screamer-checkout/screamer/screamer.asd")
(asdf:load-system "screamer")
```

The checkout lives outside the lab directory in the recorded build environment
(`/private/var/folders/z2/cwfm40fn65n176q8m227wl0r0000gn/T/opencode/screamer-checkout/screamer`,
commit `ce50614024de090b376107668da5e53232540ec7`). Nothing is vendored into the
lab.

## What Screamer is

Screamer is a nondeterministic-programming extension of Common Lisp with two
layers:

1. Nondeterministic substrate: `either`, `fail`, backtracking via a trail,
   undoable side effects (`local` vs `global`), answer collection
   (`all-values`, `one-value`, `ith-value`, `for-effects`).
2. Constraint language over variables: `make-variable`, `a-member-ofv`,
   `an-integer-betweenv`, `assert!`, `andv`/`orv`/`notv`, arithmetic
   constraints (`+v`, `=v`, `<v`), search/forcing (`solution`, `static-ordering`,
   `divide-and-conquer-force`), domain introspection (`domain-size`, `ground?`,
   `bound?`, `value-of`, `apply-substitution`).

Screamer code is transformed into ordinary Common Lisp at compile time, so it
runs compiled and deterministic code pays no trail overhead.

## Source files of interest

| File | Role |
| --- | --- |
| `screamer.lisp` | entire engine: transformation, trail, choice points, constraint store, variable representation, `apply-substitution` |
| `equations.lisp` | attached to the system historically; not in `screamer.asd` components |
| `package.lisp` | exports, `define-screamer-package` |
| `tests.lisp` | regression suite behind `screamer-tests.asd` |
