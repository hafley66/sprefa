# IR shrink census

Inputs: `tests/fixtures/15_standard_plus.dl7` and `fixtures/comptime_effect/0_fs_json.dl7` (`fs_json`). Counts use each compile output's `program` field. Install times are one `E::install` wall sample in nanoseconds. SQLite CREATE count is the number of entries with `CREATE %` SQL in `sqlite_master` plus `sqlite_temp_master` after evaluation.

Commands: `$CARGO_TARGET_DIR/debug/dl8 compile <input> > <case>.compile.json`; `$CARGO_TARGET_DIR/debug/dl8 emit ir-json <case>.compile.json > <case>.ir.json`; `jq -r '.program.nodes | map(keys[0]) | group_by(.) | map("\(.[0])=\(length)") | join(" ")' <case>.ir.json`; `RUST_LOG=dl8::ir=info DL8_ENGINE=<engine> $CARGO_TARGET_DIR/debug/dl8 eval <case>.compile.json` (read `install_ns` and `sqlite_create_count` from stderr). Both eval commands exited 0 for both cases.

| Commit | Case | Nodes by op | Total | SQLite CREATE | ir:dd install | ir:sqlite install |
|---|---|---|---:|---:|---:|---:|
| 0 baseline | standard_plus | Antijoin=160 Get=1590 Join=1042 Mfp=891 Mint=219 Reduce=7 Threshold=148 Union=188 | 4245 | 6113 | 195260834 ns | 1051237542 ns |
| 0 baseline | fs_json | Antijoin=160 Get=1596 Join=1044 Mfp=893 Mint=220 Reduce=7 Threshold=149 Union=189 | 4258 | 6133 | 198478291 ns | 1089195875 ns |
| 1 scaffolding | standard_plus | Antijoin=160 Get=1590 Join=794 Mfp=891 Mint=219 Reduce=7 Threshold=140 Union=180 | 3981 | 5583 | 166270709 ns | 830623083 ns |
| 1 scaffolding | fs_json | Antijoin=160 Get=1596 Join=796 Mfp=893 Mint=220 Reduce=7 Threshold=140 Union=180 | 3992 | 5600 | 156593459 ns | 850203042 ns |
| 2 effects (notes only) | standard_plus | Antijoin=160 Get=1590 Join=794 Mfp=891 Mint=219 Reduce=7 Threshold=140 Union=180 | 3981 | 5583 | 172388541 ns | 843551458 ns |
| 2 effects (notes only) | fs_json | Antijoin=160 Get=1596 Join=796 Mfp=893 Mint=220 Reduce=7 Threshold=140 Union=180 | 3992 | 5600 | 146958000 ns | 845128917 ns |
| 3 fused cons (notes only) | standard_plus | Antijoin=160 Get=1590 Join=794 Mfp=891 Mint=219 Reduce=7 Threshold=140 Union=180 | 3981 | 5583 | 184271625 ns | 885468083 ns |
| 3 fused cons (notes only) | fs_json | Antijoin=160 Get=1596 Join=796 Mfp=893 Mint=220 Reduce=7 Threshold=140 Union=180 | 3992 | 5600 | 168689958 ns | 935981375 ns |

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

Open items for commit 1:

| Item | Evidence |
|---|---|
| Constructor installers | Removing shapes absent from the initial program makes a later `Event(const(1))` insert fail on both IR engines with `Unsupported("dynamic term constructor")`; baseline derives `Action(const(1))`. |
| `__ir_all_sources` | Removing the sentinel changes `_9_ladder::rung_10_continuing_executors` from 2 expected edit writes to 3 on `ir:dd`; restoring it passes. |
| `nil` seed | Removing it when no rule reads `nil` drops an observable row in `_9_ladder::rung_11_handwritten_oracle_constructs`; restoring it passes. |
| Seed source path | A relation with no initial seed can receive later frontier inserts. Dropping its source Get would discard those rows. |

Validation (commit 1):

| Command | Result |
|---|---|
| `cargo test --workspace` | 6 library and 147 integration tests passed; 0 failed or ignored |
| `DL8_ENGINE=ir:dd cargo test --test it _9_ladder::` | 17 passed; 0 failed or ignored |
| `DL8_ENGINE=ir:sqlite cargo test --test it _9_ladder::` | 17 passed; 0 failed or ignored |
| `DL8_ENGINE=ir:dd cargo test --test it _1_compile_oracles::` | 11 passed; 0 failed or ignored |
| `DL8_ENGINE=ir:sqlite cargo test --test it _2_eval_oracles::ir_engines_match_v7_oracles -- --exact` | 1 passed; 0 failed or ignored |
| `cargo test --manifest-path sqlite_ivm/Cargo.toml --config 'profile.dev.opt-level=1' --config 'profile.dev.package."*".opt-level=3' -p ivm-dd --test 2_random` | 6 passed; 0 failed or ignored |
| `cargo test --manifest-path sqlite_ivm/Cargo.toml --config 'profile.dev.opt-level=1' --config 'profile.dev.package."*".opt-level=3' -p ivm-sqlite --test 2_random` | 8 passed; 0 failed; 7 pre-existing ignored worker tests |
| `DL8_ENGINE=ir:sqlite dl8 compile <each committed oracle/compile case>` (900-second cap per case, compare committed expected fields and exit code) | 15/16 passed and matched; `project-tsi-contract` timed out at 900.01 s, as on baseline. `test-fixtures-5_curry` passed in 484.51 s. |

Commit 2 dropped item: replacing the shared `effect(rel, app_id)` rows with only `R_req` rows removes rows that `_9_ladder::rung_8_effects` currently asserts. At size 3, the rung requires 3 visible `effect` rows on every engine. Deleting the table would yield 0 unless legacy rows were synthesized, which would retain the removed app-id representation. No effect implementation was changed.

Validation (commit 2, notes only):

| Command | Result |
|---|---|
| `cargo test --workspace` | 6 library and 147 integration tests passed; 0 failed or ignored |
| `DL8_ENGINE=ir:dd cargo test --test it _9_ladder::` | 17 passed; 0 failed or ignored |
| `DL8_ENGINE=ir:sqlite cargo test --test it _9_ladder::` | 17 passed; 0 failed or ignored |
| `DL8_ENGINE=ir:dd cargo test --test it _1_compile_oracles::` | 11 passed; 0 failed or ignored |
| `DL8_ENGINE=ir:sqlite cargo test --test it _2_eval_oracles::ir_engines_match_v7_oracles -- --exact` | 1 passed; 0 failed or ignored |
| `cargo test --manifest-path sqlite_ivm/Cargo.toml --config 'profile.dev.opt-level=1' --config 'profile.dev.package."*".opt-level=3' -p ivm-dd --test 2_random` | 6 passed; 0 failed or ignored |
| `cargo test --manifest-path sqlite_ivm/Cargo.toml --config 'profile.dev.opt-level=1' --config 'profile.dev.package."*".opt-level=3' -p ivm-sqlite --test 2_random` | 8 passed; 0 failed; 7 pre-existing ignored worker tests |
| `DL8_ENGINE=ir:sqlite dl8 compile <each committed oracle/compile case>` (900-second cap per case, compare committed expected fields and exit code) | 15/16 passed and matched; `project-tsi-contract` timed out at 900.01 s; `test-fixtures-5_curry` passed in 495.54 s |

Commit 3 dropped item: `intern_row` constructs `ref(application(Name, plain_list))`, and a real `cons` chain can construct that same `plain_list` at runtime. A k-ary Mint of `Name` would decode as `Name(arg1, ..., argk)` instead. Those compounds have different arity and functor structure; `Universe::cmp` compares arity, name, and arguments. Both representations could coexist for the same application and compare unequal. No fusion was applied.

Validation (commit 3, notes only):

| Command | Result |
|---|---|
| `cargo test --workspace` | 6 library and 147 integration tests passed; 0 failed or ignored |
| `DL8_ENGINE=ir:dd cargo test --test it _9_ladder::` | 17 passed; 0 failed or ignored |
| `DL8_ENGINE=ir:sqlite cargo test --test it _9_ladder::` | 17 passed; 0 failed or ignored |
| `DL8_ENGINE=ir:dd cargo test --test it _1_compile_oracles::` | 11 passed; 0 failed or ignored |
| `DL8_ENGINE=ir:sqlite cargo test --test it _2_eval_oracles::ir_engines_match_v7_oracles -- --exact` | 1 passed; 0 failed or ignored |
| `cargo test --manifest-path sqlite_ivm/Cargo.toml --config 'profile.dev.opt-level=1' --config 'profile.dev.package."*".opt-level=3' -p ivm-dd --test 2_random` | 6 passed; 0 failed or ignored |
| `cargo test --manifest-path sqlite_ivm/Cargo.toml --config 'profile.dev.opt-level=1' --config 'profile.dev.package."*".opt-level=3' -p ivm-sqlite --test 2_random` | 8 passed; 0 failed; 7 pre-existing ignored worker tests |
| `DL8_ENGINE=ir:sqlite dl8 compile <each committed oracle/compile case>` (900-second cap per case, compare committed expected fields and exit code) | 15/16 passed and matched; `project-tsi-contract` timed out at 900.02 s; `test-fixtures-5_curry` passed in 503.80 s |
