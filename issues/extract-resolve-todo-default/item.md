---
created: 2026-08-16
updated: 2026-08-16
type: task
status: open
priority: high
epic: extract-port-closeout
labels:
- pkg:extract
- needs-chris
---

# Resolve<F> ships a todo!() default body

## Description

## needs-chris

Design decision, not implementation. No lane.

## Description

`Resolve<F>::resolve` ships a DEFAULT BODY that is `todo!()`. Every language
without an arm is one dispatch line away from a panic in a released binary, and
the guard against it is a hand-maintained match statement.

## Receipts

| fact | receipt |
|---|---|
| the default body | `v6/sprefa-extract/src/types.rs:1105-1109` — `todo!("4b-iii landed Resolve<TypeF> + 4c-ii Resolve<CallF> for TsSource; 4d landed both arms for RustSource; next: 4d go arms")` |
| the hand-maintained guard, and its own admission | `v6/sprefa-extract/src/project.rs:446-448` ("Explicit rather than blanket: the trait's default body is `todo!()`, so a source without an arm must never be dispatched") and `:463-466` |
| the guard already failed once | @extract-dl6-resolve-unwired: `DlSource` has both arms and neither is dispatched |
| the design freeze that put it there | `v6/sprefa-extract/src/types.rs:823-826` ("The trait surface + types only. Every method body is todo!(); NOTHING calls resolve; no impl exists yet ... Human review gates 4b") |

The freeze note's premise expired: seven `Resolve` impls exist and `project.rs`
calls them.

## Forks, decided by nobody

| fork | what it means | cost |
|---|---|---|
| A. default body returns `Vec::new()` | a lang with no arm resolves to nothing, quietly | silent zero rows, the exact shape of the dl6 bug |
| B. delete the default body | the trait becomes non-defaulted; every `Source` that names `Resolve` must implement it | compiler-enforced, but forces empty impls on langs with no plane |
| C. keep `todo!()`, add a rail | a test enumerating `sources()` against a declared arm table, so drift is caught at test time not at runtime | the rail is the work; the panic stays reachable if the rail is bypassed |
| D. split the trait | `Resolve` for langs that resolve, nothing for langs that do not; dispatch keys off a trait-object registry rather than a name match | biggest change, removes the match statement entirely |

Recommendation withheld. The `todo!()` in a shipped binary is the part that
needs a decision either way.
