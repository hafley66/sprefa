# Duel: CST extraction demand — eager corpus vs lazy query-demanded (Fork B)

Base 92756b54. Decision agent doc, judged on verified claims. Every claim below carries a
read source path.

## 1. Ruling: LAZY — demand is the only materialization trigger; "eager" is a demand-set, not a mechanism

**Boundary, exact:**

- The ONLY thing that materializes a CST fact is the `extract` HostDef firing on a
  `cst_need(path, content_hash, lang)` demand row, idempotent via the existing
  `effect_cache` content-addressed digest (full_digest = mix(host, ...requestCols),
  `v6/dl/src/0_types.ts:163-174`, `v6/dl/src/1_hosts.ts:75-76,478-491`). Nothing is
  materialized at ingest.
- Demand rows are derived from query goals (the planner/compiler emits `cst_need` for
  the (path, hash, lang, family-mask) each live goal needs). A file with no live
  structural query never fires `extract` for the CST plane.
- Derived/reindex views recompute over whatever the cache currently holds, the v5
  source-vs-derived shape (`v6/sprefa-extract/src/types.rs:167-184` CstF is lossless,
  per-file, no cross-file resolution, so there is no global plane to keep consistent).

**"EAGER" is not a competing mechanism.** An ingest-time planner that pre-plants
`cst_need(P,_,L) <- files(P)` for every file is the same code path as lazy query
demand: same HostDef, same effect_cache, same batched grouping (`v6/prolog/ARCH.pl:767`
extraction_host_batching landed; same-frontier demands grouped by executor/template/
path/digest, one run). Eager is a demand-set that happens to cover the corpus. So the
fork reduces to "who plants demand rows", and ruling LAZY means the planner plants them
from goals, not from ingest.

Three archive facts drive this:

1. **Correctness of any scheme requires content hashing in the key, and the machinery
   for that already fires on demand.** `v4/perf/baseline.toml` `[contenthash]`: a
   clock-only staleness oracle silently replayed a stale edit (16627 vs 16628 correct);
   the fix was blake3(input bytes) as the staleness oracle. `cst_need` carries
   `content_hash` (the fork spec), so a content edit yields a new full_digest -> refire;
   an unchanged file is a cache hit with zero spawn. Arch row `ARCH.pl:813`
   (staged_writes): "effect identity is content-addressed over the DEMAND only."
2. **The eager reform is latency-hostile for exactly the shape queries are not.**
   `ARCH.pl:710` + `ARCH.pl:814`: v6 served extraction runs 40.7 files/s vs v5's
   3,540.9 (87x) because host subprocesses run at concurrency 1.0. Eager front-loads
   that wall into ingest for the whole corpus; lazy pays it only for the demanded
   subset. And `bench/scip_perf_results.md`: the warm oracle is the in-memory server
   hitting ms per file, not a batch pass; lazy per-(path,hash) extraction is the editor
   shape.
3. **Selectivity is real and measured.** `bench/printk.dl` + justfile:46 ("v4 linux
   bench equivalent"); `chat_log/20260505.0...:8`: only ~5% of 63k files pass the
   printk prefilter, and parse is the 3.87s lower bound. Full-lossless eager parses the
   untouched 95%; lazy never does unless a goal demands it.

Effect spawn count is bounded by the extracted set, not the query count
(`ARCH.pl:810` effect_chain_batch: 700 demands -> 100 spawns, fan-out dedupe over
identical values; `ARCH.pl:767` groups same-frontier demands before one run).

## 2. Archive evidence table

| claim | source path | what it says |
|---|---|---|
| whole-roll extraction is the costliest shape and is /db-bytes heavy | `v6/prolog/ARCH.pl:861` (files_repos_p2) | capturing the extractor's whole JSONL instead of one row/file took the 779-file corpus 20.26s -> 62.97s and db 1.0 MB -> 595 MB. "real extraction-seam number, wrong question for this bench." |
| v6 per-file extraction is the throughput wall (concurrency 1.0) | `v6/prolog/ARCH.pl:710` + `:814` | v5 org-fan 3,540.9 files/s vs v6 served 40.7 files/s (87x); root cause "host subprocesses run at concurrency 1.0". Lazy shrinks the extracted count; eager multiplies the wall by corpus size. |
| extraction is content-addressed by demand; cache/refire rides content_hash | `v4/perf/baseline.toml` `[contenthash]` | staleness oracle = blake3(input bytes) vs recorded hash, "clock-independent". incremental 15.70s vs seam-off 53.45s, parse collapsed 18600.9ms -> 0.8ms; clock-only oracle silently wrong (16627 vs 16628). |
| demand identity is content-addressed over the demand only | `v6/prolog/ARCH.pl:813` (staged_writes) | "the engine is BLIND to the disk it wrote because effect identity is content-addressed over the DEMAND only"; writes at-least-once. |
| effect batching amortizes spawn/s; identical-input dedupe, not value fan-in | `v6/prolog/ARCH.pl:810` (effect_chain_batch) + `:767` | 700 demands -> 100 spawns; "FAN-OUT DEDUPE OVER IDENTICAL VALUES"; extracts grouped by executor/template/path/digest, one stdout. |
| extraction is parse-bound with strong selectivity | `chat_log/20260505.0.linux-bench-tail-sync-cap-fix.md` | 3.87s lower bound parse-bound; ~5% of 63k files pass printk prefilter; batch_cap drives tail-sync not memory; multi-rule scans share the parse. |
| position-keyed reconcile needs path-unique ids; content-keyed memo ignores them | `chat_log/20260504.11.linux-bench-reconcile-path-collision-fix.md` | 94% match drop + 1.8x cold from one path-collision bug; "content-keyed memo and position-keyed reconcile read DIFFERENT row attributes." |
| v4 cold scan shape + numbers for the lab lineage | `v4/perf/baseline.toml` `[cold.main]`, `[reference.main_v4_bench]` | linux ~37k .c: wall 5.06s, fs_seen 93299, fs_emitted 63482, ast_parses 4495, ast_matches 16627; bare median 4.151s. |
| v3 parallel-extraction batcher strategies | `v3/crates/effect_runtime/src/batchers/` mod.rs + work_steal.rs + bounded_batched.rs + cache.rs | 4 shapes: Passthrough (concatMap), WorkSteal (rayon per-request, "within 5% of ast-grep CLI"), BoundedWorkSteal (tokio mpsc->rayon, backpressure), BoundedBatched (coalesce for sqlite/git/network amortization), CacheLayer (moka content-keyed). |
| CstF is lossless per-file JSONL, node/child only | `v6/sprefa-extract/src/types.rs:167-184,1330-1335,1369-1372` | "lossless named-node tree (tree-sitter CST)"; single edge kind Child; per-file interner; wire `FlatFact` JSONL `{"record":"node"...}`. |
| extract / effect_cache already a HostDef with content-addressed dedupe | `v6/dl/src/0_types.ts:163-174,188-190`; `v6/dl/src/1_hosts.ts:75-76,478-491` | full_digest = mix(host,...requestCols) is PK, fire-once per witness; `?` idempotent; new digest within identity group supersedes. |
| warm oracle is in-memory server, not batch CLI | `bench/scip_perf_results.md` | "SCIP-via-CLI is always ~11s; the warm oracle is the server, and the server does not emit SCIP." |

## 3. Steelman against LAZY (where low demand-set wins)

**Whole-repo CST rewrite / level-set diff as the first cold query.** A program whose
goal is "re-emit every file's CST" or "diff node spans across two revs of the entire
tree" demands all (path, content, lang). Under lazy:

- The cold first query pays the full extraction on the interactive request clock, not
  at ingest. On today's seam (concurrency 1.0, 40.7 files/s, `ARCH.pl:710/814`), a
  63k-file linux tree is ~26 minutes of wall in the request path before a single shape
  of `node`/`child` is answerable. Eager burned that time at ingest in the background,
  and the query is pure indexed reads.
- Extract-grouping (`ARCH.pl:767`) amortizes, but at full-corpus coverage lazy is
  doing identical work to eager with the same subprocess ceiling, plus demand
  bookkeeping overhead (plant rows, cache rows, supersession churn per content flip).
- No atomic whole-plane guarantee: a rewrite that must see all CST of one rev in one
  consistent snapshot can't trust a partially-populated cache; eager gives the materialized
  plane for free.

Flip trigger: when the live goal set covers ~the whole corpus and the query is in the
request path, lazy has surrendered its selectivity advantage and kept its spawn
overhead. That is the regime the lab below measures.

## 4. The definitive lab

Reuse the linux lineage (`bench/run.sh`, `bench/printk.dl`, justfile `bench-printk` /
`bench-printk-on`). Corpus: a real kernel checkout at HEAD (bench/linux-sim is the CI
smoke, not a scale bed).

**Program:** a `.dl6` structural query that exercises the `cst` node/child plane
(replace printk's `sg` via the CST plane), e.g. "count named nodes by kind and list
every `function_declaration` span." Run a project-wide form (goal demands CST for all
files — the eager-equivalent demand set) and an editor-selective form (goal demands a
small file subset, e.g. one driver dir).

**Method — two hardware arms on one db:**
- Arm A = ruling (lazy): fresh db, plant `cst_need` only from goals, cold-query once.
- Arm B (eager-equivalent): same binary, pre-plant `cst_need(P) <- files(P)` at ingest,
  then run the same query. This isolates the demand-set width from the code path, which
  is the whole bet (section 1: eager is a demand-set).

**Metrics (all already tracible via the DL_PERF_LOG seams, `v6/dl/src/0_types.ts:435-470`):**
- fact count: rows on `rel_<cst_node>` / `rel_<cst_child>` after settle.
- on-disk bytes: db size post-ingest (whole-roll JSONL reference: `ARCH.pl:861`).
- ingest wall: settle time with zero goals (A) vs with all-files planted (B).
- cold-query latency: first query wall after plant, from request to answer.
- effect spawn count: `HostEffectDone` / PerfEffectEntry (`effects`) per run.

**Numeric threshold that flips the ruling:** keep LAZY iff, over the workload,
`coverage = distinct cst_need(path,content,lang) / distinct files < 0.85` AND Arm A
`cold-query latency <= budget` (budget = 10x the warm in-memory read path per
`bench/scip_perf_results.md`). Commit to eager pre-planting (= Arm B as the default,
still through extract + effect_cache) iff coverage >= 0.85, because at full coverage
lazy holds no selectivity win and pays spawn/concurrency 1.0 overhead on the interactive
clock. The decisiveness comes from coverage: below it lazy's only failure is latency on
the first project-wide query and that is curable by a warm-up demand rule without
changing the mechanism.

## 5. Risks

1. **Cold project-wide CST-query latency on the request clock.** Earliest symptom: a
   fresh-db run of a whole-tree structural query takes minutes (DL_PERF_LOG effect
   `ms` dominated, effects count ~ file count) while warm reads are ms. Mitigation:
   pre-planted warm-up `cst_need` demand rule; the effect_cache makes re-demands hits.
2. **Cache staleness via an incomplete demand key.** If `cst_need` omits `lang`/family
   mask or the extract template, a content edit produces a silent wrong answer (the
   v5 collect-style silent miscompile class, `ARCH.pl:811` F9; the v4 16627-vs-16628
   stale-replay class in `baseline.toml [contenthash]`). Earliest symptom: a byte-identical
   edit yields no re-extract row / no supersession (no new full_digest); flush by
   kill -9 and replay exactly-once (`ARCH.pl:813`).
3. **Extraction-seam throughput wall under CST-heavy or full-coverage workloads.** The
   concurrency-1.0 subprocess ceiling (`ARCH.pl:814`, v6 40.7 vs v5 3,540.9 files/s)
   becomes the bottleneck; batching (`ARCH.pl:767`) bounds it, not removes it. Earliest
   symptom: wall-to-file-count ratio far below the v5 crawl-bench yardstick, effect
   spawn count ≈ distinct files for a CST-wide goal, RSS rising as effect_cache rows
   accumulate.
