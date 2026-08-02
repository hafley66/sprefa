# DUEL: CST extraction demand — eager corpus vs lazy query-demanded

Base: `92756b54`. Rival fork: same prompt.

## 1. Ruling

**LAZY for CST; eager remains for the compact source planes (type/call/df).**

Exact boundary:
- **Eager at ingest**: `file`, `node`/`edge`/`sig`/`site`/`const` for `type|call|df` families, via `ingestFile` → `extractFile` → `toFactLines` (`v6/dl/src/4_ingest.ts:330`). These are small, shared, and required for most rules.
- **Demanded for CST**: `cst` family rows are produced only when a query cone contains a structural goal that reads CST facts. Demand is emitted as `cst_need(path, content_hash, lang)` (or a host probe equivalent), satisfied by the existing `extract` HostDef (`v6/dl/src/1_hosts.ts:358`), batched per tick, and cached by content-hash witness in `effect_cache` (`v6/dl/src/1_hosts.ts:170`).
- **No full lossless CST plane at ingest.**

Why this boundary and not pure eager: CST is per-file, has no cross-file resolution (`v6/sprefa-extract/src/types.rs:1055`), and is 1–2 orders of magnitude larger than the other planes. Materializing it for every file before any query exists contradicts the v6 demand ruling that "nothing computes without a reason" (`v6/plans/2026-07-19-v6-demand.md:16`). The existing `extract` HostDef and `effect_cache` digest machinery already solve the hard parts (dedupe, staleness, batching); the only missing piece is wiring CST demand to that host instead of pre-computing it in `ingestFile`.

## 2. Archive evidence table

| claim | source path | what it says |
|-------|-------------|--------------|
| Parse dominates source-scan wall time on a real corpus. | `~/projects/sprefa-archive-20260701/v4/perf/baseline.toml:207-215` | Linux cold: `ast_parse_ms=18600.9`, `ast_read_ms=9305.7`, `ast_match_ms=2089.1`; parse is ~2× read and ~9× match. |
| Content-hash memo makes incremental correct *and* fast. | `~/projects/sprefa-archive-20260701/v4/perf/baseline.toml:194-203,218-229` | Cold 68.31s → incremental 15.70s; parse collapses from 63482 files to 1; hits stay correct (16628). Staleness oracle is `blake3(input file bytes)`. |
| The Linux bench scanned ~63k .c/.h files and produced only 16,627 matches; most files do not need deep structure for typical queries. | `~/projects/sprefa-archive-20260701/v4/bench/linux-called.sprf:14-16` and `~/projects/sprefa-archive-20260701/v4/perf/baseline.toml:48` | Fixture: ~37k .c, 16627 printk sites; `hits_rows=16627`, `fs_emitted=63482`, `ast_parses=4495`. |
| v3 tried three parallel-extraction batchers; bounded work-steal became the default for scan/parse/match. | `~/projects/sprefa-archive-20260701/v3/crates/effect_runtime/src/batchers/bounded_work_steal.rs:1-31` | "Use this for CPU-bound effects emitted under burst conditions — many ops, streaming pipeline, no natural pacing. This is the default for v3 scan/parse/match." |
| The v4 bench shapes were simple scan+match, join, antijoin, and tiny variants — none materialize a full CST. | `~/projects/sprefa-archive-20260701/v4/bench/linux.sprf:16-19`, `linux-join.sprf:17-23`, `linux-antijoin.sprf:7-26` | All use `fs(...) > ast(:c)` pattern matching, not a lossless CST plane. |
| Linux bench operationally broke on multi-flush path collision and inter-batch tail sync — i.e., scale-dependent batching/coordination bugs, not parse cost. | `chat_log/20260504.11.linux-bench-reconcile-path-collision-fix.md:13-24,36-44` and `chat_log/20260505.0.linux-bench-tail-sync-cap-fix.md:14-25` | Pre-fix reconcile: 1081 matches (-94%) and 8.1s; post-fix 4.5s; batch_cap≥65536 kills tail-sync, yielding ~3.98s. |
| Current A/B bench isolates bundled (one parse, all families) vs baseline (three separate extractions). | `bench/extract-ab.sh:1-9`, `examples/extract_ab.rs:102-126` | Baseline does 3 parses/file; bundle does 1. Bundle is the production target. |
| v6 already extracts all four families eagerly in ingest; doing so is ~87× slower than v5 scan and blows the DB. | `v6/prolog/ARCH.pl:710` | v5 org-fan: 42,739 files / 12.07s = 3,540.9 files/s; v6 served extraction: 779 files / 19.15s = 40.7 files/s, running `cst+type+call+df`. |
| Capturing the extractor's whole `cst/type/call/df` JSONL as EDB arrivals for 779 files increased wall 20.26s → 62.97s and scratch DB 1.0MB → 595MB. | `v6/prolog/ARCH.pl:619-620` | Measured fork: eager full-JSONL ingest is 3.1× slower and ~600× larger. |
| CstF is lossless named-node tree; one node per named node + one Child edge per named child. | `v6/sprefa-extract/src/types.rs:165-185`, `v6/sprefa-extract/src/lang/astgrep.rs:162-191` | Node count scales with AST size, not query selectivity. |
| v6 demand model: rels are cold; computation requires a subscription; a watcher subscribes to *source* rels only. | `v6/plans/2026-07-19-v6-demand.md:21-35` | "Rels are cold observables. ... A subscription is a refcounted handle over `(root, query rel)`." |
| The extraction-fork verdict chose host-relation shape for extraction because of content-salt sharing and spine residency. | `plans/2026-07-29-hosts-extraction-verdict.md:356-360` | "sg, ast, tree-sitter, and span extraction take the host relation shape." |
| HostRunner currently spawns one subprocess per demand row (concatMap), and the extraction-host-batching lab plans to group `sprefa_extract` demands. | `v6/dl/src/1_hosts.ts:430-432` and `plans/2026-07-30-extraction-host-batching-lab.pl:35-39` | Current: "host N; extract N". Target: batch by executor/template/ordered inputs. |

## 3. Steelman against the ruling

**Workload: a structural linter run over the entire corpus on every commit, where >80% of files are queried structurally.**

In this shape lazy still parses each file once, but pays the demand-to-host latency (`extract` process spawn, `effect_cache` lookup, stdout JSONL decode) per file instead of inside a batched ingest pipeline. If the host batching lab has not landed, lazy can serialize on `concatMap` and be slower than eager ingest. If the linter runs repeatedly over the same snapshot, effect_cache amortizes the parse but not the first-run delay. This is the only class where eager wins, and it wins only when:
- the fraction of files touched by CST queries is high,
- query latency is measured from ingest time (not first query), and
- host demand batching is absent.

Even then, eager pays the storage and ingest-time cost on *every* file, including the ~20% (or more) never queried structurally. Lazy pays only for queried files.

## 4. The definitive lab

**Method**: fork the existing `bench/printk.dl` lineage into two arms and run against `bench/linux-sim` plus a real kernel checkout (e.g., `linux` stable, ~60k .c/.h files). Both arms use the same v6 engine build and the same `extract` binary.

- **Eager arm**: modify `ingestFile`/`spineDeclsLocal` to always extract and store `cst` family rows alongside `type/call/df`. Run `bench/run.sh bench/printk-cst-eager.dl <root>`.
- **Lazy arm**: add a rule that derives `cst_need(path, content_hash, :c)` from a structural goal; the compiler lowers it to a probe against the `extract` HostDef with family mask `cst`. Run `bench/run.sh bench/printk-cst-lazy.dl <root>`.
- **Query**: a simple structural rule such as "every `function_definition` whose `identifier` child text contains `printk`", producing the same result set on both arms.

**Metrics** (reported by `bench/run.sh` timing + instrumented `effect_cache`/`ingestFile` traces):

| metric | how measured |
|--------|--------------|
| fact count | `SELECT COUNT(*) FROM node WHERE family='cst'` after ingest; plus `edge`/`span_line`. |
| on-disk bytes | `PRAGMA page_count * page_size` of the bench db; compare to source bytes. |
| ingest wall | `bench/run.sh` cold run wall seconds. |
| cold-query latency | wall seconds of the first query after a fresh db (lazy includes first-demand extraction). |
| effect spawn count | `effect_cache` rows created with `host='extract'` during the run. |

**Threshold that flips the ruling**:

Lazy remains correct if **all** of the following hold on the real kernel checkout:
1. Cold-query wall (lazy) ≤ 1.2 × eager cold-query wall. Same parse work dominates; the 1.2× allows for demand-host overhead.
2. CST fact count in eager mode ≥ 10× non-CST source facts (`node`+`edge`+`site`+`sig`+`const` for `type|call|df`).
3. Eager on-disk db bytes ≥ 5× lazy db bytes.
4. With host demand batching landed, lazy effect spawns for a full-corpus structural query ≤ number of files touched / batch size (target batch size ≥ 64, matching the v4 tail-sync fix reasoning at `chat_log/20260505.0.linux-bench-tail-sync-cap-fix.md:42-45`).

If eager cold-query wall is >20% faster than lazy **and** ≥50% of ingested files are queried structurally in the same session, eager flips. Otherwise lazy wins.

## 5. Risks

| # | risk | earliest observable symptom |
|---|------|------------------------------|
| 1 | **Demand amplification / spawn storm.** A structural query over many files fans out one `extract` demand per file; without host batching, HostRunner serializes on `concatMap`. | First structural query over >100 files takes >5s and `effect_cache` shows one pending `extract` row per file. |
| 2 | **Stale CST from cache supersession bug.** If content hash is omitted or incorrectly mixed into the witness digest, an edited file replays old CST rows. | Structural query returns spans from the previous file version; unit test appending one `printk("x")` and re-querying shows old line numbers. |
| 3 | **Runaway storage from historical CST versions.** `effect_cache` and response rows for every demanded `(path, content_hash, lang)` accumulate; retained past the current snapshot. | Db size grows with each commit even though source working tree size is flat; `effect_cache` row count >> current file count. |
