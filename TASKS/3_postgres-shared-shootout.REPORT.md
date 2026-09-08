# PostgreSQL shared shootout integration

Run date: 2026-09-07. Branch: `feature/postgres-ivm-crossover`. Base:
`c27c0c007`.

## Integration contract

The primary entrypoint is `v6/sprefa-store/bench/run.sh`. PostgreSQL-family
adapters participate in its existing engine loop, `layers x width` graph,
root-0 retraction, CSV schema, chart generator, and generated report.

For every cell, an adapter must:

1. Generate the existing DAG exactly, including roots 0 and 1.
2. Materialize the reachable-node set from both roots during setup.
3. Delete root 0 and materialize the survivor set from root 1.
4. Compare both complete ordered sets with an independent BFS oracle.
5. Emit the existing 11-column CSV row only after exact agreement.

Ordinary PostgreSQL and PGlite use recursive SQL full recomputation. Setup and
retraction remain separate timing phases. `pg_ivm` recursive maintenance is an
explicit unsupported status because pg_ivm rejects recursive view definitions.

The existing default run remains unchanged. PostgreSQL-family arms are opt-in,
and opt-in receipts use a new destination so prior lab receipts remain
byte-identical. A disposable native cluster and private Unix socket are scoped
to one harness invocation. PGlite data directories are disposable and scoped
to individual cells.

## Pre-implementation inventory

The current engine array contains `swi-incr`, `swipl-pure`, `swi-sqlite`,
`swi-ts`, `swi-emit`, `tsv2-gen`, and `v1-gen`. It does not contain a Rust
Differential Dataflow executable. `report.sh` has stale prose naming `dd` and
`dbsp`; generated reporting must identify the engines actually present and
distinguish full-recomputation arms.

## Executed result

Pending implementation and measurement.
