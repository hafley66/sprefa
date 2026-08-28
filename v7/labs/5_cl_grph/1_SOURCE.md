# cl-grph Source

| Item | Value |
| --- | --- |
| Upstream | https://github.com/inconvergent/cl-grph |
| Commit pinned | `d9d5eddeebf4eeaa2dcffc62791406961ed74e4f` (2026-01-13, "bugfix in xgrph:2cut-to-area. minor new features") |
| License | MIT (LICENSE in repo root) |
| Systems | `grph`, `grph/tests` (`grph.asd`) |
| Packages | `grph` (graph + Datalog query compiler), `xgrph` (spatial extension), `grph/io` (serialization) |
| SBCL | 2.6.7, Homebrew arm64 |

## Dependencies

| Dependency | Version | Source |
| --- | --- | --- |
| veq | 6.2.5 | git `https://github.com/inconvergent/cl-veq` @ `d82dc83f8d275e36e516264b390c37cdb4d646d4` (2026-01-13) |
| fset | quicklisp dist 2026-01 | quicklisp |
| lparallel | quicklisp dist 2026-01 | quicklisp |
| prove | quicklisp (tests only) | quicklisp |

The grph README requires cl-veq from GitHub, not quicklisp. This is load-bearing: quicklisp ships cl-veq 4.5.5, which lacks `veq:ungroup` (imported by grph's `defpackage`) and `veq:lpos` (used by `qry.lisp` and `qry-rules.lisp`).

## Dependency install route (one route, project-local, pinned)

Checkouts are pinned to executable `git rev-parse` validation; `2_PROBE.lisp` re-verifies both at every load and errors on mismatch.

```sh
mkdir -p /tmp/cl-grph-lab
git clone https://github.com/inconvergent/cl-grph /tmp/cl-grph-lab/cl-grph
git -C /tmp/cl-grph-lab/cl-grph checkout --detach d9d5eddeebf4eeaa2dcffc62791406961ed74e4f
git clone https://github.com/inconvergent/cl-veq /tmp/cl-grph-lab/veq
git -C /tmp/cl-grph-lab/veq checkout --detach d82dc83f8d275e36e516264b390c37cdb4d646d4

# executable validation (must print the pinned hashes above)
git -C /tmp/cl-grph-lab/cl-grph rev-parse HEAD
git -C /tmp/cl-grph-lab/veq rev-parse HEAD

curl -fL -o /tmp/cl-grph-lab/quicklisp.lisp https://beta.quicklisp.org/quicklisp.lisp
sbcl --noinform --disable-debugger --load /tmp/cl-grph-lab/quicklisp.lisp \
  --eval '(quicklisp-quickstart:install :path "/tmp/cl-grph-lab/.quicklisp/")' --quit
```

`2_PROBE.lisp` is self-loading. It validates both commits and clean worktrees at
every load. It rejects a process that already contains the `grph` package, then
loads the project-local quicklisp, re-initializes the ASDF source registry to
prefer the pinned veq checkout, and loads `veq` and `grph`. Checkout locations
can be overridden with the `CL_GRPH_DIR` and `VEQ_DIR` environment variables.
Their defaults are `/tmp/cl-grph-lab/cl-grph` and `/tmp/cl-grph-lab/veq`.

## Key source files (grep targets from 4_RESULTS.md questions)

| Concern | File |
| --- | --- |
| graph core, immutable fset maps | `src/grph.lisp`, `src/edge-set.lisp` |
| query matcher / pattern match | `src/qry-match.lisp`, `src/qry-runtime.lisp`, `src/qry-runtime-2.lisp` |
| query front end (`qry` macro) | `src/qry.lisp` |
| rules / fixpoint (`rqry`) | `src/qry-rules.lisp` |
| parallel query lane | `src/qry-runtime-par.lisp` |
| graph algorithms | `src/grph-queries.lisp`, `src/grph-walk.lisp`, `src/xgrph-cycle.lisp` |
| serialization | `src/xgrph-io.lisp` (`grph/io`) |

`grph` does not implement unification. There is one unification-adjacent file (`src/qry-match.lisp`) but it is triple-pattern matching over `(subj pred obj)` with variables (`?x`) and wildcards (`_`), with no compound terms and no occurs check.
