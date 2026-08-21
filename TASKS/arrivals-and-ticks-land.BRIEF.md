# Brief: land feature/arrivals-and-ticks

Branch `feature/arrivals-and-ticks` exists with 16 commits (the bind/sh/host collapse). Your
worktree is on it. FIRST ACTION: `git merge origin/main` (coordinator: main is the sha the
spawner prints; `just v5-rails` and `docs/failure-modes.md` 62-63 live there). Never spawn
subagents. `export CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=4`; `timeout` on everything.

## Coordinator's grade of the tree as left (2026-08-21)
conformance 440 PASS, plunit 1041/0, grade.sh 440/335, oracle rustc+knip OK, ghcacher 6,
crosswalk 10/10. FAILING: engine `cargo test -q` 114 passed 4 FAILED, all in
`v6/sprefa-engine-rs/tests/live_hosts.rs`:
- `:273 an_unrouted_sh_declaration_is_a_named_stop_at_construction` still spells adapter `shell`
- `:304 native_structured_input_does_not_enter_a_scalar_transport_check` adapter `sprefa_extract`
- `:424 unknown_family_name_is_a_named_stop_in_the_extract_twin` extract wants input `path`
- `:511 digest_carrying_demand_reads_the_blob_not_the_worktree` same `path`
Fix the tests to the new form (the behaviour under test must stay tested: a named stop at
construction, the scalar seam, the unknown family stop, blob-not-worktree). Also:
`just selfdoc` (extract.md drifted), `just feature-reach` must print diet/scip/nested PASS.

## USER DECISION (2026-08-21): the executor path is spelled with slashes
`rel /soopy/files(glob: key(text)) -> (path: text, digest: text).` Not dotted. Apply across
the tree: parser (`parse_dl_dcg.pl` dotted_path), registry roster, `LINKED_EXECUTORS` in
`hosts.rs:41`, every `.dl6` and test string, tmLanguage, docs. The `scip.diet.call` namespace
becomes `/scip/diet/call`. Byte-identical `emit_ts.pl` output is NOT required for programs the
collapse already touched; it is for any program it did not.

## Then
Post the PR against main with every gate number, three runs for conformance and engine.
Title: "arrivals: sh, bind and host collapse into rel -> with slash executor paths".
Style laws as in TASKS/arrivals-and-ticks.BRIEF.md.
