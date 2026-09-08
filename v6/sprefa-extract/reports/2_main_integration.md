# Current-main extract integration

Date: 2026-09-08

Branch: `fix/extract-main-integration`

Current-main base: `b9b7c0560a87cdfb0dc55985f71ed5ba69619715`

Substantive integration commit: `b490261b1b6552e21842b7416a0112c1a992019f`

## Selected commit mapping

The selected changes were applied oldest first with `git cherry-pick --no-commit`, then committed together after semantic conflict resolution and the required single formatter run. Each source commit maps to integration commit `b490261b1b6552e21842b7416a0112c1a992019f`.

| Source commit | Source subject | Current-main integration |
| --- | --- | --- |
| `5fa36e78046efcef47d8e37c8f6fad8a8d648fb4` | `extract: PASS - pin fast and slow aliases and expose build identity` | `b490261b1b6552e21842b7416a0112c1a992019f` |
| `aaf14bcba45f5cc5e73230c11bf9a41280e6659d` | `v6/extract: use shared warn-default CLI telemetry` | `b490261b1b6552e21842b7416a0112c1a992019f` |
| `7592bb5db0ab04c07be09221aa1ca6f073a68f92` | `v6/extract: retain bounded indexer failure evidence` | `b490261b1b6552e21842b7416a0112c1a992019f` |
| `a8008ebb8abf9fdcb889e3f2625e30f1f4fcb5dd` | `v6/extract: record Falcon slow reproduction` | `b490261b1b6552e21842b7416a0112c1a992019f` |
| `a43816d77b2caed258afe48ae8d14cda04f90bd1` | `v6/extract: FAIL nested Cargo staging topology` | `b490261b1b6552e21842b7416a0112c1a992019f` |
| `044f71bfe010c887d3bdce9ae5d858a25d03785a` | `v6/extract: PASS nested Cargo staging topology` | `b490261b1b6552e21842b7416a0112c1a992019f` |
| `d83d9cc919a8d6f18fd469b621b889ec7dfd83e9` | `v6/extract: record Falcon staging green` | `b490261b1b6552e21842b7416a0112c1a992019f` |
| `7d6272bbcfde8eebaac8c3ee213ff2ec60888955` | `v6/extract: integrate pending source and SCIP fixes` | `b490261b1b6552e21842b7416a0112c1a992019f` |
| `5eca5163accc3a7054c2d057b9c6d2a801b70adb` | `v6/extract: isolate re-export SCIP fixture` | `b490261b1b6552e21842b7416a0112c1a992019f` |
| `c58c33fec0efc528d67de297da48632a97ae588f` | `v6/extract: record combined integration evidence` | `b490261b1b6552e21842b7416a0112c1a992019f` |
| `c8865c7303932d6f789cc8c33c70317cf9d55802` | `v6/extract: keep ingest stdin out of file existence checks` | `b490261b1b6552e21842b7416a0112c1a992019f` |
| `001eeeb9e53a423ed271f85439aad02a48c52efc` | `v6/extract: record final combined integration gate` | `b490261b1b6552e21842b7416a0112c1a992019f` |

The transplanted `reports/0_falcon_slow_reproduction.md` and `reports/1_combined_integration.md` retain their original branch receipts. Their results describe the earlier `fix/extract-final-gate` lineage and are not evidence for this current-main integration. The current-main evidence is recorded below.

## Semantic conflict resolution

The only textual cherry-pick conflict was the `5fa36e780` CLI change in `src/bin/extract.rs`. Resolution retained current main's command structure and added the `fast` and `slow` aliases and build identity output.

Alias validation now includes current main's `--go-checker` and `--indexer` flags. Both aliases reject explicit family, SCIP index/build, Rust checker, TypeScript checker, and Go checker selections. `fast` also rejects a named indexer because it selects the diet SCIP path. `slow --indexer <name>` remains accepted, reaches current main's named-indexer dispatch, and uses the selected indexer's separate cache path. Tests compare the alias result with the equivalent explicit family invocation.

Current main's Go checker, Kotlin and Python module facts, SCIP relationship conformance, named-indexer selection, and selected-indexer cache separation were retained. Rust `pub use` occurrences are discriminated from calls while current main's module and relationship logic remains active. Missing generated files in a readable exact-file source are skipped during extraction. `/dev/stdin` remains valid for `--ingest` and bypasses ordinary file-existence validation.

The first full compilation exposed six stale current-main `ResolveRequest` initializers missing `go_checker: None`; those initializers were updated without changing resolver behavior. The second full gate exposed an assertion in `79_rust_generic_args` that predated current main's impl self-type head edge. The test now expects both the generic argument edge and the current `Boxed -> Boxed` head edge. The extractor semantics were not changed for this failure.

Source files differed in formatter output between the two lineages. `cargo fmt` was run once immediately before the substantive Rust commit, and all resulting formatting is included in `b490261b1`. No formatter was run after that commit.

## Tests

All test commands ran from `v6/sprefa-extract` with the dedicated target directory `/tmp/sprefa-extract-main-integration-target.LCsjvv`.

A focused regression set covered CLI aliases and build metadata, shared telemetry, source-tree missing-file handling, SCIP families and re-export discrimination, freshness and Cargo staging, named-indexer selection and cache paths, Rust checker behavior, SCIP relationship conformance, and the Go checker tier. It completed with 79 passed, 0 failed, and 0 ignored. The platform-gated Rust checker files compiled but registered zero active tests on this host; the real rust-analyzer discrimination test in `8_scip_families_cli` ran and passed.

Three full `cargo test --features cli --no-fail-fast` attempts were used:

1. The first attempt stopped at compilation on the stale `ResolveRequest` initializers described above.
2. The second attempt compiled the full suite and had one failing stale assertion in `79_rust_generic_args`; its focused rerun passed 3 tests after the expectation was aligned with current main.
3. The final post-fix gate ran against committed code at `b490261b1` and completed across 171 test targets with **897 passed, 0 failed, and 16 ignored**.

The final gate log is `/tmp/sprefa-extract-main-integration-full3.log`, SHA-256 `bf4dd3f66b5c846a04525b90bc1078d837ca03846f69dffa502c74fae7616a56`.

The final gate includes current main's named-indexer, selected-cache, Go checker, Kotlin/Python module, and SCIP conformance tests. Added or changed coverage exercises alias equivalence and flag conflicts, build identity, warn-default and structured telemetry, bounded indexer failure evidence, nested Cargo staging topology, missing generated source files, Rust re-export discrimination, and stdin ingest. No tests or CI execution paths were removed. No optional language-tooling failure blocked the final gate.

## Falcon current-main reproduction

Source root:

`/Users/chrishafley/projects/hafley-rs/games/blender-godot-sqlite-proof/falcon-lab`

Binary:

`/tmp/sprefa-extract-main-integration-target.LCsjvv/debug/extract`

Identity:

```text
extract 0.1.0
git hash: b490261b1b65
datetime: 2026-09-08T18:19:38Z
rust-analyzer 1.100.0-nightly (17fd5b8a3 2026-08-28)
```

Task scratch and cache:

```text
/tmp/sprefa-extract-falcon-main.sGfHD7
/tmp/sprefa-extract-falcon-main.sGfHD7/scip-cache
```

Both runs used:

```sh
CARGO_BUILD_JOBS=2 \
CARGO_TARGET_DIR=/tmp/sprefa-extract-falcon-main.sGfHD7/cargo-target \
DL_TRAIL=0 \
RUST_LOG=sprefa_extract=debug \
HAFLEY_LOG_FORMAT=json \
/tmp/sprefa-extract-main-integration-target.LCsjvv/debug/extract \
  slow \
  --scip-timeout 45 \
  --scip-cache /tmp/sprefa-extract-falcon-main.sGfHD7/scip-cache \
  /Users/chrishafley/projects/hafley-rs/games/blender-godot-sqlite-proof/falcon-lab
```

| Receipt | Cold run | Warm run |
| --- | ---: | ---: |
| Exit | 0 | 0 |
| Wall time | 33.85 s | 0.06 s |
| User time | 46.91 s | 0.05 s |
| System time | 7.70 s | 0.00 s |
| Semantic rows | 2,583 | 2,583 |
| stdout bytes | 405,215 | 405,214 |
| stderr lines | 3 | 1 |
| `scip_skip` rows | 0 | 0 |

Both streams contained the same semantic row census:

| Record | Rows |
| --- | ---: |
| `scip_callee_type` | 39 |
| `scip_def` | 291 |
| `scip_edge` | 80 |
| `scip_fn_edge` | 738 |
| `scip_index` | 1 |
| `scip_local` | 670 |
| `scip_name` | 290 |
| `scip_ref` | 474 |

The cold run created an 853,222-byte `index.scip`. Its stderr contained structured JSON events for `indexer process completed` and span closure. The process event recorded `rust-analyzer scip . --output <cache>/index.scip`, repository-shaped staging cwd, exit code 0, outcome `success`, timeout 45,000 ms, pid 71998, and duration 31,752 ms. The third stderr line was the CLI cache-location receipt. The warm run emitted only that cache-location line and did not spawn the indexer.

The raw stdout SHA-256 values differ only because the `scip_index.reused` field changes from cold to warm:

```text
cold stdout: 88a8541abc23cd020e387f8f4e66de3d77910887d44b1d105ec9ee77b2020639
warm stdout: 9cbd9e64f3ff192d84052b009f7d48031a24487c8274ca3120a68af69b389e02
cold stderr: dc8f2137fe67b82008b926150299d72363bbcbf21bd8d856bb37ed55e8761ade
warm stderr: 1ddb6c8e42976f3eae7d7743e731b7f14c292765c15b2807e34228739e53c454
```

After normalizing `scip_index.reused=false`, the output files compare byte-for-byte and both have SHA-256 `88a8541abc23cd020e387f8f4e66de3d77910887d44b1d105ec9ee77b2020639`.

The source inventory covered 16,257 files and 4.2 GB, including an existing 4.0 GB `target` directory. Before and after SHA-1 inventories compare equal, and both inventory files have SHA-256 `90a09ef2c5346dd5629613d8450bf1134783d9374f4c973b71472e64a18ad2cb`. Source checkout status also compares equal. The pre-existing source status was:

```text
 M games/blender-godot-sqlite-proof/falcon-lab/contracts/0_presentation.tsp
 M games/blender-godot-sqlite-proof/falcon-lab/contracts/1_generate.mjs
?? games/blender-godot-sqlite-proof/falcon-lab/contracts/1_ui_emit.mjs
```

The reproduction did not change source bytes or source checkout status.

## Merge readiness

No in-scope or external blocker remains. The substantive code is at `b490261b1`, the final full gate is green, the Falcon cold and warm reproductions exit successfully with zero `scip_skip` rows, and the worktree is ready for the parent lane to sample and merge.
