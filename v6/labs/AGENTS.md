# v6/labs — AGENTS.md · the golden-data contract for every benchmark

Standing instructions for any agent (or human) running a v6 lab benchmark. This
file defines **what data every run MUST capture**, how it is stored (raw, then
aggregated), and the standing measurement discipline. Read it before touching a
harness.

---

## SESSION HANDOFF — 2026-07-22 (READ FIRST after a context reset)

**Mission:** kill v5's resident 36 GB-swap model by putting the graph-algo
covering set on v6's **on-disk counting cascade** (RSS bounded, Rust heap ~0),
matching the resident engines (dd/salsa) on correctness while driving the
counting Big-O down. **Target machine ≈ 1 GB RAM, not 12.**

**Landed this session on branch `v11` (newest first):**
- `4c483691` `tools/chat-find.sh` (rg+fzf reverse chat index + pin) + `v6/MAP-SOURCES.md` + `v6/findings/soft-delete-durable-scan.md`
- `4561e031` AGENTS context-pointer + `v6/findings/HYPOTHESES.md` idea ledger
- `2310e7bf` this file — the golden-data contract
- `015c4784` millions perf sweep to **11.5M nodes** + `perf-charts.html`
- `2f94cc43` `v6/MAP.md` living map + `v6/skills/mermaid-living-map.md`
- `5d3339dd` `perf.json` emit + `v6/sprefa-store/tools/gen-perf-charts.sh`
- `004c1b68` reorg: experiment crates → `v6/labs/{labkit,frp-lab,reactor-lab,temporal-lab}`
- `1fa19d35` **G6**: `retract_scc` BEATS DRed (2123<2267 ms CYC960k); DAG early-out still OPEN
- `17cafc07` **G7**: 7 engines wired into `0_unified` (14/14 correct)
- `0fc58912` **G5**: `retract_scc` cycle-correct counting (SCC nested fixpoint)
- `fe386a15` **G4v2**: labkit rusqlite-free (SQLite in ONE crate) + hermetic runner

**In flight — DO THIS NEXT:**
- **G10** (`exp/g10-golden-data`, codex terra, was RUNNING): the golden-data
  bench per this contract — impl-level `tracing` in `cascade.rs` + per-process
  sensors + `cache_size` sweep under 1 GB → raw `v6/labs/perf-runs.sqlite`(+csv).
  It already produced `perf-runs.sqlite`. **Verify** the tracing is impl-level
  (phase/round/DML, NOT trait-boundary) and overhead is negligible, then merge.
- **G8** (`exp/g8-mmap-kv`, codex terra, DONE but **UNMERGED + SUSPECT**): the
  `redb-count` mmap engine. TWO red flags caught, do not trust until explained:
  (1) its `sqlite-count` baseline is ~1000x too slow (12,654 ms @100k vs the
  store's ~30 ms) — a broken comparison; (2) `redb-count` resident RAM CLIMBS
  ~97 B/node (194 MB @2M) — it FAILS the memory-first objective (was meant to be
  flat/evictable). Re-measure against the real store sqlite-count before merging.

**Open threads (prioritized):**
1. **Rebuild the living map in D2, not Mermaid** ("mermaid sucks"). Feed the anim
   atlas pipeline (`~/projects/anim`: D2 → `@terrastruct/d2` WASM → Model →
   cytoscape, with `explorer.jsx` progressive drill-down + `AtlasPanel`
   fold/unfold, and a CSS-anchor render backend). The current `v6/MAP.md` is
   "turbo mid" and needs FAR more detail; drill-down is how the detail lands. Next
   step was: write a natural-language map-content spec → haiku authors `v6/MAP.d2`.
2. Verify+merge G10; re-measure G8 (redb) honestly.
3. Consolidate narrative docs → `v6/labs/docs` (only 4 files ref DECISIONS/
   ARCHITECTURE — cheap). Deferred so it didn't break in-flight path refs.
4. Work the hypotheses in `v6/findings/HYPOTHESES.md` (H1 soft-delete/tombstone,
   H4 SCC DAG early-out, H5 cache_size curve).

**Delegation gotchas (hard-won, do not relearn):**
- **codex exec MUST launch with `< /dev/null`** or it WEDGES on stdin at 0% CPU
  forever. Three jobs stuck 15–45 min this session before this was found.
- Models: codex `gpt-5.6-terra` (heavy Rust), `-sol` (perf, low), `-luna` (grunt,
  low); haiku subagents for grunt fs/syntax with known tools.
- **NEVER trust subagent/codex output.** Verify every citation, number, and
  baseline yourself — a fabricated line number or a broken baseline (see G8) is
  worse than nothing. Ask the user when genuinely ambiguous.
- codex can't commit (pre-commit hook needs `dl`, not on PATH); coordinator
  commits `--no-verify`. `dl` is not on PATH in this env either.

---

## The reality this is measured against

**The target machine has ~1 GB of RAM, not 12.** Every claim must hold under a
**1 GB address-space cap** (`DL_MEMCAP_MB=1024`), and we care about tighter caps
too. A number measured at a 12 GB cap is a lie about the deployment. dd and any
resident engine are expected to ABORT early here — that is a result, recorded,
not an error.

## The question every run exists to answer

**What is the relative effect of the RAM-ceiling knob (SQLite `PRAGMA
cache_size`) on speed and on total disk read/write, per engine, per problem
size?** Squeezing `cache_size` down lowers RSS but forces page eviction →
more disk I/O → slower. We want that trade curve under the microscope so we can:
1. find the smallest `cache_size` that still finishes at each scale under 1 GB,
2. read the speed / read-write cost of that squeeze,
3. then run experiments to **drive that cost down** (fewer statements, better
   access paths, on-disk layout) — measured, not asserted.

## When the lab stops being a lab (the graduation criterion)

The benchgraph's `(tag, id)` toy keys are a stand-in. The lab is DONE — stops
being a lab — when the on-disk counting cascade + **dense integer foreign-key
interning** (every entity/name interned to a packed `i64`, FKs as those ints, as
God intended for efficiency) works for **any relational schema**, not just this
lab's two-column graph. That means: an arbitrary set of relations with arbitrary
FK edges gets the same weight-counted retraction, SCC fixpoint, and RSS-bounded
on-disk behaviour we prove here on the toy. Until the interning + cascade is
schema-general, it is still a lab. Measure toward that: the sensors and the
storage layout must not assume the 2-column shape.

## Golden data — capture EVERYTHING, per process, per phase

One OS process per (engine × workload × scale × cache_size) cell. Inside it,
mark three phases — **build** (generate the input graph in Rust), **insert**
(load it into the store), **retract** (the timed algorithm) — and capture, for
each run, at minimum:

### Cost of the INPUT (the thing the user asked for explicitly)
- `input_rust_bytes` — the graph's resident Rust size BEFORE the bench:
  `rows: Vec<(u32,i64,i64)>` (20 B/row + Vec overhead) + `edges: Vec<(u32,i64,
  u32,i64)>` (32 B/edge) + any adjacency `HashMap`. Compute it
  (`capacity * size_of` per Vec) AND read RSS at end of build — report both.
- `nodes`, `edges` (counts).

### Memory (over time, not just peak)
- `rust_peak_kb` — memcap high-water (the gun-visible Rust heap). NOTE: near-zero
  for SQLite engines is EXPECTED and is NOT the footprint — do not headline it.
- `rss_build_kb`, `rss_insert_kb`, `rss_retract_kb` — RSS at each phase boundary
  (`getrusage`/`rusage_info`). RSS is the honest footprint.
- `sqlite_hw_kb` — SQLite C-heap high-water (`sqlite3_memory_highwater`).
- `cache_size_kb` — the configured knob for this run (the independent variable).
- A **RAM time-series** during retract: sample RSS + rust-heap every ~50 ms from
  a sidecar thread → `ram_timeseries(run_id, t_ms, rss_kb, rust_kb)`.

### Disk read/write (the other half of "RAM and total read/write")
- `disk_read_bytes`, `disk_write_bytes` — process disk I/O. On macOS use
  `rusage_info(RUSAGE_INFO_V*)` `ri_diskio_bytesread/written` (NOT `ru_inblock`,
  which is usually 0 on Darwin). Record which source was used.
- `cache_hit`, `cache_miss`, `cache_write` — SQLite's own view via
  `sqlite3_db_status(DBSTATUS_CACHE_HIT/MISS/WRITE)`. This is the direct readout
  of the cache_size knob's effect: squeeze the knob, watch miss/write climb.
- `wal_checkpoint_pages`, `db_bytes` (on-disk file size after retract).

### Compute / throughput
- `t_build_ms`, `t_insert_ms`, `t_retract_ms` (per-phase wall time).
- `stmts` — SQL statements in the retract.
- `rows_retracted`, `survivors`, `throughput_rows_per_s` (= rows_retracted /
  t_retract_s).

### Final state — how big are the tables when it's over (read/write is only half)
- `db_bytes` total after retract, AND the per-table breakdown via `dbstat`:
  `final_table_bytes`, `final_index_bytes`, `final_free_bytes` per table
  (`SELECT name, SUM(pgsize) FROM dbstat GROUP BY name`). Report table-vs-index
  split — index bloat is a standing v5 defect (indexes were 57% of the file).
- rows-per-table live count at the end. For a soft-delete variant (H1), also the
  tombstone count (`weight=0` rows still present) vs live — that ratio is the
  whole cost of not hard-deleting, and it must be measured, not assumed.

### Correctness + query plans
- `correct` (hash == oracle), `out_hash`.
- `EXPLAIN QUERY PLAN` of **every DML in the retract** →
  `query_plans(run_id, dml_index, sql_prefix, plan_text)`.

### Outcome
- `aborted` (bool), `abort_phase` (build/insert/retract) when the gun fires.

## Storage — RAW first, aggregate second (do NOT hand-write tables)

1. **Raw**: append every run as a row into a SQLite db `v6/labs/perf-runs.sqlite`
   (tables: `runs`, `ram_timeseries`, `query_plans`), AND export `runs` to
   `v6/labs/perf-runs.csv`. This is the source of truth — "literally everything
   we can get our hands on," one row per process. Never overwrite; each sweep
   appends with a `sweep_ts`.
2. **Aggregate**: a generator reads `perf-runs.sqlite` and emits the markdown
   tables + charts (extend the existing `perf.json` → `gen-perf-charts.sh`
   pipeline; charts must plot RSS and disk-write vs cache_size, per engine). The
   markdown/charts are DERIVED and disposable; the sqlite/csv is the archive.

## The sweep

`engine ∈ {sqlite-count, sqlite-count-scc, dd, mmap-kv(when it lands)}`
`× workload ∈ {DAG, CYC}`
`× nodes ∈ {ladder into the millions}`
`× cache_size ∈ {a ladder, e.g. 2000, 8000, 32000, 128000 KiB}`
all under `DL_MEMCAP_MB=1024` (and repeat at 512 to find the wall).

## Where the standing context lives (READ before you re-derive anything)

Half our pain is re-deriving decisions and **losing good hypotheses** we already
had. Before proposing or re-arguing anything, check these. When you have a new
banger idea, it goes in the HYPOTHESES ledger — do not let it evaporate into a
transcript nobody re-reads.

| file | what it holds |
|---|---|
| `chat_log/LATEST.md` → dated `chat_log/*.md` | the session log; resume via `/i:load-session`, dump via `/i:save-session` |
| `v6/DECISIONS.md` | PINNED decisions — do not re-open (counting-not-DRed, SCC fixpoint, dd/salsa-as-teachers) |
| `v6/MAP.md` | the living map: 7-function covering set, the one-cascade unification, exploration verdicts, the DONE contract |
| `v6/findings/HYPOTHESES.md` | **the idea ledger — every promising-but-untested hypothesis with its source session + status (untested/promising/rejected).** The soft-delete/tombstone-for-temporal idea lives here. Add to it; never lose one. |
| `v6/findings/SESSION-DIGEST.md` | the reactive/graph-algo lineage timeline, sourced |
| `v6/findings/SELF-RESEARCH.md` | how to mine our own history (the method) |
| `v6/sprefa-store/FINDINGS-AND-GAPS.md` | measured findings + open algorithmic gaps |
| `v6/plans/*.md`, `plans/*.md` | dated design docs (the retraction model is `v6/plans/2026-07-19-v6-table-design.md:344-368`) |
| `v6/labs/perf-runs.sqlite` / `.csv` | the raw golden-data archive (this contract's output) |

To re-find any past decision or hypothesis, use `tools/chat-find.sh PATTERN`
(rg+fzf over chat_log + the raw CC transcripts), or the command block in
`v6/DECISIONS.md`. When a session ends, its plan/context changes get written back
to these files — that write-back is not optional; a finding that only exists in a
transcript is a finding we will lose.

## Standing discipline (non-negotiable, applies to every agent)

- One OS process per cell; drop input staging before the retract timer so the
  retract memory is not contaminated by the builder — BUT record the builder's
  cost separately (that is the `input_rust_bytes` / `rss_build_kb` the user
  wants).
- **Interpret your own numbers firsthand and DOUBT them.** Identical-across-scale
  numbers are a red flag to disprove. Re-run headline numbers; confirm the
  correctness hash is identical across runs before trusting a time.
- Correctness = blake3/digest vs `benchgraph::oracle_survivors`.
- Never fabricate a number or a query plan; every cell is a reproducible run.
- No `eprintln!` in `src/**` (tracing only); harness `examples/`/`bin/` may print.
