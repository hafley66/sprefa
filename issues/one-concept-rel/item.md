---
created: 2026-08-21
updated: 2026-08-21
type: feature
reporter: chris
assignee: chris
status: open
priority: high
epic: cheap-fast-analysis
---

# One concept: a rel with external arrivals, sh and bind and host collapse into it

## Description

User 2026-08-21: sh, bind and host are all rels that can have external arrivals. Today: sh_decl (name, ins, outs, template) in 12 compiler files, bind_decl (watch, interval) as a second form, host_input_contract keyed on host names in registry.pl, adapters json mapping demand/response rels to executors. Target: one declaration form reusing rel syntax with the demand-to-response arrow, no template string (zero shell made it dead text), executor chosen by the adapter row or the registry, bind forms become rels whose arrivals come from the clock or the watcher executor. No new keywords. Plan first: inventory every consumer of sh_decl/bind_decl with file:line, then the smallest grammar that keeps every compiled program compiling, then the lane.
