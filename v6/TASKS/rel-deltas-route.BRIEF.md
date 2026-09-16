# Brief: rel deltas route + resident-coroutine fixture (phase 1a)

Plan: `/Users/chrishafley/projects/sprefa/plans/2026-08-18-boop-resident-coroutine.md` (absolute path, main tree; a copy also sits in your worktree at `plans/`). Read it first, whole. `v6/sprefa-engine-rs/Cargo.toml` is added to your owned files (one dep at most, say which and why).

## Base
Your branch starts at 79e6faa5c (also kept as `wip/rel-deltas-route-opus`) = origin/main 2d3c8891d + ONE unverified WIP commit from an earlier lane that died mid-way (serve.rs deltas route partly written, fixture, conformance file, golden test, snapshot). FIRST action: `git log -2` shows 79e6faa5c on top of 2d3c8891d, else STOP AND REPORT. Read `git show --stat HEAD` and the diff; keep what is right, fix or delete what is not; nothing in it has been run. Squash or amend as you like; the PR is judged on its final diff. NEVER `git stash`. Never spawn subagents. Read `CLAUDE.md` standing laws and style laws.

## Deliverables

### 1. `GET /rel/{name}/deltas?since=<tick>` on the UDS server
`v6/sprefa-engine-rs/src/serve.rs` (routes at `:242-246`, `ServeState` `:86`, `read_rel` `:111`). Long-poll: if the rel has deltas after tick `since`, return immediately; else wait for the next tick (bounded, 30s, then return empty). Response JSON: `{"tick": <int>, "add": [[..values..]], "del": [[..values..]]}` with the rel's columns as `GET /rel/{name}` returns them. Deltas come from the same `TickDeltas` the host collector reads (`hosts.rs:1840`); keep a bounded ring of per-tick deltas per rel in `ServeState` (say 256 ticks) and say the bound in a comment. Test in `tests/serve_uds.rs` beside `boot_on_socket`: arrive two rows, read deltas since 0 -> both in `add`; arrive one del -> `del` has it; `since` past the ring -> error named. Existing serve tests unchanged.

### 2. Fixture `v6/dl/fixtures/resident-coroutine.dl6`
The program in the plan, section 3, verbatim modulo what the compiler forces (if a rule fails, cite the throw site in the PR and adjust minimally; do NOT add `sh` or `bind`). `resident` is a base rel with an arrow and no rules; `turn` is a base rel. Compile: `swipl -q -l v6/prolog/compile.pl -l v6/prolog/emit_rust.pl -g "compile_dl6('v6/dl/fixtures/resident-coroutine.dl6', '<out>.program.rs', [emitter(emit_rust:emit_program)])" -g halt` (see `tests/15_source_mutation_hosts.rs:14-18`).

### 3. Conformance fixture `v6/prolog/conformance/fixtures/resident_coroutine.pl`
Copy the shape of a neighbour (e.g. `temporal_pipe.pl`). Arrivals into `turn`: session s, turns 1..6 with roles user,assistant,assistant,user,user,assistant and short `said` texts. Expected: `run` has 4 rows (turn1 user; turns 2-3 assistant with `group_concat`; turns 4-5 user; turn 6 assistant), `bundle` has 1 row (ai_run=2, user_run=4), `resident_ask` 1 row. Then arrive `resident(s, 4, 9, 'reply')` -> `handled(s,4)` 1 row. Run: `cd v6/prolog/conformance && swipl -g go -t halt go.pl` (whole thing once, ~seconds; while iterating run only your fixture if go.pl supports a filter; check its head).

### 4. Rust golden `v6/sprefa-engine-rs/tests/17_resident_coroutine.rs`
Same arrivals through `run_schedule` (pattern: `tests/diverging_recursion.rs`), asserting the same counts and the `group_concat` text, then arrive the `resident` row and assert `handled`. Snapshot `tests/fixtures/resident-coroutine.program.rs` from the recipe in 2.

### 5. Clock boundary row
`v6/prolog/3_clock_check.pl`: add `clock_boundary(Program, not_provable(externally_fed(Ref)))` for a base rel (no rule head) that at least one rule reads and no arrival/boot writes in-program: stated like `multi_trigger_batch_invariance` at `:357`, NEVER a refusal. plunit test beside the clock tests in `compile/test/plunit_tests.pl` (grep `clock_boundary`), fail-first header. Verify no fixture in `compile/out/manifest.json` changes bucket (run the sweep once, `cd v6/tsv2 && bash scripts/sweep.sh`, report `MANIFEST_REASON_DIFF` line; commit `compile/out/**` changes).

## Files owned
`v6/sprefa-engine-rs/src/serve.rs`, `v6/sprefa-engine-rs/tests/serve_uds.rs`, `v6/sprefa-engine-rs/tests/17_resident_coroutine.rs`, `v6/sprefa-engine-rs/tests/fixtures/resident-coroutine.program.rs`, `v6/dl/fixtures/resident-coroutine.dl6`, `v6/prolog/conformance/fixtures/resident_coroutine.pl`, `v6/prolog/3_clock_check.pl`, `v6/prolog/compile/test/plunit_tests.pl`, `v6/prolog/compile/out/**`, `v6/tsv2/gen_emitted/**` if the sweep writes them. Nothing else. Do NOT touch `registry.pl`, `parse_dl_dcg.pl`, `1_host_expand.pl`, `hosts.rs` (phase 2 lane owns them). Do NOT touch `dl_view/**`.

## Tests, iterate one at a time; whole batteries once at the end
`cargo test --test serve_uds`, `cargo test --test 17_resident_coroutine`, single plunit test via `swipl -g "run_tests(<name>)"` (check `plunit_tests.pl` head for the load recipe), then once: `cd v6 && just plunit` (5 known-red are in `.github/CI-KNOWN-RED.md:32`), conformance go.pl, `bash v6/sprefa-engine-rs/grade.sh` (report line; RATCHET means refresh `graded.tsv` with `RUST_GRADE_WRITE_GRADED=1` and commit only if the diff is the newly clean row), sweep once.

## Pre-commit
`DL_EXTRACT_BIN=/Users/chrishafley/projects/sprefa/v6/sprefa-extract/target/release/extract`; `pnpm install --frozen-lockfile` in `v6/tsv2` and `v6/sprefa-store/js` inside the worktree.

## PR
`gh pr create --base main`. Body: 1-3 plain sentences on what a user gets (the deltas route with a curl line, and the dl6 snippet of `resident`), `## Reading order` (numbered files, why each), `## Tests` (per test: name, input, expectation, what it printed before; one line "full suite unchanged otherwise"). No words gate/leg/receipt/door/probe/refusal, no em dashes, no suite counts, no allowlist refs. Do NOT merge; report PR number, head sha, the test result lines, grade line, sweep line, exact error text on any failure.
