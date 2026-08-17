---
created: 2026-08-15
updated: 2026-08-15
type: task
reporter: fable
status: done
priority: normal
epic: bug-mining
labels:
- size:med
- area:compiler
- bugmine
- pkg:prolog
closed: 2026-08-15
commits:
- hash: 538b8f69
  summary: lowerpl-arm-census coverage census
---

# Coverage census: lower.pl arms and throw sites no corpus program reaches

## Description

The dd arm already noticed mutual_recursion fires on ZERO corpus fixtures (ARCH.pl:950) — from the other side. Systematize: instrument or statically enumerate every lower.pl clause arm and unsupported_construct throw site, cross with what the 448-fixture corpus exercises, report the unreached set. Every unreached arm is an untested claim and a candidate fixture.
