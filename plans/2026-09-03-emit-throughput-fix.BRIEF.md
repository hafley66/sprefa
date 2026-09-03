# brief: the 350k-row emission budget, red since #567

Lane: `fix/emit-throughput-567`. Base: `origin/main` (coordinator states the sha).
FIRST ACTION: `git merge --ff-only <sha>`. Failure = STOP AND REPORT.

## Contract

- `docs/failure-modes.md` entry 107 (incident, bisect, receipts).
- `tests/45_emit_throughput.rs`: 350,005-line generated `rows.go`, piped emission under `WALL_BUDGET_SECS = 5.5`; header records the post-#533 band 4.03 to 4.15 s.
- First bad commit `7bfc8d4a4` (#567): `src/lang/go.rs` gained `go_chain_of`, `go_chain_receiver_target`, `go_facts_of_path`, `go_field_type_in_dir`, `go_iface_fanout`, and `GoChainStep::{Field,Call}(String)` pushes per selector step, run during the per-file pass for every call site.

## Files you own

| file | change |
|---|---|
| `v6/sprefa-extract/src/lang/go.rs` | the chain walk runs only when a resolve leg will read it (project mode), or reads the CST slices without allocating (`&str` / `NameId` through `Strings`, the way `push_candidate` interns). Byte-identical output for every golden and every `tests/67_go_multihop.rs` case |
| `v6/sprefa-extract/tests/45_emit_throughput.rs` | entry 104's rail: read the 1-minute load average (`libc::getloadavg` or `sysctl vm.loadavg`), and when it exceeds `LOAD_SKIP_THRESHOLD` (pick 8.0 on this 10-core machine, state the reasoning) print `skipped: load N > threshold` and return; the header names the sha and band you re-measured |
| `docs/failure-modes.md` | entry 107's **Entry** line: the fix PR number and the three post-fix measurements |

Forbidden: `src/lang/{ts,rust,kotlin,python}*`, `src/project.rs`, `src/wire.rs`, `src/tsi/**`, `tests/fixtures/**`, `v6/tsv2/**`, `v6/prolog/**`, `v7/**`.

## Receipts required in the PR

| receipt | command | expected |
|---|---|---|
| fail-pre-fix | `cargo test -q --features cli --test 45_emit_throughput` on the base sha, 3x | `piped emission took 5.8-6.0 s`, rc=101, 3/3 |
| fixed | same on your branch, 3x | under 4.5 s, 3/3; paste all three numbers and `uptime` at each |
| multihop unchanged | `cargo test --features cli --test 67_go_multihop --test 63_go_inferred --test 66_go_iface_fanout --test 68_go_type_refs` | all pass |
| goldens | `cargo test --features cli --test golden_parity --test 1_resolve_cli` | byte-identical |
| count test | a COUNT receipt beside the wall one: the number of `String` allocations or chain walks over the 350k fixture before and after (a counter behind `cfg(test)` or a `tracing` field), so the fix is not end-state timing alone (CLAUDE.md: formerly-quadratic paths get COUNT tests) |
| full battery | `cargo test --features cli 2>&1 \| tail -3` | no failures |

## Style laws

- No `eprintln!`; `tracing` only.
- Comments: constraints only. No dates, no PR numbers in code comments.
- Banned words: provenance, substrate, load-bearing, regime, refusal, ground truth.
- No em dashes.
- N+1 law: never a per-site allocation where a slice or an interned id serves.

## Done

PR titled `extract: go chain walk off the emission path; emit budget under load skips by name`.
Then: `boop beep --no-wait --as <your-lane> sprefa-coordinator "emit-throughput PR #<n>: 3x <a>/<b>/<c> s, count <before> -> <after>, multihop unchanged"`.
