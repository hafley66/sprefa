# VivaceGraph source record

## Selected upstream

| Field | Value |
| --- | --- |
| Repository | https://github.com/kraison/vivace-graph |
| Branch selected | `master` |
| Pinned commit | `68230b3879c238b3c24b79a97fc06048841f4f0b` |
| Commit | 2026-08-09T12:17:49+03:00, `Merge experiment into master: VivaceGraph 3.0.0` |
| Version | 3.0.0 (`graph-db.asd`) |
| License | GPL-3.0-or-later (`LICENSE`) |
| ASDF systems | `graph-db/core`, `graph-db`, `graph-db/algorithms`, `graph-db/test` |
| Implementation | SBCL 2.6.7, Homebrew arm64 |

The upstream repository calls the system `graph-db`. `graph-db/core` contains
storage, graph, Prolog query, transaction, and on-disk WAL components;
`graph-db` adds HTTP and replication transport.

## Isolated install route

```sh
git clone https://github.com/kraison/vivace-graph.git /private/tmp/sprefa-v7-vivace-graph
git -C /private/tmp/sprefa-v7-vivace-graph checkout --detach 68230b3879c238b3c24b79a97fc06048841f4f0b
test "$(git -C /private/tmp/sprefa-v7-vivace-graph rev-parse HEAD)" = 68230b3879c238b3c24b79a97fc06048841f4f0b
test -z "$(git -C /private/tmp/sprefa-v7-vivace-graph status --porcelain)"

mkdir -p /private/tmp/sprefa-v7-vivace-cache
curl -fsSL -o /private/tmp/sprefa-v7-vivace-cache/quicklisp.lisp https://beta.quicklisp.org/quicklisp.lisp
sbcl --noinform --no-sysinit --no-userinit --disable-debugger \
  --load /private/tmp/sprefa-v7-vivace-cache/quicklisp.lisp \
  --eval '(quicklisp-quickstart:install :path #P"/private/tmp/sprefa-v7-vivace-cache/.quicklisp/")' --quit
```

`2_PROBE.lisp` takes `VIVACE_SRC` and `QL_SETUP`, validates the exact commit
and a clean worktree on every load, then loads `graph-db.asd` directly and
uses the project-local Quicklisp setup for declared dependencies. `VIVACE_DB`
is an empty, isolated temporary graph directory. `VIVACE_BIN` names a saved
image when measuring it. No checkout, Quicklisp cache, database, FASL, or
image is placed under Git.

## Source locations exercised

| Concern | File | Mechanism |
| --- | --- | --- |
| graph lifecycle | `graph.lisp` | `make-graph`, `open-graph`, `close-graph` |
| ACID writes and rollback | `transactions.lisp` | `with-transaction`, validation, commit, rollback |
| node and edge schema | `node-class.lisp`, `edge.lisp`, `vertex.lisp` | `def-vertex`, `def-edge`, generated constructors |
| durable secondary index | `index.lisp` | `:index t`, `index-lookup`, ordered index sidecar |
| Prolog compiler and query bounds | `prologc.lisp` | `<-`, `select`, trail, inference and wall-clock bounds |
| graph-backed Prolog predicates | `prolog-functors.lisp` | `is-a/2`, generated edge functors, `unique/1`, `retract/1` and `/3` |

The Prolog evaluator compiles clauses into Common Lisp functions and uses a
trail for destructive variable bindings. `select` creates a `*seen-table*`
for explicit `unique/1`; this does not form a recursive call or answer table.
