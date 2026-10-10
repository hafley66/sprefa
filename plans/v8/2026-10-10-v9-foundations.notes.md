# v9 foundations: notes

Status: notes from chat, 2026-10-10. Nothing decided.

User, 2026-10-10: "how can we ensure efficient dd ram and storage graph backpressure. half of me
wants to go v9 and from first foundations ensure dd and storage and durability are answered for
in pure ivm/dd/iterator terms of backpressure and graph theoretically only parsing/phasing what
is needed and identifying evcen phasing in that with scc"

## Concerns, in DD/IVM terms

| concern | answer in DD/IVM terms | where it sits today |
|---|---|---|
| parse and phase only what is needed | demand: compute only relations a sink reads (magic sets) | `_6_eval` demand code |
| phases from the graph | SCCs of the dependency graph are strata; clock labels pick loop vs DD per SCC | `src/_6_eval/_2_stratify.rs`, `plans/v8/2026-10-09-clock-checker-strategy.design.md` |
| DD RAM | arrangements are the memory: share, compact, drop unread ones | `sqlite_ivm/crates/ivm-dd` |
| storage, durability | one transaction per frontier; write deltas, not snapshots | `src/_9_runtime/_2_reconcile.rs` |
| backpressure | bounded channel into the DD input; producer awaits | not built |

## Related

- `plans/v8/2026-10-10-rust-app-emitter.notes.md`
- `plans/v8/2026-10-09-dataflow-memory-budget.plan.md`
