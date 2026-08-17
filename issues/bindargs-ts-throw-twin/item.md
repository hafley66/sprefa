---
created: 2026-08-15
updated: 2026-08-15
type: task
reporter: fable
status: done
priority: normal
epic: list-ergonomics-closeout
labels:
- size:med
- area:engine
- pkg:tsv2
- pkg:prolog
closed: 2026-08-15
commits:
- hash: 78464b06
  summary: 'emit_ts+tsv2 typed BoundaryError at the SQL-parameter seam, PR #275'
---

# TS door: bind_args throw on list-at-SQL-parameter becomes unrepresentable or typed

## Description

PR #260 removed the six Rust list panics (ScalarValue seams + BoundaryError). The TS twin remains: runtime/1_incremental.ts:73 bind_args and the emit_ts.pl:592 template both throw new Error('a list value reached a SQL parameter'). Mirror the fix: IRowScalar already names the seam type-side; make the runtime path a typed error consistent with the Rust door's Display bytes. Needs the user's earlier no-panics intent read as covering the TS door — flag in PR if judged otherwise.
