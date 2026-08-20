---
created: 2026-08-20
updated: 2026-08-20
type: task
status: in-progress
priority: high
epic: write-verb-interface
labels: [compiler]
---

# port per_rel and shared transient storage to the verb interface, delete the runtime flag branches

## Description

Landed with @write-verb-contract: both runtimes now hold one interface with two
implementations, and the four `shared_frontier` branches inside the tick loops
are deleted. What remains under this card is the work the contract deliberately
did NOT do.

Remaining:

1. `stage_ordered_frontiers` still writes its own per-rel DELETEs around the
   stage verb (`v6/tsv2/runtime/1_incremental.ts`). An ordered-occurrence
   program is refused under `frontier(shared)`
   (`lower.pl` `shared_frontier_todo/3`, reason `tick`), so the names are always
   real tables today; the verb interface should still own that clear.
2. The recursive recount path (`expand_sql`) closes its round with its own
   statement list and does not call the recount verb. Recursion is refused under
   shared, so nothing is wrong today, and the guard is what makes it safe.
3. Rust resolves the strategy per entry point (`write_verbs_for` scans the
   relations slice) where TS memoizes per program in a WeakMap. Same answer,
   different cost: O(rels) integer checks per tick phase, no branch inside any
   loop.
4. The eight `shared_frontier_todo/3` reasons (edge_rules, retention,
   aggregate_head, recursion, departure, non_set_rel, bytes_column, tick, host)
   are each a verb-shaped hole: every one of them writes transient state that no
   verb names yet.

## Acceptance Criteria

- [ ] `stage_ordered_frontiers` clears through the clear verb
- [ ] The recursive recount closes through the recount verb
- [ ] Each `shared_frontier_todo` reason either gains its verb or a written reason it cannot have one
- [ ] Strategy resolution measured once per program on both doors

## Implementation Notes

Interface and both implementations are at
`v6/tsv2/runtime/types.ts:352`, `v6/tsv2/runtime/writeVerbs.ts:219` and `:267`,
`v6/sprefa-engine-rs/src/write_verbs.rs:43`, `:171` and `:262`.

## Tests Run

Nothing yet under this card; the contract card carries the gates for what
landed.
