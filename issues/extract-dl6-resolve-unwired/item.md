---
created: 2026-08-16
updated: 2026-08-16
type: bug
status: fixed
priority: high
epic: extract-port-closeout
labels:
- pkg:extract
- size:small
closed: 2026-08-16
---

# dl6 phase-2 arms exist and are never dispatched

## Description

## Description

`DlSource` implements both phase-2 arms, and `--resolve` over `.dl6` files
reaches neither. The dispatch in `project.rs` matches on `Source::name()` and
has no `"dl6"` arm, so every `.dl6` input silently yields zero
`resolved_edge` / `resolved_type_edge` rows.

## Receipts

| fact | receipt |
|---|---|
| `Resolve<CallF> for DlSource` exists | `v6/sprefa-extract/src/lang/dl6/_0_source.rs:425` |
| `Resolve<TypeF> for DlSource` exists | `v6/sprefa-extract/src/lang/dl6/_0_source.rs:449` |
| `Source::name()` is `"dl6"` | `v6/sprefa-extract/src/lang/dl6/_0_source.rs:389-391` |
| call dispatch has no dl6 arm | `v6/sprefa-extract/src/project.rs:449-462` (ts, rust, go, kotlin, prolog) |
| type dispatch has no dl6 arm | `v6/sprefa-extract/src/project.rs:467-478` (ts, rust, go) |
| the arm is only ever called directly from a test | `v6/sprefa-extract/tests/0_dl6.rs:143` |
| nothing catches roster drift | `tests/4_capability_parity.rs` legs cover default output + capability enum, not per-lang phase-2 dispatch |

## Fix shape

1. Add `Some("dl6") => Resolve::<CallF>::resolve(&DlSource, output, cx)` and the
   `TypeF` twin to the two matches in `project.rs`.
2. Add a rail so the next arm cannot land unreachable: a test that, for every
   `Source` in `sources()`, asserts the dispatch decision is DECLARED rather
   than defaulted. Cheapest honest form: a `const RESOLVE_ARMS: &[(&str, bool,
   bool)]` table checked in both directions against `sources()`, with the
   dispatch matches reading the same table.
3. A CLI-level test in `tests/1_resolve_cli.rs`: two `.dl6` fixtures where one
   rel references the other, `extract --resolve a.dl6 b.dl6 --family call,type`
   emits at least one `resolved_edge` and one `resolved_type_edge`.

## Gate

```bash
cd v6/sprefa-extract
cargo build --all-targets --features cli
cargo test --features cli            # twice, read rc explicitly
```

## Comments

### 2026-08-16T17:29:31Z · @extract-closeout-driver

PR #302, gate green twice. Found a second defect underneath: the dl6 type projection read field(inner,"columns") and the grammar binds every column to that same field name, so only the first column ever minted a sig or candidate. Both fixed, plus a RESOLVE_ARMS table so dispatch is a lookup and a new Source cannot land unreachable.
