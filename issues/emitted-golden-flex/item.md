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

# emitted golden-flex types a literal array slot IRowValueArray where the target is scalars-only

_Source: v6/prolog/emit_ts.pl_

## Description

gen_emitted/golden-flex.ts:70 fails TS2322: (string | number | bigint | IRowValueArray)[] not assignable to (string | number | bigint)[]. Emitter gap from the PR #256 IRowScalar split: the emitter types a literal array slot as the full row value where the receiving signature is scalars-only. Repro: cd v6 && bash tsv2/scripts/sweep.sh (fresh gen_emitted), then just typecheck. One of the 2 errors left after PR #259 (kind union widening).

## Comments

### 2026-08-15T03:18:27Z · @fable

Stale generated artifact: gen_emitted/golden-flex.ts was Aug-11 emitter output. Regenerating via compile_dl6.sh clears the TS2322; current emitter already types bind_args with the scalar guard. Real finding filed separately: the golden-flex coverage gate blocks regeneration.
