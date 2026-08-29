# cl-grph Results

Upstream: cl-grph at
`d9d5eddeebf4eeaa2dcffc62791406961ed74e4f`, with cl-veq at
`d82dc83f8d275e36e516264b390c37cdb4d646d4`. Runtime: SBCL 2.6.7 arm64.

## Probe output

```text
PROBE library=grph version=d9d5eddeebf4eeaa2dcffc62791406961ed74e4f
UNIFY pattern=?x->1 all-edges=(0)
NESTED unsupported=absent error="qry macro compile error: bad qry clause ((0 EDGE 1) EDGE ?Y)"
OCCURS policy=absent-by-construction
PATH-RAW order=((0 3) (0 0) (2 2) (1 1) (2 3) (2 0) (1 2) (0 1) (1 3) (1 0)
                (0 2) (2 1))
PATH set=((0 0) (0 1) (0 2) (0 3) (1 0) (1 1) (1 2) (1 3) (2 0) (2 1)
          (2 2) (2 3)) mechanism=linear-fixpoint
FAIR starvation-shape=unsupported left-filter-called=NIL later-answer=(2 1 0)
RULE-ORDER base-first=closure recursive-first-terminated n=12
DUP raw=((0 3) (0 1) (0 2) (1 3) (2 3)) raw-count=5 undup-count=5
NOT no-out-edge=(2)
ORJOIN=(1 2)
UPDATE after-del-path=((0 0) (0 1) (0 2) (1 0) (1 1) (1 2) (2 0) (2 1) (2 2)) original-still-has-2-3=T
SERIALIZE verts-in=4 verts-out=4 closure-eq=T
COMPILER-FACTS verts=signed-byte-32 adapter=id-dictionary
COMPILER-FACTS symbol-vert-rejected="A is not of type (SIGNED-BYTE 32)"
BINARY 55582944
SECONDS closure-elapsed=0.003
```

The full nested-query diagnostic is emitted by the `qry` macro and includes
its compiled-query dump. The normalized line above preserves the rejected
clause and result without copying that formatting.

## Commands

```sh
export CL_GRPH_DIR=/private/tmp/cl-grph-lab/cl-grph
export VEQ_DIR=/private/tmp/cl-grph-lab/veq
sbcl --noinform --disable-debugger --script 2_PROBE.lisp
CL_GRPH_LAB_OUTPUT=/private/tmp/sprefa-v7-cl-grph-lab-d9d5edd-20260828-r3 \
  sbcl --noinform --disable-debugger --load 3_BUILD.lisp
export CL_GRPH_LAB_BINARY=/private/tmp/sprefa-v7-cl-grph-lab-d9d5edd-20260828-r3
"$CL_GRPH_LAB_BINARY"
wc -c < "$CL_GRPH_LAB_BINARY"
shasum -a 256 "$CL_GRPH_LAB_BINARY"
file "$CL_GRPH_LAB_BINARY"
otool -L "$CL_GRPH_LAB_BINARY"
for sample in 1 2 3 4 5; do /usr/bin/time -p "$CL_GRPH_LAB_BINARY" >/dev/null; done
/usr/bin/time -lp "$CL_GRPH_LAB_BINARY"
```

The probe validates both Git commits and rejects dirty upstream worktrees
before loading either library.

## Recursive-rule semantics

`rqry` separates base rules from one linear self-recursive rule. It runs the
recursive query against the previous tuple set, unions the result, and repeats
until the set stops growing or `:lim` is reached. The default limit is 1000.
The cyclic fixture reaches 12 distinct source and destination pairs and
terminates by set fixpoint.

The diamond fixture produces 5 result tuples and 5 distinct tuples. The engine
stores result tuples as a set, so proof multiplicity is discarded. Raw fset
iteration order is retained in `PATH-RAW`; `PATH` supplies the sorted
comparison boundary.

The starvation-shaped query put a host filter in the left `or` branch and a
finite edge pattern in the right branch. The host filter was not called and the
finite answers `(2 1 0)` were returned. This does not establish a fair lazy
search scheduler. `qry` evaluates finite set queries; `rqry` performs
bottom-up iteration.

## Graph and update model

Graph vertices are signed 32-bit integers. Edge properties may be symbols.
Compiler symbols therefore require an ID dictionary when represented as
vertices. `add` and `del` return new immutable graph values. The old graph
retains edge `2 -> 3` after the derived graph deletes it. Recursive results
are recomputed after an update.

`gwrite` and `gread` round-trip the four-vertex fixture. The probe removes
its temporary `.grph` file through `unwind-protect`.

## Capability classification

| Capability | Result | Receipt |
| --- | --- | --- |
| nested term unification | absent-from-probe | only fixed triples are terms; compound subjects fail macro compilation |
| occurs check | absent-from-probe | variables bind only signed 32-bit vertex positions in a triple |
| multiple answers | native | raw path and diamond records; fset iteration order |
| fair search | absent-from-probe | starvation-shaped host-filter branch was not evaluated |
| cyclic transitive closure | native | linear bottom-up set fixpoint, bounded by `:lim` |
| Datalog fixpoint | native | base, simple, and one linear recursive rule |
| tabling | absent-from-probe | no variant, subsumptive, or answer-subsumption tables |
| constraints | absent-from-probe | host filters are predicates, with no constraint store |
| dynamic facts and retraction | native | immutable `add` and `del`; recursive result recomputed |
| standalone image | built | retained SBCL executable and receipts below |

Non-linear recursion is rejected by `rqry`. Recursive negation, incremental
retraction, proof trees, and first-order substitution objects are absent from
the probe.

## Relevant source locations

| Facility | Source |
| --- | --- |
| graph representation and updates | `src/grph.lisp`, `src/edge-set.lisp` |
| triple matching and enumeration | `src/qry-match.lisp`, `src/qry-runtime.lisp`, `src/qry-runtime-2.lisp` |
| query validation and lowering | `src/qry.lisp` |
| recursive rule evaluation | `src/qry-rules.lisp` |
| parallel query path | `src/qry-runtime-par.lisp` |
| serialization | `src/xgrph-io.lisp` |
| persistent query cache | absent |

## Standalone image

Retained executable:
`/private/tmp/sprefa-v7-cl-grph-lab-d9d5edd-20260828-r3`

| Measurement | Value |
| --- | --- |
| executable bytes | 55,582,944 |
| SHA-256 | `c3693384ea6456f0df133bed6010ffb9df091aa7506322d6ba8fa0194025a676` |
| format | Mach-O 64-bit executable arm64 |
| dynamic libraries | `/usr/lib/libSystem.B.dylib`, `/opt/homebrew/opt/zstd/lib/libzstd.1.dylib` |
| startup plus full probe, 5 samples | 0.67, 0.19, 0.18, 0.19, 0.18 seconds |
| peak RSS | 119,603,200 bytes |
| source loading and compilation | available; `qry` compiles query forms at call sites |

## Raw executable receipts

```text
55582944
c3693384ea6456f0df133bed6010ffb9df091aa7506322d6ba8fa0194025a676  /private/tmp/sprefa-v7-cl-grph-lab-d9d5edd-20260828-r3
/private/tmp/sprefa-v7-cl-grph-lab-d9d5edd-20260828-r3: Mach-O 64-bit executable arm64
/private/tmp/sprefa-v7-cl-grph-lab-d9d5edd-20260828-r3:
    /usr/lib/libSystem.B.dylib
    /opt/homebrew/opt/zstd/lib/libzstd.1.dylib
real 0.67
real 0.19
real 0.18
real 0.19
real 0.18
119603200  maximum resident set size
```

Startup and resident-set measurements are per-run observations. File cache,
allocator state, and scheduler load change later measurements without changing
the executable checksum.

## DL7 compiler-rule gap

Direct DL7 compiler-fact execution would require an interned integer ID
dictionary, a triple encoding for compiler edges, stable answer sorting, and a
rule-loading path that produces `qry` or `rqry` forms at macro expansion.
Rules outside `rqry`'s single-linear-recursion subset require another
fixpoint driver.
