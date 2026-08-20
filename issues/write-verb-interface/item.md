---
created: 2026-08-20
updated: 2026-08-20
type: epic
owner: hafley66
status: open
priority: high
labels: [compiler, runtime, sqlite]
---

# Write-verb interface: one relational lowering IR, storage strategies behind six verbs

## Description


### 2026-08-20 · @sprefa-coordinator

PAUSED by Chris (perf grind takes the machine). Branch feature/write-verb-interface @ 12ffa6a5c, pushed, no PR. Code complete: six-verb contract (lower.pl:6951-7035, IWriteVerbs runtime/types.ts:352 + writeVerbs.ts, Rust trait write_verbs.rs, flag branches deleted), step-5 shared __support_count recount, 4-fixture parity battery green both doors. Unrun: v6/tsv2 npm test (killed under load), named in TASKS/write-verb-interface.PAUSED.md. Resume = run npm test, then PR "supersedes #378".
