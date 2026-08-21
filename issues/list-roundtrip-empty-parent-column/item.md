---
created: 2026-08-17
updated: 2026-08-17
type: bug
status: open
priority: high
labels: [domain-v6, component-lower]
---

# Four list round-trip programs emit an empty parent list column in tick 1

## Description

Sweep on origin/main 0bcb91657: 4 wrong. list_bare_column_round_trips, rel_element_list_round_trips, nested_rel_element_list_round_trips, recursive_list_arg_parent_holds_child_node_values. Actual tick 1: the __gen__list_* and __member rows land, but the parent rel row carries [] where the oracle carries the list. Appeared with the storage-namespace merge ed3dcbd3e (1fba75557 dl6: namespace SQLite relation storage) landing on main 2026-08-17. Was identical=342 wrong=0 on 2b3c33ea0. Owner: the peer session driving list-persistence / typed-collect.
