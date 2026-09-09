---
created: 2026-09-08
updated: 2026-09-08
type: improvement
status: open
priority: normal
related: ['@extract-reach-entry-sink']
labels:
- pkg:extract
- size:small
---

# resolved_edge carries in_loop and cfg so reachability needs one pass

## Description

## Ask

`resolved_edge` should carry `in_loop: bool` (site is inside a `df_nest` row in the caller's file) and `cfg: string|null` (site is inside a `cfg_scope` span). Both facts are already computed per file during `--resolve`; putting them on the edge removes the second per-file pass every reachability consumer runs today.

## Receipt

`loop_reach.py` (attached to @extract-reach-entry-sink) runs `extract --family call,df` once per file only to rebuild `df_nest` and `cfg_scope` membership for offsets the `--resolve` stream already names in `caller_site_start`.

## Acceptance Criteria

- [ ] `--resolve` edges gain `in_loop` and `cfg`; absent (`false` / `null`) when the caller's language emits no df rows
- [ ] `--schema` documents both fields
- [ ] existing resolved_edge consumers see no other change
