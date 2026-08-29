# cl-prolog2 bridge results

## Run boundary

| field | value |
| --- | --- |
| start | `2026-08-28T22:02:39Z` |
| stop | `2026-08-28T22:10:36Z` |
| elapsed | 7 minutes 57 seconds |
| Common Lisp | SBCL 2.6.7 |
| Prolog | SWI-Prolog 10.0.2 for arm64-darwin |
| library | cl-prolog2 0.1 at `21531c553208e01c0b0b205ea005afaefa7057e3` |

## Reproduction setup

```sh
task_tmp=/private/tmp/cl-prolog2-lab.uuvw4c
export ASDF_OUTPUT_TRANSLATIONS="(:output-translations (t \"$task_tmp/fasl-cache/\") :ignore-inherited-configuration)"
export CLP2_QUICKLISP_SETUP="$task_tmp/quicklisp/setup.lisp"
export CLP2_UPSTREAM="$task_tmp/upstream/"
export CLP2_LAB_IMAGE="$task_tmp/image/cl-prolog2-lab-11-r2"
```

Source-loaded fixture:

```sh
test "$(git -C "$CLP2_UPSTREAM" rev-parse HEAD)" = \
  21531c553208e01c0b0b205ea005afaefa7057e3
test -z "$(git -C "$CLP2_UPSTREAM" status --porcelain)"
sbcl --noinform --disable-debugger --no-userinit --no-sysinit \
  --eval "(load #P\"$CLP2_QUICKLISP_SETUP\")" \
  --eval "(asdf:load-asd #P\"$CLP2_UPSTREAM/cl-prolog2.asd\")" \
  --eval "(asdf:load-asd #P\"$CLP2_UPSTREAM/swi/cl-prolog2.swi.asd\")" \
  --eval '(asdf:load-system "cl-prolog2.swi")' \
  --load 2_PROBE.lisp \
  --eval '(cl-prolog2-lab-11:run-probe)' \
  --eval '(quit)'
```

Image build and run:

```sh
sbcl --noinform --disable-debugger --no-userinit --no-sysinit --load 3_BUILD.lisp
"$CLP2_LAB_IMAGE"
```

## Saved-image fixture result

The fixture declares `table path/2`, has the shared cyclic `edge` graph, collects `setof/3` and `findall/3` answers, checks `X = f(X)` and `unify_with_occurs_check/2`, catches an exception in Prolog, retracts `edge(c,d)`, calls `abolish_all_tables/0`, and repeats the path query.

```text
PROBE library=cl-prolog2 version=0.1 backend=swi swipl=10.0.2
UNIFY f(g(a))
OCCURS standard-rational-tree=success occurs-check=fail
PATH (a b c d)
ANSWERS (a b c d)
NEGATIVE true
FAIR absent-from-probe
EXCEPTION bridge-probe
UPDATE (a b c)
BINARY 45554408
```

`PATH` is sorted by SWI `setof/3`. `ANSWERS` passes the `findall/3` result
through SWI `sort/2` before printing so repeated runs have one canonical
record. The single negative query is `\\+ path(a,z)`.

## Capability classification

| capability | result | observed policy or mechanism |
| --- | --- | --- |
| nested term unification | `external-runtime` | SWI unifies the serialized `f(g(a))` term. |
| occurs check | `external-runtime` | standard `=/2` accepted `X = f(X)` as a rational tree; `unify_with_occurs_check/2` failed. |
| multiple answers | `external-runtime` | `findall/3` collected the tabled `path(a,Y)` answers; `sort/2` canonicalized them to `(a b c d)`. |
| fair search | `absent-from-probe` | no fair-search scheduler or starvation probe is exposed by cl-prolog2's batch API. |
| cyclic transitive closure | `external-runtime` | SWI `table path/2` terminates the cycle; `setof/3` returned `(a b c d)`. |
| Datalog fixpoint | `absent-from-probe` | no Datalog evaluator was invoked. |
| tabling | `external-runtime` | SWI `table/1` was applied to `path/2`; the fixture resets tables with `abolish_all_tables/0` after retraction. |
| constraints | `absent-from-probe` | no constraint domain was invoked. |
| dynamic facts and retraction | `external-runtime` | `dynamic edge/2`, `retract(edge(c,d))`, table reset, then `UPDATE (a b c)`. |
| standalone image | `built` | SBCL image was saved and executed; its Prolog work remains an external `swipl` process. |

## Transport and lifecycle trace

The following source locations define the transport:

| concern | upstream source | observed behavior |
| --- | --- | --- |
| S-expression to Prolog text | `src/printers.lisp` | symbols and terms are printed as Prolog source; a variable is any symbol whose name starts with `?` or `_`. |
| variable identity | `src/printers.lisp`, `README.md` | variables are name-based source text. The printer converts the first character to `_` and non-alphanumerics to `_`; `?a-b-c` and `?a_b_c` therefore collide as `_a_b_c`. |
| temporary program | `src/interpreter.lisp:21`, `swi/package.lisp:24-29` | creates a temporary directory and `XXXXXX.prolog`, then writes all rules. |
| process launch | `swi/package.lisp:30-32` | `swipl --quiet -l <temporary-program>` through `uiop:run-program`. |
| multiple solutions | `src/interpreter.lisp:19`, `swi/package.lisp:15` | `run-prolog` returns one complete stdout string; there is no parsed answer-stream or query handle. |
| exception boundary | `src/interpreter.lisp:43-50` | subprocess aborts are reported by UIOP. The fixture's `throw(bridge-probe)` was caught and serialized by Prolog as `EXCEPTION bridge-probe`. |
| threading | `rg -n -i 'thread|cffi|foreign|libswipl|query' src swi` returned no matches | no host-thread, attached-engine, foreign-frame, or query lifecycle API is present in these source directories. |
| runtime loading | `swi/package.lisp` and `otool -L` | the image runs `swipl`; `otool` lists no `libswipl`. |

The current r2 receipts at
`/private/tmp/cl-prolog2-lab.uuvw4c/image-r2-run.out` and
`image-r2-run.trace` contain the canonical output above, the emitted `sort/2`
rule, and the exact `swipl --quiet -l ...` command. Replays generated their
debug programs under the system temporary directory. The image completed with
the ordinary system `mktemp`, without the temporary compatibility wrapper.

## Image receipt

```text
path: /private/tmp/cl-prolog2-lab.uuvw4c/image/cl-prolog2-lab-11-r2
bytes: 45554408
sha256: 8d4af37c3a891b73ee969a81de8613917d9a058c24e05a46c93700eed2dac53e
file: Mach-O 64-bit executable arm64
otool -L:
  /usr/lib/libSystem.B.dylib
  /opt/homebrew/opt/zstd/lib/libzstd.1.dylib
```

The build rejects a dirty checkout, a commit other than the recorded pin, and
an image path outside `/private/tmp`. The executable did not load source files
during its test run. Runtime dependencies are `swipl` on `PATH`,
`/usr/lib/libSystem.B.dylib`, and
`/opt/homebrew/opt/zstd/lib/libzstd.1.dylib`.

## Measurements

Five startup samples are recorded. Each executes the full fixture and launches
one `swipl` child process. The RSS sample was collected in the same full-image
shape.

The bridge benchmark command loads the same pinned systems and probe as the
source command above, then evaluates:

```lisp
(format t "~F~%" (cl-prolog2-lab-11:run-bridge-benchmark 20))
```

| measurement | raw result |
| --- | --- |
| image startup samples | `0.11, 0.11, 0.11, 0.11, 0.11` real seconds |
| image RSS sample | `0.11 real` seconds; `52690944` maximum resident set size bytes |
| bridge benchmark | 20 minimal `run-prolog` calls: `3.669` seconds total; `183.472` milliseconds per query |

The benchmark fixture writes `ok`, halts, and starts a new `swipl` process for each iteration. It measures the batch bridge plus process startup and excludes SBCL system loading.

## Remaining implementation boundary

Unification, proof search, rule evaluation, tabling, constraints, and caching reside in the selected Prolog runtime. cl-prolog2 source files implement Prolog text printing, temporary-file construction, and child-process invocation. A DL7 rule path still needs structured term/result decoding, collision-free variable identities, query/session ownership, explicit table lifetime and update policy, source-location transport, error conversion, and any required fair-search, Datalog, constraint, or incremental-update semantics.

## Stop boundary and blockers

The lab stopped inside the ten-minute boundary. The remaining blockers recorded by this probe are `absent-from-probe` fair search, Datalog fixpoint, and constraints; cl-prolog2 exposes no in-process SWI engine, typed query lifecycle, or host-thread API for those experiments. The first sandboxed RSS attempt could not read `kern.clockrate`; the required single RSS value was obtained by rerunning the same image command with permitted system measurement access.

## External artifact receipts

| artifact | external path | state |
| --- | --- | --- |
| upstream checkout | `/private/tmp/cl-prolog2-lab.uuvw4c/upstream` | pinned checkout, commit above |
| Quicklisp and dependencies | `/private/tmp/cl-prolog2-lab.uuvw4c/quicklisp` | Quicklisp dist `2026-01-01` |
| ASDF compiled output | `/private/tmp/cl-prolog2-lab.uuvw4c/fasl-cache` | external cache used for final builds and probes |
| generated Prolog programs and run traces | system temporary directories, `/private/tmp/cl-prolog2-lab.uuvw4c/image-r2-run.{out,trace}` | current r2 debug program and command trace retained for transport inspection |
| SBCL executable | `/private/tmp/cl-prolog2-lab.uuvw4c/image/cl-prolog2-lab-11-r2` | 45,554,408 bytes, SHA-256 `8d4af37c3a891b73ee969a81de8613917d9a058c24e05a46c93700eed2dac53e` |
| temporary `mktemp` wrapper | `/private/tmp/cl-prolog2-lab.uuvw4c/bin/mktemp` | retained from the initial macOS compatibility investigation; final image test did not use it |
