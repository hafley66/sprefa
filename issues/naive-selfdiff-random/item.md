---
created: 2026-08-15
updated: 2026-08-15
type: task
reporter: fable
status: open
priority: normal
epic: bug-mining
labels:
- size:small
- area:testing
- bugmine
- pkg:tsv2
blocked_by: ['@fuzz-grammar-threedoor']
---

# Naive self-diff wired into the fuzzer loop

## Description

Once fuzz-grammar-threedoor emits random programs, add the naive mode as the fourth judge in its loop (cheap: same emitted module, env flag). Small: one flag plumb + diff, gate is byte equality.
