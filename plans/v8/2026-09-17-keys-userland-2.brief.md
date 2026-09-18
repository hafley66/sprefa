# Keys userland 2 brief

Lane branch `feat/keys-userland-2`. Base `origin/main` at spawn (coordinator
states the sha). First action `git merge --ff-only <sha>`; failure = stop and
report. Preset opus.

Decision rows, read first: `AGENTS.md` "Kernel and prelude vocabulary" and
"Emitters are userland std modules" (grep `relation` in the decisions table).
Findings from lane 1: `plans/v8/2026-09-17-keys-userland.findings.md` stop one.

## Goal

`relation/3` leaves lower and check. Arity comes from the product's own `:`
edges and the `def`/`<-` rows. The value-position mode gate reads `return`
edges. Keys are not computed anywhere in Rust; `std/dl6.dl7` declares
`relation` as an interned node for the later emitter.

## Sites

| site | today | after |
|---|---|---|
| `src/_2_lower/_2_declare.rs:266-306` `constructor_relations` | mints `relation(Owner, Arity, KeySets)` from the `return` edge | deleted |
| `src/_2_lower/_2_declare.rs:153` | `relation(inner)` wrapper | keep only if it is not the `/3` row; state which |
| `src/_2_lower/_0_api.rs:65` | collects `relation/3` rows into `Lowered` | gone; `Lowered` loses the field or it is always empty, say which |
| `src/_2_lower/_12_units.rs:189` | reads `relation/3` owner | reads product owner from the `:` edges |
| `src/_2_lower/_8_express.rs:250-254` `cx.relations.get(&callable)` | arity from `relation/3` | arity = count of `:` edges whose owner is the callable (same walk as `:281-288` on the base) |
| `src/_2_lower/_8_express.rs:534` area, `ambiguous_expression_projection` | key sets from `relation/3` | the one `return` edge index; supplied positions are every other index |
| `src/_3_check/_0_api.rs:151-161` `relations_refs`, `relation_arities` | `basement.relations` | arity table built from `basement.edges` (owner, count) plus `def` rows for `<-` heads |
| `src/_3_check/_2_resolve.rs` | `cx.relations` | same table |
| `src/_3_check/_7_resolved.rs:24`, `_6_strata.rs:260` | read `relation/3` | read the arity table |
| `src/_3_check/_0_api.rs:170-192` `Checked.relations`, `strata_rows` | list of `relation/3` | `Checked.relations` stays as the runtime's table list, derived in check from the arity table with keys = every non-`return` position; one comment names it the `@std/dl6` seam |
| `src/_4_comptime/_3_assemble.rs:97,268` | rebuilds `relation/3` for derived rels | same derivation, one function shared with check |
| `src/_1_macrotime/_7_slice.rs:62`, `_2_protocol.rs:81` | read `relation/3` from the macro slice | read owner from `:` edges |
| `std/dl6.dl7` | absent | new: `(: relation (* (: rel type) (: arity int) (: keys any)))` interned, no rules yet |

NOT touched: `src/_3_check/_5_kernel.rs` (`kernel_relation_rows` stays and is
consumed as-is; lane `feat/str-primitive` and the later kernel-dot lane own
the file), `src/_9_runtime/**` (reads `Checked.relations`, unchanged shape),
`prelude/**`.

## Steps

| # | step | receipt |
|---|---|---|
| 1 | on base: `dl8 compile plans/v8/probes/2026-09-17-keys.dl7` rc + `cargo test --test _3_lower_oracle --test _8_compile_oracle` | pasted |
| 2 | arity table in check from edges + `def`; `relations_refs` deleted; `_8_compile_oracle` green with no regolden | PASS line |
| 3 | `_8_express.rs` callable arity from edge count; `ambiguous_expression_projection` from the `return` edge; `_3_lower_oracle` green | PASS line |
| 4 | delete `constructor_relations`; `Lowered` field; `_12_units.rs`, `_7_slice.rs`, `_2_protocol.rs` readers | build tail |
| 5 | `_3_assemble.rs` shares the derivation with check | `grep -rn "\"relation\"" src/_2_lower src/_3_check src/_4_comptime src/_1_macrotime` pasted; expected hits only `_5_kernel.rs` and the shared derivation |
| 6 | `std/dl6.dl7`; `dl8 compile` a probe importing `@std/dl6` rc=0 | rc |
| 7 | `bash oracle/refreeze.sh`; regolden alone in its own commit with the stat line | stat |
| 8 | full gate | summary |

Commit after every step. A step that needs `prelude/**` or `_5_kernel.rs` to
change is a stop-and-report row (`site : missing : fork`), not an edit.

## Gate

```bash
cargo test --no-fail-fast 2>&1 | grep -E "^test result|FAILED|panicked"
cargo test --test _3_lower_oracle --test _4_check_oracle --test _8_compile_oracle --test _21_openapi 2>&1 | grep -E "^test |test result"
grep -rn "\"relation\"" src/_2_lower src/_3_check src/_4_comptime src/_1_macrotime
git diff --stat origin/main...HEAD
```

Known red on the base: `.github/CI-KNOWN-RED.md` dl8 battery section. Measure
each red leg three times.

## Laws

`plans/v8/2026-09-17-import-arc.brief.md` section 8. No long-form markdown;
the PR body is the steps table with receipts, under 20 lines of prose.

## Report

```bash
boop beep --no-wait --as feat-keys-userland-2 sprefa-coordinator "keys 2: PR #<n>, relation/3 hits <n>, battery <pass>/<total>, red: <list or none>"
```
