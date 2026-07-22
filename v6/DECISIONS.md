# v6 pinned decisions — STOP re-deriving these

If you are about to re-open "counting vs DRed", "how do cycles work", "weight
semantics", or "acyclic fast-path": DON'T. It is decided. Pointers below.

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

DRed / `retract_dred` / `retract_dred_cte` in `v6/sprefa-store/src/cascade.rs` are
the LAB COMPARISON engines, not production. Do not optimize them for production.
The production retract is the counting upsert + SCC nested fixpoint.

Supporting: `v6/ARCHITECTURE.md:73,108,144` (one semi-naive cascade, prune =
digest·A / weight·B / reached·C), `v6/labkit/WHY-DRED.md` (the experiment).

## How to re-find any past decision (the commands that work)

```bash
# raw Claude Code transcripts (the real conversation, ranked by hit count):
rg -i -c 'PATTERN' ~/.claude/projects/-Users-chrishafley-projects-sprefa/*.jsonl \
  | sort -t: -k2 -rn | head
# then pull phrases without dumping the 500MB file:
rg -i -o '"[^"]*PATTERN[^"]*"' ~/.claude/projects/-Users-chrishafley-projects-sprefa/<uuid>.jsonl | head

# session summaries + design decisions:
grep -rniE 'PATTERN' chat_log/*.md plans/ v6/plans/ v6/*.md v6/labkit/*.md

# decision docs live in plans/ and v6/plans/ (dated YYYY-MM-DD-topic.md)
```
