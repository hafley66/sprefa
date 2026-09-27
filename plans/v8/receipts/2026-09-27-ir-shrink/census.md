# IR shrink census

Inputs: `tests/fixtures/15_standard_plus.dl7` and `fixtures/comptime_effect/0_fs_json.dl7` (`fs_json`). Counts use each compile output's `program` field. Install times are one `E::install` wall sample in nanoseconds. SQLite CREATE count is the number of entries with `CREATE %` SQL in `sqlite_master` plus `sqlite_temp_master` after evaluation.

Commands: `$CARGO_TARGET_DIR/debug/dl8 compile <input> > <case>.compile.json`; `$CARGO_TARGET_DIR/debug/dl8 emit ir-json <case>.compile.json > <case>.ir.json`; `jq -r '.program.nodes | map(keys[0]) | group_by(.) | map("\(.[0])=\(length)") | join(" ")' <case>.ir.json`; `RUST_LOG=dl8::ir=info DL8_ENGINE=<engine> $CARGO_TARGET_DIR/debug/dl8 eval <case>.compile.json` (read `install_ns` and `sqlite_create_count` from stderr). Both eval commands exited 0 for both cases.

| Commit | Case | Nodes by op | Total | SQLite CREATE | ir:dd install | ir:sqlite install |
|---|---|---|---:|---:|---:|---:|
| 0 baseline | standard_plus | Antijoin=160 Get=1590 Join=1042 Mfp=891 Mint=219 Reduce=7 Threshold=148 Union=188 | 4245 | 6113 | 195260834 ns | 1051237542 ns |
| 0 baseline | fs_json | Antijoin=160 Get=1596 Join=1044 Mfp=893 Mint=220 Reduce=7 Threshold=149 Union=189 | 4258 | 6133 | 198478291 ns | 1089195875 ns |

Validation (commit 0):

| Command | Result |
|---|---|
| `cargo test --workspace` | 6 library and 147 integration tests passed; 0 failed or ignored |
| `DL8_ENGINE=ir:dd cargo test --test it _9_ladder::` | 17 passed; 0 failed or ignored |
| `DL8_ENGINE=ir:sqlite cargo test --test it _9_ladder::` | 17 passed; 0 failed or ignored |
| `DL8_ENGINE=ir:dd cargo test --test it _1_compile_oracles::` | 11 passed; 0 failed or ignored |
| `DL8_ENGINE=ir:sqlite cargo test --test it _1_compile_oracles::` | Stopped after 15 minutes in `every_committed_case_matches_v7`, while compiling `tsi_project/0_contract.dl7`; no result |

The `ir:sqlite` eval oracle filter and the `ivm-*` random suites were not run before commit 0. Parent review confirmed the `tsi_project/0_contract.dl7` timeout on base `1a3faa916`. For commits 1-3, each `ir:sqlite` compile oracle case has a 15-minute cap: baseline completions must still finish and match; baseline timeouts are reported with their elapsed time.

CI coverage: unchanged.
