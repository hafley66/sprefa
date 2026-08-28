# reazon-cl lab results

## Environment

| Item | Value |
| --- | --- |
| Library | reazon-cl, upstream commit `3c4e9d916f2e621a3cc759f58ad778473f9da513` (2022-09-27, clean checkout, no dirty files) |
| License | GPL-3.0 (LICENSE file); `reazon-cl.asd` header says MIT, which contradicts the LICENSE |
| Runtime | SBCL 2.6.7 (Homebrew arm64) |
| Host | macOS (Darwin, arm64) |
| Dependency route | one route: lab-local Quicklisp at `/var/folders/z2/cwfm40fn65n176q8m227wl0r0000gn/T/opencode/reazon-ql/.quicklisp/` supplying `trivia` 0.1 release `trivia-20260101-git`; archive SHA-256 `81f5eacce946f0ffd713f3ecfc97c92dcf5cf1773cbad12cbf378905e24d4913`; no global Quicklisp mutation |
| Checkout location | `/var/folders/z2/cwfm40fn65n176q8m227wl0r0000gn/T/opencode/reazon-cl` (temporary; not in git) |
| Provenance enforcement | probe refuses a preloaded `:reazon` package, verifies the clean Git pin, verifies the loaded Reazon pathname, verifies the Trivia archive hash and loaded pathname, and emits its own source SHA-256 |

## Commands

```sh
REAZON_SRC=<checkout> QL_SETUP=<setup.lisp> \
  sbcl --noinform --no-sysinit --no-userinit --disable-debugger --script 2_PROBE.lisp

REAZON_SRC=<checkout> QL_SETUP=<setup.lisp> \
  REAZON_LAB_DIR=<lab dir> REAZON_LAB_OUT=/private/tmp/sprefa-v7-reazon-lab-image/reazon-cl-lab-r4 \
  sbcl --noinform --no-sysinit --no-userinit --disable-debugger --script 3_BUILD.lisp

REAZON_SRC=<checkout> QL_SETUP=<setup.lisp> \
  REAZON_LAB_BINARY=/private/tmp/sprefa-v7-reazon-lab-image/reazon-cl-lab-r4 \
  /private/tmp/sprefa-v7-reazon-lab-image/reazon-cl-lab-r4
```

`--no-sysinit` is required: `~/.sbclrc` currently auto-loads a Quicklisp setup
from `/private/tmp/cl-grph-lab/.quicklisp/` left by an earlier lab. Without
bypassing it, a preloaded Quicklisp could satisfy `trivia` from an unpinned
provenance. The init file should be cleaned up (outside this lab's scope).

## Probe output

The source and image runs agree on every behavioral line. `BINARY` reports
`blocked:not-built` without an executable path and measures the supplied path
inside the saved image.

```text
PROBE library=reazon-cl version=3c4e9d916f2e621a3cc759f58ad778473f9da513
PROVENANCE trivia-version=0.1 trivia-archive-sha=81f5eacce946f0ffd713f3ecfc97c92dcf5cf1773cbad12cbf378905e24d4913 probe-sha=e6acc5834681cc24f005e84b6ae099a3e69841aced251e9affa408ed3bae0012
UNIFY u=A v=B
OCCURS occurs-check=T policy=dynamic-default result=CIRCULAR-QUERY-ERROR
ORDER raw=Z A sorted=A Z
APPEND-LHS (A B)
APPEND-RHS (C D)
FAIR-PRODUCTIVE cap=20 answers=YES count=20 done-reached=T
FAIR-STARVE first=(DONE) second=TIMEOUT
PATH raw=(B C A D B C)
PATH-SORTED (A B B C C D) count=6
DUPES count=2 sorted-unique=(A B C D)
NEG (D)
UPDATE after-retract=(A B B C C)
UPDATE after-reassert=(A B B C C D)
CONSTRAINTS absent-from-probe
BINARY 44309032
```

Notes on the raw lines:

- UNIFY: nested `f(q, g(r)) = f(a, g(b))` binds `q=A`, `r=B` in one answer; car/cdr recursion over conses (src/reazon.lisp:79-100).
- OCCURS: `*occurs-check*` is a dynamic variable, default `t`; `extend` signals `circular-query` when enabled (src/reazon.lisp:71-77). `X = (X)` therefore fails cleanly with `CIRCULAR-QUERY-ERROR`. Setting the variable to `nil` globally disables the check; the probe keeps the default.
- ORDER: the host list is arranged z→b then a→b; raw stream answers `(Z A)`, preserving host list order as each pair becomes one `disj-2` clause.
- APPEND: both directions of the exported `appendo` give the expected single answer, `(A B)` and `(C D)`.
- FAIR-PRODUCTIVE: `r-append` swaps streams at thunk boundaries (src/reazon.lisp:109-122, 141-144). Twenty pulls from `alwayso OR done` include `DONE`.
- FAIR-STARVE: an unproductive recursive branch lets the later `DONE` answer through, then a request for another answer blocks. The one-second wall bound fires.
- PATH: cyclic `edge` fixture a→b→c→a, c→d; `patho` is a depth-bound adapter (ground depth decremented at call time), depth 4. Raw stream `(B C A D B C)`: depth-first, duplicates from distinct proof paths.
- DUPES: 6 raw answers, 2 duplicates, unique set `{A B C D}` including A via the cycle. No dedup in the library.
- NEG: bounded negation is a host-side adapter (run 1 per candidate); node D has no outgoing edge.
- UPDATE: fact store is a host list rebuilt into a `disj-2` per query; retracting c→d shrinks the closure to `{A B C}`, re-asserting restores `{A B C D}`. No incremental invalidation exists; each query re-searches.
- CONSTRAINTS: no constraint operators exported or defined in src/reazon.lisp.

## Saved executable image measurements

| Metric | Value |
| --- | --- |
| Image | `reazon-cl-lab-r4`, Mach-O 64-bit executable arm64, `sb-ext:save-lisp-and-die :executable t` |
| Executable bytes | 44,309,032 |
| SHA-256 | `438681574716742538ee8ae756a439d23f992c6a03e470c09ccd1ee803a8ed6f` |
| Embedded probe SHA-256 | `e6acc5834681cc24f005e84b6ae099a3e69841aced251e9affa408ed3bae0012` |
| Dynamic deps (`otool -L`) | `/usr/lib/libSystem.B.dylib`, `/opt/homebrew/opt/zstd/lib/libzstd.1.dylib` (Homebrew zstd, non-system) |
| Startup samples (wall, `/usr/bin/time -p`) | 1.15, 1.14, 1.14, 1.14, 1.13 s |
| Peak RSS (`/usr/bin/time -lp`) | 109,428,736 bytes maximum resident set size |
| Smoke test | image run exits 0 and reproduces the probe transcript above |
| Source/compilation in image | available: the image is a full SBCL save containing the pinned ASDF system; its toplevel rechecks the Git pin, clean tree, and loaded ASDF source pathname before running the probe |
| External runtime files | pinned Reazon checkout and verified Quicklisp Trivia release remain required for runtime provenance checks |

## Capability classification

| Capability | Result | Detail |
| --- | --- | --- |
| nested term unification | native | cons-recursive `unify` with `walk` (src/reazon.lisp:50-100) |
| occurs check | native | dynamic `*occurs-check*`, default on, signals `circular-query` for `X = (X)`; exact observed policy: on by default, host-togglable, error-signaling |
| multiple answers | native | cons-stream + thunks; host-list order is preserved for finite streams (ORDER raw line) |
| fair search | native | `r-append` interleaves productive thunked streams; an unproductive branch blocks a request for another answer after the later finite answer is consumed |
| cyclic transitive closure | adapter | termination via explicit ground depth bound carried as an argument; the library itself has no tabling, no visited-state, no termination mechanism |
| Datalog fixpoint | absent-from-probe | the depth-bounded `patho` probe does not implement bottom-up saturation |
| tabling | absent-from-probe | no call/answer tables, no subsumption, no SCC completion |
| constraints | absent-from-probe | no constraint store or domains |
| dynamic facts and retraction | adapter | facts are a host-side list recompiled into a disjunction per query; retraction = setq + rebuild; no incremental behavior |
| saved executable image | built | 44,309,032 bytes, one non-system dylib (zstd), full SBCL save; external checkout and Trivia files required for provenance checks |

## Report questions

1. **SWI coverage directly:** first-order unification with occurs check, multiple answers over thunked streams, relational append/list relations, committed choice (`conda`/`condu`), and the `run`/`run*` reification interface.
2. **Adapters needed:** fact store and retraction (host list recompiled per query), termination bounds for recursive queries (depth cap or visited-state), bounded negation (host-level run-1 filter), proof deduplication at the adapter boundary.
3. **Cyclic recursion:** the library alone diverges; termination here came from a depth-bound adapter (the depth counter is ground at call time, decremented per recursive branch). No tabling exists to provide semantic termination.
4. **SBCL image:** yes, built successfully with `save-lisp-and-die`.
5. **Measurements:** 44,309,032 bytes; deps `libSystem.B.dylib` + `libzstd.1.dylib`; startup 1.13 to 1.15 s across five samples; peak RSS 109,428,736 bytes.
6. **Implementing files:** all of the engine lives in one file, src/reazon.lisp: unification = `walk/occurs-p/extend/unify` (50-107); stream/search = `r-append/append-map/disj-2/conj-2` (109-157); reification = `reify-name/reify-sub/walk*/reify` (182-214); drivers = `pull/take/run/run*` (216-264); relations = `defrel` + list relations (278-438); constraints: none.
7. **Before DL7 compiler rules could run:** add a tabling or fixpoint layer, an incremental fact store with invalidation, proof deduplication, handling for unproductive recursive branches, and an explicit symbol-package policy at the phase-0 boundary. Variables use symbols as `equal` hash keys in `make-variable` (src/reazon.lisp:33-42), so package-qualified symbols remain distinct.
