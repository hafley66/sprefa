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

# extract: visibility on call-family def nodes

## Description

node kind=function|method carries no vis; only rust.rs:1299 reads syn::Visibility. entry_point for a lib root takes every top-level fn. Want vis=pub|crate|private.
