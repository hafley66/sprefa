# Screamer Results

Upstream: Screamer 4.0.0 at
`ce50614024de090b376107668da5e53232540ec7`. Runtime: SBCL 2.6.7 arm64.

## Probe output

```text
PROBE library=screamer version=4.0.0 commit=ce50614024de090b376107668da5e53232540ec7
ORDER raw=(FIRST SECOND THIRD)
UNIFY ok=T values=(B A)
OCCURS policy=adapter-on cyclic-unify=FAILS
APPEND forward=(A B C) backward=((NIL (A B C)) ((A) (B C)) ((A B) (C))
                                 ((A B C) NIL)) mechanism=nondeterministic-adapter
PATH raw=(B C A B D) sorted=(A B C D) mechanism=depth-bound
NEGATION raw=(D) sorted=(D) mechanism=finite-closed-world-adapter
UPDATE sorted=(A B C) mechanism=replace-fact-list
FAIR left-branch=timed-out later-answer=reachable
CONSTRAINT domain=finite-integer answers=((1 4) (2 3))
VARIABLE identity-distinct=T left-bound=NIL right-bound=NIL
BINARY 43653576
```

The direct script prints `BINARY blocked:not-built`. The retained executable
prints the byte count when `SCREAMER_LAB_BINARY` names that executable.

## Commands

```sh
export SCREAMER_SRC=/private/var/folders/z2/cwfm40fn65n176q8m227wl0r0000gn/T/opencode/screamer-checkout/screamer
export XDG_CACHE_HOME=/private/tmp/sprefa-v7-screamer-cache
sbcl --noinform --disable-debugger --script 2_PROBE.lisp
SCREAMER_LAB_OUTPUT=/private/tmp/sprefa-v7-screamer-lab-ce50614-20260828-r4 \
  sbcl --noinform --disable-debugger --load 3_BUILD.lisp
export SCREAMER_LAB_BINARY=/private/tmp/sprefa-v7-screamer-lab-ce50614-20260828-r4
"$SCREAMER_LAB_BINARY"
wc -c < "$SCREAMER_LAB_BINARY"
shasum -a 256 "$SCREAMER_LAB_BINARY"
file "$SCREAMER_LAB_BINARY"
otool -L "$SCREAMER_LAB_BINARY"
for sample in 1 2 3 4 5; do /usr/bin/time -p "$SCREAMER_LAB_BINARY" >/dev/null; done
/usr/bin/time -lp "$SCREAMER_LAB_BINARY"
```

The probe validates the upstream commit and rejects a dirty checkout before
loading Screamer.

## Observed semantics

`either` visits alternatives from left to right. `all-values` restores the
choice point and collects results. A divergent first alternative prevents the
later `:reachable` result; the 0.05 second SBCL timeout records
`timed-out`.

The finite-domain probe creates two integer variables in 1 through 4, asserts
`x < y` and `x + y = 5`, then forces solutions. The results are
`((1 4) (2 3))`.

The relation layer is adapter code in `2_PROBE.lisp`. Forward append produces
`(A B C)`; reverse splitting enumerates all four prefix and suffix pairs.
`edge-target` chooses a fact with native `a-member-of`; `path-end`
recursively chooses an edge or a longer path. An explicit depth argument bounds
the three-node cycle. The finite closed-world negative query returns node
`D`, which has no outgoing edge.

The first-order unifier and occurs check are adapter code. Screamer's native
solver variables carry finite-domain constraints and do not expose a generic
SWI-style attribute hook.

## Capability classification

| Capability | Result | Receipt |
| --- | --- | --- |
| nested term unification | adapter | `UNIFY ok=T values=(B A)` from the lab unifier |
| occurs check | adapter | policy is on; cyclic `X = f(X)` fails |
| multiple answers | native | left-to-right choice order, append splits, and two constraint solutions |
| fair search | implement | divergent left alternative starves the later finite alternative |
| cyclic transitive closure | adapter | explicit depth argument |
| Datalog fixpoint | absent-from-probe | no bottom-up rule evaluator |
| tabling | absent-from-probe | no variant, subsumptive, or answer-subsumption tables |
| constraints | native | finite integer domains with arithmetic propagation |
| dynamic facts and retraction | adapter | replacement of the lab fact list |
| attributed variables | native | solver constraints attach internally; no generic attribute API was found |
| standalone image | built | retained SBCL executable and receipts below |

## Relevant source locations

All engine facilities are in upstream `screamer.lisp`:

| Facility | Line at pinned commit |
| --- | ---: |
| trail | 104, 2702 |
| `either` | 2452 |
| `all-values` | 2619 |
| `fail` | 2825 |
| `a-member-of` | 3066 |
| `make-variable` | 3269 |
| `solution` | 5513 |
| `static-ordering` | 5598 |
| `assert!` | 6675 |
| `an-integer-betweenv` | 6880 |

Screamer has no Horn-rule evaluator, first-order unifier, or persistent result
cache.

## Standalone image

Retained executable:
`/private/tmp/sprefa-v7-screamer-lab-ce50614-20260828-r4`

| Measurement | Value |
| --- | --- |
| executable bytes | 43,653,576 |
| SHA-256 | `b33aab147a35c01424cea8fb248eff37eb9125ecc2e2deecc94e51d8cecad5e1` |
| format | Mach-O 64-bit executable arm64 |
| dynamic libraries | `/usr/lib/libSystem.B.dylib`, `/opt/homebrew/opt/zstd/lib/libzstd.1.dylib` |
| startup plus full probe, 5 samples | 0.06, 0.06, 0.07, 0.07, 0.07 seconds |
| peak RSS | 48,414,720 bytes |
| source loading and compilation | available in the saved SBCL image |

Every run includes the 0.05 second starvation bound.

## Raw executable receipts

```text
43653576
b33aab147a35c01424cea8fb248eff37eb9125ecc2e2deecc94e51d8cecad5e1  /private/tmp/sprefa-v7-screamer-lab-ce50614-20260828-r4
/private/tmp/sprefa-v7-screamer-lab-ce50614-20260828-r4: Mach-O 64-bit executable arm64
/private/tmp/sprefa-v7-screamer-lab-ce50614-20260828-r4:
    /usr/lib/libSystem.B.dylib
    /opt/homebrew/opt/zstd/lib/libzstd.1.dylib
real 0.06
real 0.06
real 0.07
real 0.07
real 0.07
48414720  maximum resident set size
```

## DL7 compiler-rule gap

Executing DL7 rules would require a relation store, Horn-rule representation,
first-order term unifier, recursive-rule scheduler, answer deduplication, and
fixpoint or table completion. Screamer supplies choice points, trail
restoration, answer collection, and finite-domain constraint propagation for
that adapter.
