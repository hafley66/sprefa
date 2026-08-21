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

# scip interface: carry caller_name and caller_site_start

## Description

schema.rs:47 emits them on resolved_edge; registry.pl:516-519 scip_interface_columns(call) lists six columns without them, forcing every consumer into a per-file sites x defs containment join (the largest cost in feature-reach). Edit the registry row and scip_namespaces.test.pl together.
