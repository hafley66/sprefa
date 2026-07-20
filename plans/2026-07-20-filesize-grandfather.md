# Filesize rail: grandfathering 29 files (2026-07-20)

Ruling requested in the standing ledger under "filesize-rail ruling". Decision:
grandfather, do not block the 0.11.0 release on a 29-file split.

## Receipt

`scripts/filesize-rail.sh` exited 2 with 29 `src/**` files over the 500-line
hard budget and absent from `scripts/filesize-allow.txt`. Every one of them was
already over budget at pushed main `a3c09e3f`. None crossed the budget in the
arcs that produced 0.11.0, so the rail was reporting accumulated debt rather
than a regression.

After grandfathering: exit 0, 56 files grandfathered over 500, 42 in the
300-500 needs-a-reason band.

## The 29

| file | lines |
|---|---|
| `src/storage/call.rs` | 1938 |
| `src/graph/typegraph/ts/mod.rs` | 1467 |
| `src/graph/typegraph/python.rs` | 1384 |
| `src/graph/typegraph/rust/mod.rs` | 1332 |
| `src/engine/extract/mod.rs` | 1327 |
| `src/graph/typegraph/go.rs` | 1217 |
| `src/graph/modgraph/mod.rs` | 1211 |
| `src/engine/declare.rs` | 1167 |
| `src/engine/source_prepare.rs` | 1127 |
| `src/engine/eval.rs` | 1116 |
| `src/engine/typed_plan.rs` | 1103 |
| `src/graph/typegraph/kotlin.rs` | 1042 |
| `src/engine/family/mod.rs` | 989 |
| `src/graph/typegraph/ts/flow.rs` | 964 |
| `src/graph/typegraph/mod.rs` | 761 |
| `src/hook.rs` | 758 |
| `src/engine/pipeline/source_stage.rs` | 756 |
| `src/jobq/mod.rs` | 737 |
| `src/engine/extract/call_render_tests.rs` | 669 |
| `src/engine/symbols.rs` | 665 |
| `src/engine/deltaflow.rs` | 662 |
| `src/engine/strata.rs` | 593 |
| `src/jobq/tests.rs` | 577 |
| `src/cli/mod.rs` | 568 |
| `src/rspath.rs` | 510 |
| `src/engine/desugar.rs` | 511 |
| `src/graph/modgraph/rust.rs` | 505 |
| `src/engine/cold_stage.rs` | 503 |
| `src/engine/rpc.rs` | 501 |

## Shrink-only law

The allowlist may lose entries, never gain them. A file that drops under 500
lines is removed from both `scripts/filesize-allow.txt` and the `big_file_ok`
table in `.dl/file-size.dl`. A NEW file crossing 500 fails the rail and gets
the STOP protocol (propose 3 splits or 1 justification), same as before.

Both lists were updated together; `.dl/file-size.dl` carries the same 29 rows
so the advisory in-editor rail names the same set as the exit-2 bash prong.

## Natural shrink candidates

Not scheduled here, recorded so the ratchet has somewhere to go. The four
largest are single-language extractors (`ts/mod.rs`, `python.rs`,
`rust/mod.rs`, `go.rs`) that split cleanly by construct family, and
`src/storage/call.rs` at 1938 is the single largest file in the tree.
`plans/2026-07-11-engine-mod-split.md` already owns the `src/engine/**`
trait-extraction shrink path for the earlier grandfathered set.
