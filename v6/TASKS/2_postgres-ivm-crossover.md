# PostgreSQL crossover and memory lab

Base commit: `2c4614e8d`.

Use the task worktree only. Setup is complete; do not install dependencies or
build unchanged components. Build only what this task changes. Use Sol high and
commit frequently. Do not change compiler or kernel code, merge, push, use
production services, or change global settings.

Inventory existing Docker, Podman, Colima, and Linux facilities read-only. Use
an existing usable runtime for isolated cgroup-capped PostgreSQL when available;
record the actual memory limit, swap, cgroup peak/OOM data, and page cache. Do
not install or start system services or provision a VM without approval. If no
usable runtime exists, run native PostgreSQL with tuning budgets, label total
memory as `UNENFORCED`, and report the hard-cap axis as blocked. Reuse prior
task-local dependencies when safe. Do not delete or modify another task's data
or active database.

Independently vary base row count, batch size, and actual join fanout. Record
affected rows and output size. Include a batch-10 row-count sweep from 400
through 12,000 plus larger cases, at least two fanouts, and at least two
cache/work_mem profiles. Prefer an expanded native PostgreSQL grid, retain the
existing PGlite baseline, and make any PGlite smoke optional. Reuse fixtures and
oracles while preserving existing receipts and exact checksums. Bound client
readback.

Use a staged grid: smoke, then one warmup and three measured repeats. Limit each
process to 120 seconds and total benchmark execution to 20 minutes. Do not run
heavy benchmarks in parallel. Measure each mutation family and compute paired
total update-plus-query time per run. Record setup, transfer, checksum, and
startup separately. Record disk bytes and memory scope. Use separate diagnostic
`EXPLAIN ANALYZE BUFFERS` runs for temporary and spill I/O. Retain failures,
OOMs, and timeouts as explicit receipt rows.

Generate standalone SVG heatmaps from receipts. Put row count on the horizontal
axis and batch size on the vertical axis. Show the full-query/IVM speedup as
both a number and a color, with separate fanout and memory-budget panels.
Distinguish unmeasured cells and use readable inline labels. Preserve the chart
generator. Report variation and measured cutoff brackets without interpolation.
Keep unsupported hard memory caps explicit.

Produce `TASKS/2_postgres-ivm-crossover.REPORT.md` with commands, commits,
current executed test coverage, boundaries, and limitations. Commit code and
receipts in separate chunks. Hail after infrastructure inventory and again at
completion. Stop blocked steps without inventing authority. Scope is labs only.
