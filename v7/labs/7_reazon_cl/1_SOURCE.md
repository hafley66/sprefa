# reazon-cl lab source

## Upstream

| Item | Value |
| --- | --- |
| Library | reazon-cl (CL port of the Emacs Lisp miniKanren `reazon`) |
| URL | https://github.com/fiddlerwoaroof/reazon-cl |
| Pinned commit | `3c4e9d916f2e621a3cc759f58ad778473f9da513` |
| Commit date | 2022-09-27 22:48:36 -0700 ("Revert "experiment(ci): add intentionally broken test"") |
| License | GPL-3.0 (LICENSE file); the `reazon-cl.asd` header incorrectly says `:license "MIT"` |
| Upstream of upstream | https://github.com/nickdrozd/reazon (Emacs Lisp, GPL-3.0) |
| ASDF systems | `:reazon-cl`, `:reazon-cl/test` |
| Package | `:reazon` (uses `:cl`, shadows `equal`; `:reazon.reify` for reified names) |
| Transitive dependency | `trivia` 0.1, Quicklisp release `trivia-20260101-git` (pattern matching, used for macro matchers in `disj`/`conj`/`conda`/`condu`) |

## Install commands

```sh
git clone https://github.com/fiddlerwoaroof/reazon-cl <checkout>
cd <checkout> && git checkout 3c4e9d916f2e621a3cc759f58ad778473f9da513
```

`trivia` came from the lab-local Quicklisp cache at
`/var/folders/z2/cwfm40fn65n176q8m227wl0r0000gn/T/opencode/reazon-ql/.quicklisp/`
(installed with `quicklisp-quickstart:install :path ...`; no global Quicklisp
mutation). The probe verifies archive SHA-256
`81f5eacce946f0ffd713f3ecfc97c92dcf5cf1773cbad12cbf378905e24d4913`
and checks that ASDF loaded `trivia` from the matching release directory.
Note: `~/.sbclrc` currently auto-loads a Quicklisp setup from
`/private/tmp/cl-grph-lab/.quicklisp/` left by an earlier lab; every lab run
below uses `sbcl --no-sysinit --no-userinit` so the pinned environment is the
only provenance. The probe refuses to run if the `:reazon` package already
exists before its own load step.

## Load commands

```sh
REAZON_SRC=<checkout> \
  sbcl --noinform --no-sysinit --no-userinit --disable-debugger \
  --load <ql>/.quicklisp/setup.lisp --script 2_PROBE.lisp
```

## Public API surface exercised

Exported: `== run run* fresh defrel conde disj conj disj-2 conj-2 conda condu
project appendo membero listo nullo pairo conso caro cdro *occurs-check*
circular-query` plus list relations. Constraints: none exported, none in
source. Streams are cons lists with thunks. `r-append` swaps streams when it
reaches a thunk (src/reazon.lisp:109), so productive recursive streams
interleave. An unproductive recursive stream can still block a request for
another answer.
Occurs check is a dynamic variable `*occurs-check*` (default `t`); enabling it
makes `extend` signal `circular-query` (src/reazon.lisp:71-77).
