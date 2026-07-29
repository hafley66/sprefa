# v6 prolog organization refactor journal

Contract: `plans/2026-07-29-prolog-org-review.md` (analysis at base
`15a25c08`). Execution base sha: `5a9bfdd8c36acff55a9e683d080bc3ede38d9418`.

Review staleness corrections applied before editing:

| Review claim | Measured at `5a9bfdd8` |
|---|---|
| 46 `.pl` files | 47 (`compile/scripts/dl6_oracle.pl` added by the runtime-bridge merge) |
| 15,028 lines | 15,159 |
| conformance 135/0 | 136/0 |
| plunit 74/74 | 75/75 |
| sweep 73 compiled / 70 identical | unchanged, 73/70/0 |

Every cited line number was re-located by predicate name rather than trusted
as a line reference.

---

## R10: `just prolog-lint` read-only gate

Landed. Files: `v6/prolog/tools/prolog_lint.pl`,
`v6/prolog/tools/prolog-lint.sh`,
`v6/prolog/tools/prolog-lint-baseline.txt`, `v6/justfile` recipe.

The review's section-6 8-step recipe is implemented as three SWI processes,
because two source clusters declare the same module name and a third file set
only exists once loaded:

| Phase | Goal | Steps covered |
|---|---|---|
| sources | `lint_sources` | 1 xref every file, 2 duplicate module names, 8 unused-export advisory |
| compile cluster | `lint_loaded(compile)` | 3 load `compile/test/plunit_tests.pl`, 4 `list_undefined`, 5 `list_cross_module_calls`, 6 `list_redefined` + `list_void_declarations` + `list_trivial_fails` + `list_format_errors` |
| example cluster | `lint_loaded(example)` | 7 fresh process over `examples/ghcacher.pl`, same checks |

`library(check)` reports through `print_message/2`, so findings are captured
with a `user:message_hook/3` that records the structured term and then fails,
leaving the human-readable line intact for interactive runs.

Two normalizations keep the baseline stable across unrelated edits:

- clause references and term positions are dropped; only
  `Module:Name/Arity` survives.
- a PLUnit caller reports as its unit name with the `@line N` suffix
  stripped, so inserting a test above another one does not churn the file.

Two defects found in the first draft of the tool and fixed before commit:

1. `ensure_loaded/1` called from inside the `prolog_lint` module pulled the
   module-free entry files' clauses into `prolog_lint`, which misattributed
   the `examples/ghcacher.pl` caller as `prolog_lint-report/0`. The load is
   now qualified `user:ensure_loaded/1` and the caller reports correctly as
   `user-report/0`.
2. Rendering parse errors through `message_to_text:message_to_codes/2` was
   itself an undefined private cross-module call, which the gate flagged
   against its own author. Parse errors now render via `term_to_atom/2`.

### Measured findings at base

Reproduces the review's numbers exactly:

| Class | Count |
|---|---:|
| duplicate module | 1 (`emit_ts`) |
| private cross-module call | 9 (8 compile cluster, 1 example cluster) |
| undefined predicate | 0 |
| redefinition | 0 |
| void declaration | 0 |
| trivial fail | 0 |
| format error | 0 |
| unused-export candidate (advisory) | 37 excluding the gate's own two entries |

### Ratchet posture

`prolog-lint-baseline.txt` was committed holding the 9 private call sites
only. The `emit_ts` duplicate module was deliberately left OUT of the
baseline, so the gate is RED at the R10 commit. That is the fail-first
receipt the R6 rename needs, since no existing gate in the battery can
detect the collision (the main battery never loads both emitters).

Receipt, at the R10 commit:

```
── NEW findings (gate failure) ──
FINDING	duplicate_module	emit_ts-compile/emit_ts.pl src/emit_ts.pl

PROLOG_LINT findings=10 baseline=9 FAIL
exit 1
```

The ratchet fails in both directions: a new finding fails, and a baseline
entry that disappears also fails with a prune instruction. That keeps the
baseline from rotting while R7 and R8 shrink it.
