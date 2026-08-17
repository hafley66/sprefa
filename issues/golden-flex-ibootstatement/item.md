---
created: 2026-08-14
updated: 2026-08-14
type: bug
reporter: fable
status: obsolete
priority: normal
assignee: fable
closed: 2026-08-14
---

# golden-flex IBootStatement structural identity mismatch between local declaration and runtime import

_Source: v6/prolog/emit_ts.pl_

## Description

gen_emitted/golden-flex.ts:4294 fails TS2345: readonly IBootStatement[] not assignable to readonly import("…/v6/tsv2/runtime/types").IBootStatement[]. The generated file declares its own IBootStatement instead of using the runtime import, and the two identities diverged. Repro: cd v6 && bash tsv2/scripts/sweep.sh (fresh gen_emitted), then just typecheck. One of the 2 errors left after PR #259 (kind union widening).

## Comments

### 2026-08-15T03:18:27Z · @fable

Same root cause: stale Aug-11 golden-flex.ts. Regeneration clears the TS2345; current emitter imports IBootStatement from the runtime.
