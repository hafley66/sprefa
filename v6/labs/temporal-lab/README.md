# temporal-lab — append-only bitemporal on SQLite + retraction, proven

Is "single binary + SQLite, reactive-temporal, efficient dirty-check + retraction" a
good thing? Run it.

```
cargo run --release --example temporal_proof
```

The table: `fact(key, tt_from, tt_to, weight)`, PK `(key, tt_from)`, partial index on the
live set (`tt_to IS NULL`). Valid-time is in the KEY (coordinate model); transaction-time
is the interval. **Retract = weight hits 0 → `SET tt_to = R`, never DELETE.** That one
choice makes it durable (append-only), gives history for free, and makes as-of a filter.

Writes are set-based (JSON-batched delta → one `UPDATE` over the live index → one close),
so no N+1. SQLite does the list-scaling; the reducer lives in SQL.

## Four phases, all green

| phase | what | result |
|---|---|---|
| 1 correctness | random edit stream, 2000 revisions | SQLite live-set == RAM oracle == **salsa** at every checkpoint |
| 2 bitemporal | `moved(rev5→rev7)` — a fact carrying TWO revs | present as-of birth, absent as-of now, history retained |
| 3 scale | 3M live facts, 150 set-based revisions, file-backed | **peak RSS 31 MB** (base 11) — facts on disk, not resident |
| 4 compaction | churn → 800k dead-interval rows | compact deleted 780k, rows 3.8M→3.02M, **live digest byte-identical** |

## The two worries, answered

- **"how do we keep rotation/compaction at bay?"** — `compact(horizon)` is one
  `DELETE WHERE tt_to <= horizon` + `VACUUM`. It never touches the live set (`tt_to IS
  NULL`), and any as-of query at `tt ≥ horizon` is unaffected (a dropped interval ended
  before the horizon, so it never contained `tt`). Phase 4 proves the live digest is
  unchanged across the compaction. Size returns to ~live; growth is bounded by retention.
- **"cross-rev facts, across 2 instances of time?"** — yes. Three cases all work:
  disjoint validity (multiple interval rows per key), a fact relating two revs (both revs
  in the key, one tt-interval — Phase 2), and full bitemporal (valid-time in key,
  transaction-time in interval). The interval machinery only ever touches transaction-time,
  so it is identical regardless of how many revs the key encodes.

## Independent oracle

Correctness is not self-graded: at every checkpoint the SQLite engine's live-set XOR
digest must equal an in-RAM weighted multiset AND salsa's memoized digest — three separate
implementations of "the live set." Compaction safety is the same digest, byte-identical
before and after. See `v6/plans/2026-07-21-v6-lab-arc-oracles-and-measured-perf.md` for the
full arc.
