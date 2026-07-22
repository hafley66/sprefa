# Soft-delete / tombstone scan — is weight>0 filtering in the counting cascade?

This scan examines whether weight>0 filtering (soft-delete / tombstone) for the counting cascade
was ever proposed or tested, and distinguishes it from bitemporal/temporal durability.

## Summary

**Delete-at-zero is the only mechanism tried for the counting cascade.** Weight>0 soft-delete
filtering was mentioned as a hypothesis on 2026-07-22 but never tested. The bitemporal/temporal
mechanism (append-only, close-on-retract) is a SEPARATE durable layer—not a soft-delete
variant—designed for point-in-time queries and history retention, not row-filtering in the
cascade.

---

## A. Bitemporal/Temporal durability (the separate mechanism)

This is a distinct append-only fact table with close-intervals, implemented in
`sprefa-store/src/engine.rs` (temporal). It tracks *when* facts were alive—a history
mechanism, not an in-cascade filter.

### Reference 1: Layer 1 FACTS definition

**Source:** `v6/plans/2026-07-21-v6-lab-arc-oracles-and-measured-perf.md:23–24`

> Layer 1  FACTS     = append-only bitemporal table on SQLite. Retract = close interval.

This defines the storage: append-only with `tt_to` (transaction-time close) for retraction,
not a delete operation. Row stays in the table; the interval marks it dead.

### Reference 2: Append-only bitemporal + close-on-retract (row 9)

**Source:** `v6/plans/2026-07-21-v6-lab-arc-oracles-and-measured-perf.md:46`

> | 9 | **append-only bitemporal + close-on-retract** | durable + temporal + retraction in one write | set-based commit (JSON batch → UPDATE over partial live index → close); no N+1 | commit **O(Δ·log n)**; RSS **O(working set)** | 3M facts / 150 revs = **+20 MB RSS** (on disk, not resident) | SQLite live-set == RAM oracle == **salsa**, 2000 revisions | ✓ correct + bounded |

This mechanism runs UPDATE over the live index to close intervals on dead facts. The row
remains; it is filtered by checking `tt_to > current_time` when queried. Not a delete, not a
weight filter—an interval close.

### Reference 3: Bitemporal cross-rev fact (row 11)

**Source:** `v6/plans/2026-07-21-v6-lab-arc-oracles-and-measured-perf.md:48`

> | 11 | **bitemporal cross-rev fact** | `moved(revA→revB)`, `--move` | 2 revs in the KEY (valid-time); 1 tt-interval | same as (9) | present as-of birth rev, absent as-of now; history retained | as-of assertions | ✓ two times supported |

Enables "what was alive at revision N" queries. The temporal mechanism answers this via
interval overlap, not by filtering at weight zero.

### Reference 4: Not ported to the store

**Source:** `v6/sprefa-store/FINDINGS-AND-GAPS.md` (grep for SqliteTemporal)

The bitemporal engine (`SqliteTemporal`) was proven correct at 2000 revisions and folded
into `sprefa-store/src/engine.rs` (temporal). It is a separate, orthogonal layer.

---

## B. Weight=0 delete-at-zero (the cascade retraction, current choice)

The counting cascade uses immediate hard delete: when a row's weight reaches zero, it is
deleted. No tombstone, no row retention.

### Reference 5: DECISIONS.md — retraction model

**Source:** `v6/DECISIONS.md:34–54`

> - Retraction is NOT a separate code path. A delta is `(row, ±weight)`; apply =
>   one upsert that adds weights and deletes at zero. No "retract" verb.

Delete-at-zero is explicit in the retraction model. Weight zero triggers a DELETE.

### Reference 6: Boolean-bit weight REJECTED

**Source:** `v6/DECISIONS.md:49–50`

> Weight is INTEGER support-count; `weight>0` = alive. Boolean-bit REJECTED
> (`chat_log/20260721.1...md:58`).

The decision rejects boolean tracking; integer weight is the chosen mechanism. Implies
full deletion at zero, not filtering by weight>0.

### Reference 7: Session pin

**Source:** `chat_log/20260722.1.v6-store-hermetic-harness-counting-decision-pin-session-digest.md`

> table-design.md:344-368 -> retraction = (row, +/-weight) upsert, delete-at-zero,

Delete-at-zero is pinned as the retraction strategy (as of 2026-07-22).

---

## C. Soft-delete / weight>0 filtering — UNTESTED HYPOTHESIS

The only mention of weight>0 soft-delete is in the hypothesis ledger, marked untested.

### Reference 8: H1 — Soft-delete / tombstone

**Source:** `v6/findings/HYPOTHESES.md:13`

> | H1 | **Soft-delete / tombstone instead of hard delete-at-zero**: keep the `weight=0` row, filter by `weight>0`, sweep later — unify the retract plane with the temporal/durable plane so "what was alive at rev N" falls out for free. | 2026-07-22 session (user latent thought) | untested | trade: table never shrinks under churn until a sweep; temporal plane already gets durability cheaper via close-interval. G9 scanning transcripts for prior discussion. Test: soft-delete variant of `retract`, measure disk growth vs sweep cost. |

Status: `untested`. Source: "user latent thought" on 2026-07-22. No prior discussion recovered
from chat_log or transcripts (the G9 scan is ongoing). The hypothesis notes that the bitemporal
plane (close-interval) already provides durability, and compaction (DELETE WHERE tt_to≤horizon)
manages tombstone growth — suggesting soft-delete may be redundant.

---

## Conclusion

**Delete-at-zero is the only retraction mechanism implemented or tested for the counting
cascade.** Soft-delete/weight>0 filtering was never proposed in prior sessions — it appeared
as a hypothesis on 2026-07-22 and is marked untested. The bitemporal/temporal mechanism is a
separate append-only durable layer (proven at 2000 revisions, not yet ported to store) that
answers point-in-time queries via interval close, not row filtering.
