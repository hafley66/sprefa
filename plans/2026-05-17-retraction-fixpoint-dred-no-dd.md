# Retraction + Fixed-Point via DRed (no DD, capped RSS)

Status: PLAN. 2026-05-17. Sister doc: `v4/docs/v4-retraction-fixpoint-plan.md` (long form, signatures+lifetimes).

Goal one line: dep change (file/blob/buffer/rule table) → retract exact old rows → re-derive → works under recursion → RAM flat, N on disk.

DD dead. DD = arrangement trace, all RAM, NO rss budget, grows w/ distinct tuples. Replace w/ 5 prims: **key/val hash split · recorded deps · sqlite memo · support mult · DRed over stratified semi-naive**.

---

## Big-O. Why move.

| axis | now / DD | after plan | reason |
|---|---|---|---|
| time/change | DD O(Δ·logN); naive O(N) | O(δ·logN), δ=slice | exact deps, touch dependents only |
| **rss** | DD O(N) RAM, no cap | O(cap) RAM + O(N) disk | memo+graph off-heap, LRU bounded |
| retraction | today presence-only O(N) rescan | DRed O(closure) | mult → del when last path gone |
| fixpoint | DD frontier | strat semi-naive O(ΣΔ⋈) | join Δ not full relation |
| unchanged in | DD still ticks | O(1) replay | memo gen-compare, op 0× |

```
   RSS vs CORPUS (poke 1 file, 500 repos)
   ──────────────────────────────────────

   RAM ^
       |                              DD .....●  OOM
       |                        .....●
       |                  .....●          grows w/ distinct tuples
       |            .....●                no spill no budget
       |      .....●
       |●──────────────────────────────●  PLAN  flat: cap LRU + batch
       |________________________________________→ N
            disk grows instead (cold, cheap)
```

---

## What plan touch / dominate

DOMINATE = file's behavior changes shape, not just edit.

| area | files | verb |
|---|---|---|
| cursor identity | `v4/src/lib.rs` (Cursor ~396), `v4/src/compile/lower/ops.rs` (`OperatorDef`) | DOMINATE: add key/val hash, `key_terms()` |
| source clock | `v4/src/store.rs` (caps 29-36), `v4/src/sql.rs` (table_version 316), `v4/src/app.rs` (bump 1002) | DOMINATE: `table_version` dies, fold into `SOURCE_GEN` |
| dep capture | `v3/crates/effect_runtime/src/v2/expand.rs` (RenderCtx), `v4/src/compile/lower/ops.rs` (fs/read/fact lower), `v4/src/mounted_query.rs` | touch: record_read seam |
| memo | NEW `v4/src/memo.rs`; `v3/.../v2/expand.rs` (driver loop ~203-298) | DOMINATE: probe→replay vs run |
| reconcile | `v3/.../v2/expand.rs` Phase-E hook (239-245) | DOMINATE: TODO → real diff |
| support mult + DRed | `v4/src/mounted_query.rs` (retract 597-642), `v3/.../v2/runtime_graph.rs` (replace_supports 908-955) | DOMINATE: presence → integer mult |
| fixpoint | `v4/src/rule.rs` (Rule 32, RuleInvoke 285), `v3/.../v2/runtime_graph.rs` (mark_dirty 642, dirty_owners 665, sweep 718), `v4/src/runtime_graph.rs` (516-547) | DOMINATE: stratify + semi-naive |
| kill leaks | `RuleMemo` hashmap → `memo.rs`; seen-set dashsets → `SOURCE_GEN`; pending buffers → flush on stratum | DOMINATE: 3 unbounded heap die |
| proof | NEW `v4/tests/rss_slice_proof.rs`, `v4/src/bin/v4_bench.rs` | touch: pinned bench |

DD attic stay quarantined: `v4/src/_attic/dd.rs`, `v4/Cargo.toml:29-30` deps removable after Ph6.

---

## Phases. Order fixed. Each ship + test.

```
   Ph0  key/val hash ─┐ diff possible. churn O(rows)→O(changedkeys)
   Ph1  SOURCE_GEN  ──┤ invalidation O(deps) not O(owners). table_version dies
   Ph2  record_read ──┤ deps EXACT. dispatch_wake → slice only
   Ph3  memo sqlite ──┤ RSS off heap. unchanged = O(1) replay
   Ph4  reconcile   ──┤ re-render → 1 Retract+1 Assert (Phase-E hook fires)
   Ph5  mult + DRed ──┤ retraction O(closure), diamond-safe
   Ph6  stratify    ──┘ recursion terminates, replace DD frontier
   Ph7  rss proof    ── pin it
```

### Ph0 — cursor identity split
- `OperatorDef::key_terms() -> &[&str]` default `&[]` (= whole cursor key, safe coarse).
- `Cursor::key_hash()` / `val_hash()` (`v4/src/lib.rs`). blake3, order-indep over term set.
- migrate `content_hash()` callers in `v4/src/sql.rs`, `v4/src/mounted_query.rs`.
- test: value-only edit → key_hash same, val_hash differ.

### Ph1 — source clock, one for all
- NEW table `SOURCE_GEN(source_id PK, gen)`. `trait SourceClock { current_gen; bump }`.
- `SourceId = blake3("src"++uri)`. file, lsp buffer, AND rule/fact table all = SourceId.
- DELETE `table_version` (`v4/src/sql.rs:316`), `bump_table_version` (`v4/src/app.rs:1002`) → call clock.
- event layer (fs-watch/lsp/fact write) is ONLY bump caller.
- test: file edit bumps gen; rule write bumps its table gen.

### Ph2 — dep capture
- `RenderCtx.deps: RefCell<Vec<(SourceId,gen)>>`. `record_read(s)` push.
- wire fs/read/fact-read lower (`v4/src/compile/lower/ops.rs`, `v4/src/mounted_query.rs`).
- persist `MEMO_DEPS(owner_op_id,in_key,source_id,gen_seen)`.
- test: parse file → file SourceId in owner MEMO_DEPS.

### Ph3 — memo + replay
- NEW `v4/src/memo.rs`. `MEMO(owner_op_id,in_key PK, out_rows,out_keys,dep_fp,gen)`.
- driver (`expand.rs` loop): probe before run. all dep gens equal → replay out_rows, op dispatch 0×. else stale.
- hot = existing `StripedLru`; cold = sqlite. `RuleMemo` hashmap (`v4/src/rule.rs`) DELETED, points here.
- test: unchanged source rerun → dispatch count == 0.

### Ph4 — reconcile (the Phase-E hook)
- `expand.rs:239-245` TODO → `reconcile(prior_memo, fresh) -> Vec<Delta>`.
- key gone → Retract. same key new val → Retract+Assert. new key → Assert. same → noop.
- test: value edit → exactly 1 Retract + 1 Assert.

### Ph5 — support mult + DRed
- `SUPPORT` gain `mult i64`. row live iff Σmult>0.
- `cascade_retract`: dec mult; ==0 delete + recurse children; >0 keep (other path).
- replace presence delete in `v4/src/mounted_query.rs:597-642`, `replace_supports` `runtime_graph.rs:908-955`.
- test: row w/ 2 supports survives losing 1.

### Ph6 — stratify + semi-naive fixpoint
- `stratify(&RuntimeGraph)->Vec<Stratum>` at lower; neg/antijoin edge → higher stratum. unstratifiable cycle = diagnostic at lower time.
- `eval_stratum`: drive `RUNTIME_DIRTY` (`runtime_graph.rs` 642/665/718), re-render owner over dirty Δ only, loop til Δ empty, then next stratum.
- recursive rule (`v4/src/rule.rs`) joins Δ not full relation → terminates, memo blocks re-derive.
- test: transitive-closure rule terminates; mid-graph edit retracts closure.

### Ph7 — RSS proof
- NEW `v4/tests/rss_slice_proof.rs`, hook `v4/src/bin/v4_bench.rs`.
- 500-repo corpus, poke 1 file. assert: RSS delta < cap; recomputed owners == affected slice; not O(N).
- after green: drop `differential-dataflow`/`timely` from `v4/Cargo.toml:29-30`.

---

## Soundness invariants (must hold all phases)

- `RowId = blake3(owner++in_key++ordinal)` pure → memo replay exact.
- row in sink table IFF Σ SUPPORT.mult > 0. debug_assert.
- no neg edge within/down a stratum. `stratify()` reject at lower.
- termination: per gen monotone-under-union + memo blocks re-derive → least fixpoint finite rounds. DRed delete half finite (bounded by transitive support).

## 3 calls before Ph4
1. `key_terms()` default — recommend per-op opt-in. `re`/`ast`/`json`: capture names = key, span = val.
2. DRed vs Counting — DRed now, keep SUPPORT schema wide for upgrade.
3. memo eviction — LRU fine for cap RSS; watch Ph7 recompute storms.

## DD answer, recorded
DD no RSS budget. arrangement = all live (k,v,t) tuples + index, RAM-resident, no spill. compaction bounds history not width. that = the 70% you can't afford at 500-repo. plan trades arrangements+frontier for sqlite memo + ordered sweeps. same correctness, stratified Datalog. cheap 30% (mult, support teardown) kept.
