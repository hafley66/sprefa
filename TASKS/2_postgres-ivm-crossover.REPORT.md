# PostgreSQL IVM crossover and memory lab

Run date: 2026-09-07. Branch: `feature/postgres-ivm-crossover`. Base:
`2c4614e8d`.

## Executed result

The staged native PostgreSQL grid completed 17 cells. Each cell ran one
discarded warmup followed by three measured repetitions for both the ordinary
full-query view and the pg_ivm immediate materialized view. All 680 mutation
records, including warmups, are `ok`. All 51 measured query/IVM pairs have
matching final SHA-256 checksums. No process recorded an error, mismatch,
timeout, OOM, or missing arm.

The new grid uses native PostgreSQL 18.6 and pg_ivm 1.15. The existing PGlite
0.5.8 and PGlite pg_ivm 0.0.9 baseline was retained without rerunning it. Its
three published receipts remain byte-identical.

The hard-memory-cap axis is blocked. Total PostgreSQL memory is explicitly
`UNENFORCED` in every new receipt.

## Infrastructure inventory

The inventory was read-only and completed before benchmark implementation.

| facility | observed state |
|---|---|
| Docker | client 29.7.2 at `~/.docker/bin/docker`; selected `desktop-linux` socket absent; server connection failed |
| Docker processes | no Docker daemon; `com.docker.helper` launch agent registered without a running daemon |
| Podman | executable absent |
| Colima | executable absent |
| Lima | executable absent |
| OrbStack, Finch, Multipass | executables absent |
| host PostgreSQL | `postgres`, `pg_ctl`, `psql`, and `pg_config` absent from `PATH` |
| host | Mac14,10, Apple M2 Pro, arm64, macOS 14.6.1 build 23G93, 16 GiB RAM |
| inventory swap | 8,192 MiB total, 7,173.81 MiB used |

No existing Linux runtime was usable for an isolated cgroup. Docker Desktop was
not started, and no system service or VM was installed, started, or
provisioned. The run reused the prior task's PostgreSQL and Node packages
through symlinks created only inside this worktree. The prior task's data and
active databases were not changed.

The completion hail for the inventory was delivered as `m-0b518230`.

## Workload and axes

The deterministic fixture uses `dimension(group_id, factor)` and
`fact(id, group_id, amount)`. It evaluates this grouped join:

```sql
SELECT dimension.group_id,
       count(*) AS row_count,
       sum(fact.amount::bigint * dimension.factor::bigint) AS weighted_sum
  FROM fact
  JOIN dimension USING (group_id)
 GROUP BY dimension.group_id
```

The generator places exactly `fanout` initial fact rows in `group_id = 0`.
The dimension mutation changes that group's factor. Every setup receipt checks
the actual database count against the requested fanout. Batch insert, delete,
and update IDs are disjoint from that group, so row count, batch size, and
dimension-join fanout vary independently.

Each state is checked against an independent JavaScript `Map` oracle using seed
`0x706f737467726573`. The client reads at most 256 aggregate rows. Executed
outputs contained 20 through 128 rows and 241 through 2,005 canonical bytes.

The full grid is staged as follows:

| budget | fanout | fixed batch-10 row sweep | additional batch sweep at 12,000 rows |
|---|---:|---|---|
| constrained | 10 | 400, 12,000 | none |
| constrained | 200 | 400, 1,200, 4,000, 12,000, 40,000, 160,000 | 1, 100, 1,000 |
| roomy | 10 | 400, 12,000 | none |
| roomy | 200 | 400, 12,000, 160,000 | 1,000 |

The profiles use these PostgreSQL settings:

| budget | shared_buffers | work_mem | effective_cache_size | maintenance_work_mem | temp_file_limit |
|---|---:|---:|---:|---:|---:|
| constrained | 32 MiB | 1 MiB | 64 MiB | 32 MiB | 2 GiB |
| roomy | 256 MiB | 32 MiB | 2 GiB | 256 MiB | 2 GiB |

Both use `fsync=on`, `synchronous_commit=on`, `full_page_writes=on`,
`track_io_timing=on`, and a private Unix-domain socket. Each profile uses a new
disposable cluster and two dedicated databases.

## Timing boundaries

For each state, `update_transaction_ms` covers `BEGIN` through `COMMIT`.
`query_compute_ms` covers materializing the current aggregate into a temporary
snapshot. Full recomputation occurs in this phase for the ordinary view.
`update_plus_query_ms` is their sum. It excludes client transfer and checksum.
The paired-run speedup divides the ordinary-view sum by the pg_ivm sum for the
same case and repetition.

Setup records schema creation, initial load, shared index construction, and
view or IMMV construction separately. Client transfer, checksum, server
startup, Node start-to-connect, case wall, and process wall are separate
fields. Native server startup was 136 ms for the constrained profile and
141 ms for the roomy profile.

## Measured crossover grid

Values are paired full-query/pg_ivm speedup. The center is the median of three
paired repetitions and the range is the measured minimum to maximum.

| budget | fanout | rows | batch | median | measured range |
|---|---:|---:|---:|---:|---:|
| constrained | 10 | 400 | 10 | 0.703x | 0.697x to 0.738x |
| constrained | 10 | 12,000 | 10 | 1.689x | 1.613x to 1.804x |
| constrained | 200 | 400 | 10 | 0.825x | 0.798x to 3.508x |
| constrained | 200 | 1,200 | 10 | 0.844x | 0.759x to 0.899x |
| constrained | 200 | 4,000 | 10 | 1.177x | 1.099x to 1.311x |
| constrained | 200 | 12,000 | 1 | 1.604x | 1.589x to 1.853x |
| constrained | 200 | 12,000 | 10 | 1.555x | 0.881x to 2.333x |
| constrained | 200 | 12,000 | 100 | 1.060x | 0.941x to 1.222x |
| constrained | 200 | 12,000 | 1,000 | 0.966x | 0.510x to 1.064x |
| constrained | 200 | 40,000 | 10 | 3.679x | 2.017x to 3.915x |
| constrained | 200 | 160,000 | 10 | 11.452x | 11.337x to 11.632x |
| roomy | 10 | 400 | 10 | 0.753x | 0.699x to 0.819x |
| roomy | 10 | 12,000 | 10 | 1.830x | 1.591x to 1.874x |
| roomy | 200 | 400 | 10 | 0.919x | 0.448x to 2.185x |
| roomy | 200 | 12,000 | 10 | 1.658x | 1.500x to 1.677x |
| roomy | 200 | 12,000 | 1,000 | 1.034x | 0.268x to 1.104x |
| roomy | 200 | 160,000 | 10 | 12.100x | 11.496x to 12.193x |

The receipt-generated chart is
[crossover-heatmaps.svg](../v6/labs/exec_shootout/postgres_pglite_ivm/results/crossover-heatmaps.svg).
It marks every unscheduled cell separately from a planned cell without a
successful pair. This run has zero planned cells without a pair.

Measured cutoff brackets, with no interpolation:

| slice | lower measured point | upper measured point |
|---|---|---|
| constrained, fanout 10, batch 10 | 400 rows: 0.703x | 12,000 rows: 1.689x |
| constrained, fanout 200, batch 10 | 1,200 rows: 0.844x | 4,000 rows: 1.177x |
| roomy, fanout 10, batch 10 | 400 rows: 0.753x | 12,000 rows: 1.830x |
| roomy, fanout 200, batch 10 | 400 rows: 0.919x | 12,000 rows: 1.658x |
| constrained, 12,000 rows, fanout 200 | batch 100: 1.060x | batch 1,000: 0.966x |

The roomy 12,000-row fanout-200 batch sweep measured batches 10 and 1,000 at
1.658x and 1.034x. It does not contain a measured crossing.

Variation is retained rather than smoothed. The largest relative speedup span
is 328.384% at constrained/400 rows/batch 10/fanout 200. The next two broad
ranges are 189.042% at the corresponding roomy cell and 93.366% at
constrained/12,000 rows/batch 10/fanout 200. The full variation column is in
[crossover-summary.tsv](../v6/labs/exec_shootout/postgres_pglite_ivm/results/crossover-summary.tsv).

## Mutation-family timing

The following slice shows median `update + query` milliseconds at 160,000 rows,
batch 10, and exact fanout 200. The complete 170-family table includes update,
query, transfer, checksum, affected rows, output size, and min/median/max values
for every cell.

| budget | family | full query ms | pg_ivm ms |
|---|---|---:|---:|
| constrained | insert batch | 21.926 | 2.135 |
| constrained | delete batch | 21.872 | 1.812 |
| constrained | update batch | 21.930 | 1.975 |
| constrained | dimension fanout | 21.822 | 1.816 |
| roomy | insert batch | 22.285 | 1.930 |
| roomy | delete batch | 21.866 | 1.786 |
| roomy | update batch | 22.600 | 2.173 |
| roomy | dimension fanout | 22.428 | 1.715 |

At 12,000 rows, batch 1,000, and fanout 200, the insert and update families
account for the larger pg_ivm update cost:

| budget | family | full query ms | pg_ivm ms |
|---|---|---:|---:|
| constrained | insert batch | 6.739 | 6.715 |
| constrained | delete batch | 2.815 | 2.713 |
| constrained | update batch | 7.133 | 8.186 |
| constrained | dimension fanout | 3.028 | 1.534 |
| roomy | insert batch | 6.275 | 6.789 |
| roomy | delete batch | 2.763 | 2.511 |
| roomy | update batch | 7.217 | 8.253 |
| roomy | dimension fanout | 2.497 | 1.574 |

The complete data is
[crossover-family-summary.tsv](../v6/labs/exec_shootout/postgres_pglite_ivm/results/crossover-family-summary.tsv).

## Memory and disk scope

| field | executed observation |
|---|---|
| total memory limit | `null`; `UNENFORCED` |
| swap limit | no cgroup value; host swap was 8,192 MiB total and 6,916.12 MiB used at both full-run boundaries |
| OOM events | no cgroup counter; no process failure or OOM status occurred |
| page cache | no cgroup counter or attributable macOS process-group page-cache value |
| PostgreSQL group memory | sampled sum of RSS for the disposable postmaster process tree; 48,784 to 103,472 KiB across measured processes |
| database bytes | 7,870,143 to 21,280,447 bytes at measured case completion |
| WAL directory bytes | 16,777,216 to 352,321,536 bytes; cluster-wide at each sample |
| client memory | Node RSS, heap use, and process peak recorded per state |
| backend memory | backend RSS and `pg_backend_memory_contexts` bytes recorded per state |

RSS includes resident mappings and does not separate PostgreSQL private memory,
shared buffers, and host page cache. The hard-cap, cgroup swap, cgroup OOM, and
cgroup page-cache values remain `null` rather than substituting host-wide data.

## Separate EXPLAIN diagnostics

The diagnostic profile used 160,000 rows, batch 10, and fanout 200. It ran after
the timed grid and its timings are not included in crossover speedups.

| budget | arm and statement | execution ms | shared hit blocks | temp read blocks | temp written blocks |
|---|---|---:|---:|---:|---:|
| constrained | full query | 30.856 | 866 | 0 | 0 |
| constrained | ordinary dimension update | 0.119 | 5 | 0 | 0 |
| constrained | pg_ivm query | 0.033 | 2 | 0 | 0 |
| constrained | pg_ivm dimension update | 0.844 | 7 | 0 | 0 |
| roomy | full query | 32.580 | 866 | 0 | 0 |
| roomy | ordinary dimension update | 0.102 | 5 | 0 | 0 |
| roomy | pg_ivm query | 0.030 | 2 | 0 | 0 |
| roomy | pg_ivm dimension update | 0.779 | 7 | 0 | 0 |

All diagnostic plans report zero shared reads, temp reads, temp writes, and disk
spill. `pg_stat_database` also reports zero temp-file and temp-byte deltas. The
pg_ivm after-update trigger took 0.736 ms under the constrained profile and
0.685 ms under the roomy profile. Full JSON plans and buffer fields are in
[crossover-diagnostics.jsonl](../v6/labs/exec_shootout/postgres_pglite_ivm/results/crossover-diagnostics.jsonl).

## Commands executed

Infrastructure inventory:

```bash
command -v docker podman colima limactl postgres pg_ctl psql
docker context ls
DOCKER_CLIENT_TIMEOUT=5 docker info --format '{{json .}}'
podman machine list --format json
podman info --format json
colima status --json
colima list --json
limactl list --json
ps -axo pid=,comm=,args= | rg -i '(Docker|podman|colima|lima|postgres)'
sysctl -n hw.memsize
sysctl vm.swapusage
memory_pressure
```

Task-local dependency reuse and checks:

```bash
ln -s /Users/chrishafley/projects/sprefa/.boop-worktrees/feature/postgres-pglite-ivm/v6/labs/exec_shootout/postgres_pglite_ivm/.work/postgres-18.6 .work/postgres-18.6
ln -s /Users/chrishafley/projects/sprefa/.boop-worktrees/feature/postgres-pglite-ivm/v6/labs/exec_shootout/postgres_pglite_ivm/node_modules node_modules
node --check 9_crossover_workload.mjs
node --check 10_crossover_case.mjs
node --check 11_crossover_native.mjs
node --check 12_crossover_runner.mjs
node --check 14_crossover_summarize.mjs
node --check 15_crossover_heatmap.mjs
bash -n 13_crossover_run.sh
```

Benchmark and artifact generation, from
`v6/labs/exec_shootout/postgres_pglite_ivm`:

```bash
./13_crossover_run.sh smoke results/crossover-smoke.jsonl
./13_crossover_run.sh full results/crossover-full.jsonl
./13_crossover_run.sh diagnostic results/crossover-diagnostics.jsonl
node 14_crossover_summarize.mjs \
  results/crossover-full.jsonl \
  results/crossover-summary.tsv \
  results/crossover-family-summary.tsv
node 15_crossover_heatmap.mjs \
  results/crossover-full.jsonl \
  results/crossover-heatmaps.svg
xmllint --noout results/crossover-heatmaps.svg
```

The full benchmark profile took 18.572 seconds for constrained and 11.206
seconds for roomy. Smoke took 0.472 seconds. Diagnostics took 1.493 and 1.263
seconds. Total benchmark and diagnostic execution remained below 35 seconds,
inside the 20-minute budget. No heavy benchmark process ran in parallel.

## Receipts and checksums

```text
6b0ccbd448f6ddd84d9e07687e9e8b02e9d9aa29b865ae3297dc87dbb1369b67  results/crossover-smoke.jsonl
04962d3328928887a82f277a36a7d39de8dfe77c2852db66c9fc4853024992fa  results/crossover-full.jsonl
3ff5390027df96fd8ec693ce8d25c4b50ee6e8ec5be9622e9243f4a30620b873  results/crossover-diagnostics.jsonl
4135ef5a572fbe07b560c88602f52c2f378d492b2ca05dc846507134fa5cb733  results/crossover-summary.tsv
b57c539240d59625c07cbc979edca55f02d82ed824f07eecc571fa2d3bf6ebb1  results/crossover-family-summary.tsv
499a63ffbe7aa13919e4ae87ce4f58c01ddf34959d9a0995c2f667c16a7ec798  results/crossover-heatmaps.svg
```

Retained baseline checksum verification:

```text
5b5fde64ec37c992629fb8323ecd584be9e120efdb54b4ffe32eed75f2b15c15  results/smoke.jsonl
27ad9db78ee9fad82971967dadbf830a6878235298cd48e5f0d7d06f5f2a4486  results/scale.jsonl
d435ab1e38f99a269cdfd2d7879eced3e26686cc72f30edb16767a2f058ba32a  results/scale-summary.tsv
```

## Executed test coverage and boundaries

| check | current result |
|---|---|
| JavaScript syntax, six new `.mjs` files | passed |
| shell syntax, crossover runner | passed |
| smoke | 4 processes, 20 mutation checks, 2 paired checksum checks, all `ok` |
| full grid | 136 processes, 680 mutation checks, 51 measured paired checksum checks, all `ok` |
| diagnostics | 4 processes, 8 EXPLAIN records, 4 temp-I/O records, all `ok` |
| SVG XML parse | passed |
| rendered SVG inspection | passed at 1,258 by 798 pixels |
| retained baseline SHA-256 | all three exact |

No dependency installation or build ran. Existing CI workflows were not
changed. This work adds no CI coverage and removes no CI coverage.

The results apply to a warm, single-client, disposable native PostgreSQL setup
on this host. The warmup is discarded, but later repetitions share the same
profile cluster and host page cache. Client transfer and checksum are measured
and excluded from paired speedup. Setup and process startup are also excluded.
The staged grid leaves cells unscheduled by construction. Cutoffs are reported
only as adjacent measured brackets.

## Commits

| commit | content |
|---|---|
| `d38b87176` | recorded task brief before implementation |
| `05d5c7412` | deterministic fanout fixture, bounded snapshots, native instrumentation |
| `e90cfb641` | staged runner, tuning profiles, deadlines, failure receipts |
| `ec1e82b20` | summaries, diagnostics, memory/disk fields, SVG generator |
| `d29eba8da` | smoke, full, diagnostic, TSV, and SVG receipts |
| `accbc4ce6` | explicit unsuccessful exit and possible OOM/SIGKILL receipts |

No compiler, kernel, production service, global setting, primary checkout,
merge, or push was changed.
