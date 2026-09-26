# oracle-refreeze-v8 (user decision 2026-09-26: v6/v7 frozen snapshots stop being the spec)

Full body: issuectl show oracle-refreeze-v8.
Goal: refreeze every oracle/{read,macrotime,lower,check,load,comptime,reify,compile,eval} fixture from the v8 Rust engine (dl8, DL8_ENGINE=rust) so they become v8 regression snapshots.
- oracle/refreeze.sh + oracle/refreeze.py refreeze from v7 Prolog today; re-aim them at dl8, keep the file format.
- Commit 1: script change only. Commit 2: regenerated fixtures. This lane is authorized to rewrite oracle/* fixtures; no other expected/golden file.
- Commit 2 body: every fixture whose output changed vs v7 (file, first differing row). If a change looks like a v8 bug, list it under "suspect" in the body; do not patch the engine.
Acceptance: tests _0.._8 green on refrozen fixtures; _28 and _29 still green.
Final commit Boop-Status done with Boop-Check.

## Laws
- Work only in $PWD (your worktree). Never cd to a primary checkout. Never push. Never commit on main.
- RAM: CARGO_BUILD_JOBS=4, cargo -j 4, RUST_TEST_THREADS=4, CARGO_INCREMENTAL=0. Stop if your process tree exceeds 4GB RSS.
- CARGO_TARGET_DIR=$HOME/.cache/boop/lanes/<your-lane-id>/target. Never $PWD/target.
- Commit trailers: Boop-Status wip|done|blocked, Boop-Check "<cmd> -> <result>".
- Final message receipt: status, sha, files, validation, next.
