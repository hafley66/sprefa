---
created: 2026-08-16
updated: 2026-08-16
type: feature
status: open
priority: normal
epic: extract-port-closeout
labels:
- pkg:extract
- size:med
---

# kotlin type plane: edge candidates plus Resolve<TypeF>

## Description

## Description

Kotlin is the only roster language with a `Resolve<CallF>` arm and no
`Resolve<TypeF>` arm, and it emits no type-edge candidates, though v5's kotlin
front-end DOES emit `type_edge`.

## Receipts

| fact | receipt |
|---|---|
| kotlin has CallF resolve only | `v6/sprefa-extract/src/lang/kotlin.rs:1145`; no `impl Resolve<TypeF>` in the file |
| type dispatch skips kotlin, by comment | `v6/sprefa-extract/src/project.rs:463-478` |
| kotlin imports no `TypeEdgeCandidate` | `v6/sprefa-extract/src/lang/kotlin.rs:39-42` (go.rs:31-33 and rust.rs:36-40 both do) |
| the deferral, and that v5 emits | `v6/sprefa-extract/src/lang/kotlin.rs:27-31` |
| status table | `v6/sprefa-extract/src/types.rs:1833-1834` (kotlin DEFERRED, v5 emits field/impl/generic/variant) |
| v5 source | `src/graph/typegraph/kotlin.rs` (`kotlin_decl_edges`) |

## Fix shape

Mirror the go arm exactly, which is the closest twin (same front-end family,
same span story):
1. Port `kotlin_decl_edges` into `lang/kotlin.rs` as phase-1
   `TypeEdgeCandidate` rows on `TypeFAux.candidates`.
2. `impl Resolve<TypeF> for KotlinSource`, modeled on `lang/go.rs:1544`.
3. Wire `Some("kotlin")` into `project.rs:467-478` and delete the stale comment
   at `:463-466`.
4. Flip `src/types.rs:1833`.
5. Parity leg in `tests/golden_parity.rs` mirroring `type_edge_resolve_parity_go`.

## Gate

```bash
cd v6/sprefa-extract
cargo build --all-targets --features cli
cargo test --features cli
cargo test --features cli --test golden_parity
```
