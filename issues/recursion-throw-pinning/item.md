---
created: 2026-08-15
updated: 2026-08-15
type: task
status: done
priority: normal
labels:
- size:small
- area:prolog
- conformance
closed: 2026-08-15
commits:
- hash: 9e88f078
  summary: fixtures pin lower.pl recursion throw surface; oracle twins the throws; census rows flip to reached
---

# Pin the unreached recursion refusal surface with conformance fixtures

## Description

Census (plans/2026-08-15-lowerpl-arm-census.REPORT.md, Real finding section): the recursion refusal surface in v6/prolog/lower.pl is entirely unreached by the 448-fixture corpus. Three throw sites — recursive_cte_multiple_self_reads (lower.pl:5205), built_text_in_recursive_head (lower.pl:5260), built_list_in_recursive_head (lower.pl:5264) — plus the two guard arms recursive_arm_builds_no_string / recursive_arm_builds_no_list. ARCH.pl:952 documents the direct spelling refused loudly while the two-rel spelling silently under-derives (PR #266 silent-wrong); nothing currently guards the refusal from regressing into silent wrongness. Fix: conformance fixtures using the existing throws(unsupported_construct(...)) pattern (fixtures/1_match_block.pl:110) that spell direct recursive text/list construction and assert each named throw, plus a compiling recursive fixture that reaches the two guard arms. Prove with arm_census: the five rows flip to reached.
