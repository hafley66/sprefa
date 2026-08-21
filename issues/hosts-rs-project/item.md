---
created: 2026-08-21
updated: 2026-08-21
type: task
reporter: chris
assignee: chris
status: open
priority: normal
epic: cheap-fast-analysis
---

# hosts.rs: project a group's shared answer once per plan, not once per demand

## Description

collect calls project per demand and select_columns re-projects the whole answer each time: 152 demands x 18859 rows on the rail. Memo per distinct plan in the group; resolve output ordinals once per projection; drop the two String clones in claim_once; plan_by_demand_rel index. Diffs and an ignored COUNT test (a_host_group_projects_its_answer_once_per_plan) are in tests/n_plus_one.rs. Also cfg.rs:233 role lookup per node and scip.rs:622 definition_of global scan (unmeasured).

## Comments

### 2026-08-21T17:24:55Z · @chris

Also: LINKED_EXECUTORS at hosts.rs:41 omits ast_rule although executor_for dispatches it at :48 (v5 census lane finding).
