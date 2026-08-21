---
created: 2026-08-21
updated: 2026-08-21
type: bug
reporter: chris
assignee: chris
status: open
priority: normal
epic: cheap-fast-analysis
---

# extract: Expr::Call df node span is the whole call, the call-family site span is the ident

## Description

rust.rs:1979-1985 vs :2028. On boop main.rs 80 of 218 arg rows join a site by span; 0 of 33 run_* sites do. Want the Expr::Call node at the callee ident or a call_ident record.
