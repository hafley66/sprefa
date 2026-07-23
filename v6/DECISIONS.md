# v6 pinned decisions — STOP re-deriving these

> **How this pin got made:** we mined our own session logs + raw Claude Code
> transcripts to recover decisions we kept re-deriving. See
> `v6/findings/SELF-RESEARCH.md` for the method (rg the `.jsonl` transcripts, 8
> haiku passes over chat_log) and `v6/findings/SESSION-DIGEST.md` for the lineage.


If you are about to re-open "counting vs DRed", "how do cycles work", "weight
semantics", "acyclic fast-path", or "are these graph algos the same thing":
DON'T. It is decided (6+ times). Pointers below.

## THE UNIFICATION (the thesis, settled — do not re-argue it)

Salsa reactivity (dep dirty-propagation), SCC decomposition, reachability /
blast-radius, and dd/feldera Z-set incremental maintenance are **the same graph
counting algorithm**. They sound eerily alike because they ARE one thing:

    ONE semi-naive cascade:  frontier → one hop → prune → fixpoint

The ONLY thing that varies is the prune predicate (`v6/ARCHITECTURE.md:73,108,144`):
  - A · control (salsa)     → prune by **digest** (early-cutoff)
  - B · facts (dd/feldera)  → prune by **weight ≠ 0** (Z-set counting)
  - C · reach (SCC/blast)   → prune by **reached**

Unifying them in SQLite is HIGHLY VIABLE and is the plan. The remaining work is
NOT choosing an algorithm — it is **empirically deriving each path's Big-O and
driving it down** (measured, not asserted). This is what the perf harness is for
(`examples/perf_report.rs`, `profile_dred.rs`, `explain_plans.rs`). The point of
all of it: kill v5's resident 36GB-swap model by keeping state on disk (RSS
bounded by page cache, Rust heap ~0), while matching the resident engines on
correctness and driving the counting Big-O down toward them on speed.

## The retraction / recursion model (DECIDED)

Source of truth: **`v6/plans/2026-07-19-v6-table-design.md:344-368`**.

- Retraction is NOT a separate code path. A delta is `(row, ±weight)`; apply =
  one upsert that adds weights and deletes at zero. No "retract" verb.
- **Recursion is handled WITHOUT DRed.** Weight = support count (3 rules → weight
  3; kill one → weight 2, survives). Arithmetic answers it. "Feldera is this,
  and nothing more exotic than this."
- **Cycles: NOT DRed.** Run the recursive SCC's fixpoint as a nested loop to a
  least fixed point before publishing deltas outward, so weights inside a cycle
  settle before anyone sees them. The SCC is the unit (`trace_dep`).
- Explicitly NOT adopted: salsa (resident memo), differential-dataflow (resident
  arrangements — the "you have enough RAM" assumption that fails at 500 repos and
  is the v5 36GB swap nightmare we are killing).
- Weight is INTEGER support-count; `weight>0` = alive. Boolean-bit REJECTED
  (`chat_log/20260721.1...md:58`).

DRed / `retract_dred` / `retract_dred_cte` in `v6/sprefa-store/src/engine.rs` are
the ORACLE comparison engines, not production. Do not optimize them for production.
The production retract is the counting upsert + SCC nested fixpoint.

Supporting: `v6/ARCHITECTURE.md` (one semi-naive cascade, prune =
digest·A / weight·B / reached·C). DRed derivation was in the deleted
`v6/labs/labkit/WHY-DRED.md` — `git log --follow` it if needed.

## The TS engine + rxjs lowering (DECIDED 2026-07-23)

- The reactive engine is **TS on ACTUAL rxjs** (Observable / Subject /
  BehaviorSubject + a BufferPolicy knob). NOT a Rust rx re-implementation. json-rx
  is EXTRACTED from an rxjs graph (round-trip proof), not a lowering target.
- The Rust crate keeps its job: the SQLite cascade (Reach / Cascade / Reconcile /
  GraphStore) + extraction (WASM/CLI). It is ported 1:1 to TS at
  `v6/sprefa-store/js/` so the rxjs layer calls the same knobs + reads the same
  SQLite. Golden-gated 11/11, peak RSS 141 MiB. dd/salsa stay Rust-side oracles,
  NOT ported to TS.
- The **fixpoint stays in SQLite** (the cascade). rxjs owns the control plane
  (demand, dirty, wake, compose) — the part v5's global tick did badly. Re-doing
  Z-set IVM in TS is the resident-RAM trap the unification killed.
- **BOOKMARK (owner, 2026-07-23):** groupBy / aggregation / latest-by-gen lower
  INTO SQL (`GROUP BY` + `LIMIT`) at the `dirty` boundary, never into TS arrays.
  Plan: `v6/plans/2026-07-23-v6-rxjs-lowering-and-ts-port.md`.

## How to re-find any past decision (the commands that work)

```bash
# raw Claude Code transcripts (the real conversation, ranked by hit count):
rg -i -c 'PATTERN' ~/.claude/projects/-Users-chrishafley-projects-sprefa/*.jsonl \
  | sort -t: -k2 -rn | head
# then pull phrases without dumping the 500MB file:
rg -i -o '"[^"]*PATTERN[^"]*"' ~/.claude/projects/-Users-chrishafley-projects-sprefa/<uuid>.jsonl | head

# session summaries + design decisions:
grep -rniE 'PATTERN' chat_log/*.md plans/ v6/plans/ v6/*.md

# decision docs live in plans/ and v6/plans/ (dated YYYY-MM-DD-topic.md)
```
