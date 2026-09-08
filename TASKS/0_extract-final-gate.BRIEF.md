# Finish combined extract

Use Sol. Work only in the new lane worktree branched from the combined extract commit. Read AGENTS.md and v6/sprefa-extract/reports/1_combined_integration.md first.

The user requests completion. Preserve telemetry, Falcon nested Cargo staging, lockfile handling, missing generated file handling, Rust re-export filtering, and actual calls after comments. No compiler/kernel edits. No primary checkout edits, merge, push, global installation, or unrelated changes.

Run the complete `cargo test --features cli` from v6/sprefa-extract with a dedicated CARGO_TARGET_DIR. The previous two-run limit is superseded: allow four complete attempts, using focused tests while diagnosing failures. Fix in-scope failures, then obtain a complete post-fix passing run. Do not weaken semantic tests or remove coverage to pass. Explain any unavoidable external blocker with exact command and receipt.

Review integration seams and reproduce the original Falcon extract slow command against /Users/chrishafley/projects/hafley-rs/games/blender-godot-sqlite-proof/falcon-lab if executable code changes. Use scratch cache/output, preserve source bytes, verify zero scip_skip and semantic output. Existing unchanged-code receipts may be retained with exact build provenance.

Update the integration report with final current tests, CI coverage added/changed, command, counts and receipt paths. Commit fixes and report. Run cargo fmt once immediately before each commit, including resulting formatting. No formatter during implementation. Do not stop after focused tests while the full gate remains runnable. Return commit IDs, current complete gate result, Falcon evidence, and remaining blockers. Send completion through Boop to sprefa-ivm-extract-parent; do not use native collaboration tools.
