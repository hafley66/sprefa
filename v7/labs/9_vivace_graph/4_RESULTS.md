# VivaceGraph lab results

## Recorded environment

| Item | Value |
| --- | --- |
| Library | VivaceGraph 3.0.0, system `graph-db/core` |
| Upstream pin | `68230b3879c238b3c24b79a97fc06048841f4f0b` |
| Pin receipt | `git rev-parse HEAD` printed the pinned hash; `git status --porcelain` printed no rows |
| Runtime | SBCL 2.6.7, Homebrew arm64 |
| Dependency route | local Quicklisp cache at `/private/tmp/sprefa-v7-vivace-cache/.quicklisp/`, dist `2026-01-01` |
| Source checkout | `/private/tmp/sprefa-v7-vivace-graph` |
| Database directories | fresh `mktemp -d /private/tmp/sprefa-v7-vivace-*-db.XXXXXX` directories |

The probe calls `git rev-parse HEAD` and rejects any nonempty
`git status --porcelain` result. A source process rejects a preloaded
`GRAPH-DB` package. The build script records an internal provenance marker,
and image startup revalidates the external checkout against the recorded
commit. The mutable marker is a lab build receipt, not tamper-resistant
attestation.

## Commands executed

```sh
VIVACE_SRC=/private/tmp/sprefa-v7-vivace-graph \
QL_SETUP=/private/tmp/sprefa-v7-vivace-cache/.quicklisp/setup.lisp \
VIVACE_DB=$(mktemp -d /private/tmp/sprefa-v7-vivace-db.XXXXXX) \
sbcl --noinform --no-sysinit --no-userinit --disable-debugger \
  --script v7/labs/9_vivace_graph/2_PROBE.lisp

VIVACE_SRC=/private/tmp/sprefa-v7-vivace-graph \
QL_SETUP=/private/tmp/sprefa-v7-vivace-cache/.quicklisp/setup.lisp \
VIVACE_OUT=/private/tmp/sprefa-v7-vivace-lab-r5 \
sbcl --noinform --no-sysinit --no-userinit --disable-debugger \
  --script v7/labs/9_vivace_graph/3_BUILD.lisp

VIVACE_SRC=/private/tmp/sprefa-v7-vivace-graph \
VIVACE_DB=$(mktemp -d /private/tmp/sprefa-v7-vivace-image-db.XXXXXX) \
VIVACE_BIN=/private/tmp/sprefa-v7-vivace-lab-r5 \
/private/tmp/sprefa-v7-vivace-lab-r5

wc -c /private/tmp/sprefa-v7-vivace-lab-r5
shasum -a 256 /private/tmp/sprefa-v7-vivace-lab-r5
file /private/tmp/sprefa-v7-vivace-lab-r5
otool -L /private/tmp/sprefa-v7-vivace-lab-r5
```

## Successful source and image receipts

The source probe and saved image produced the same capability receipt. The
saved image verifies the runtime checkout against the recorded pin before
running the probe.

```text
PROBE library=vivace-graph version=3.0.0 commit=68230b3879c238b3c24b79a97fc06048841f4f0b
UNIFY ((A B))
OCCURS occurs-check=compiled-present result=NIL
ROLLBACK names=NIL
COMMIT names=("A" "B" "C" "D") edges=("A>B" "B>C" "C>A" "C>D")
INDEX lookup=A rows=("A")
PATH direct=("A>B" "B>C" "C>A" "C>D") cycle=bounded=Prolog error: inference budget exceeded (64). adapter=("A" "B" "C" "D")
DUPES raw=("B" "C" "B") unique=("B" "C")
UPDATE retract=D names=("A" "B" "C") index=D rows=NIL
REOPEN names=("A" "B" "C") index=A rows=("A") edges=("A>B" "B>C" "C>A") db-kib=2768
BINARY 65873672
IMAGE source-load=T compile=T
```

The cyclic query has `:max-inferences 64` and `:timeout 1`. The finite closure
uses an explicit host-side visited set keyed by persistent node ID and is
recorded as an adapter. The
secondary index is declared by the `:index t` node slot and queried with
`index-lookup`. The 2,768 KiB database footprint is from `du -sk` after the
four-node fixture, D retraction, and graph reopen.

## Image measurements

| Metric | Receipt |
| --- | --- |
| Image | `/private/tmp/sprefa-v7-vivace-lab-r5` |
| Executable bytes | 65,873,672 |
| SHA-256 | `a21045518382ad210bd48e76761ad0a08b3edc427b677ce8b348d5e0beadf7b6` |
| Format | Mach-O 64-bit executable arm64 |
| Dynamic libraries | `/usr/lib/libSystem.B.dylib`; `/opt/homebrew/opt/zstd/lib/libzstd.1.dylib` |
| Startup samples | 0.51, 0.53, 0.52, 0.51, 0.52 seconds wall |
| Peak RSS | 477,839,360 bytes from one `/usr/bin/time -l` run |
| Retained CL facilities | `LOAD` and `COMPILE` remain fbound; the probe does not invoke either facility |
| Runtime external files | pinned VivaceGraph checkout for commit verification; fresh graph database directory |

## Capability classification

| Capability | Result | Recorded behavior |
| --- | --- | --- |
| nested term unification | native | `UNIFY ((A B))` from `f(?x,g(?y)) = f(a,g(b))` |
| occurs check | native | compiled unification rejects `?x = f(?x)`, receipt `NIL`; `compile-unify-variable` has the `find-anywhere` guard |
| multiple answers | native | Prolog clause search returns proof-path rows; duplicate fixture returns `("B" "C" "B")` |
| fair search | absent-from-probe | no productive-versus-starving stream receipt was run |
| cyclic transitive closure | adapter | native recursive query reaches its explicit inference bound; visited-set closure returns `("A" "B" "C" "D")` |
| Datalog fixpoint | adapter | finite visited-state closure, not a native bottom-up rule evaluator |
| tabling | absent-from-probe | no call table, answer table, or SCC completion receipt; `*seen-table*` supports `unique/1`, not recursive tabling |
| constraints | absent-from-probe | no constraint-domain probe |
| dynamic facts and retraction | native | `UPDATE` removes D, its index lookup returns `NIL`, and the reopened graph retains three nodes |
| standalone image | built | `r5` executes the current probe without Quicklisp at runtime and reports its own 65,873,672-byte path; commit verification requires the external checkout |

## Source locations

| Concern | Files and symbols |
| --- | --- |
| unification | `prologc.lisp:131` `unify`; `prologc.lisp:332` `compile-unify`; `prologc.lisp:362` `compile-unify-variable` |
| user rules and query compilation | `prologc.lisp:626` `add-clause`; `:744` `<-`; `:951` `select` |
| graph lifecycle | `graph.lisp:292` `make-graph`; `:485` `open-graph`; `:701` `close-graph` |
| transaction semantics | `transactions.lisp:317` `with-transaction` |
| durable index | `index.lisp:421` `def-index`; `:491` `index-lookup` |
| query graph predicates and retraction | `prolog-functors.lisp`, `is-a/2`, generated edge functors, `retract/1`, `retract/3` |

## Bounded omissions

Fair-search and constraint behavior remain `absent-from-probe`; this lab did
not execute a productive-versus-starving stream or a constraint-domain case.
