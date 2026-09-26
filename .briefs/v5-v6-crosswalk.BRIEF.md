# v5-v6-crosswalk (research; doc only)

Goal: write plans/v8/2026-09-26-crosswalk.md. Columns: ability | v5/v6 source | v8 status (have/partial/missing) | v8 file:line | ladder rung.
Ladder rungs: seeds, filter, join 1:1/1:n/n:n, negation, count, min/max, recursion chain/cycle, kernels, demand, paging/effects, comptime rounds.

Sources, read fully (not sampled):
- v5/tests/it: all 227 files. Includes comment_node markdown snippet maintenance; where/bytes/file/git-repo/rev model (now in soopy; map to soopy, never re-solve); shell effects v5/src/effect.rs:186 ShellEffectExec, shell_templates :514.
- v6/READINESS.md, v6/ARCHITECTURE.md (two-plane tick), v6/prolog/LANG.md (pre(atom) :44), v6/prolog/3_clock_check.pl, v6/DECISIONS.md (latest()).
v8 code: src/ (dl8), tests/.
Acceptance: every v5/tests/it file appears in at least one row (append a coverage list: file -> row id); have/partial rows carry a v8 file:line.
No builds needed. Commit subject: "plans: v5/v6 -> v8 crosswalk" with Boop-Status done.

## Laws
- Work only in $PWD (your worktree). Never cd to a primary checkout. Never push. Never commit on main.
- RAM: CARGO_BUILD_JOBS=4, cargo -j 4, RUST_TEST_THREADS=4, CARGO_INCREMENTAL=0. Stop if your process tree exceeds 4GB RSS.
- CARGO_TARGET_DIR=$HOME/.cache/boop/lanes/<your-lane-id>/target. Never $PWD/target.
- Never edit expected/golden/fixture files or flip probe rc to make a test pass. If an expectation looks wrong, commit with Boop-Status blocked + Boop-Ask.
- Commit trailers: Boop-Status wip|done|blocked, Boop-Check "<cmd> -> <result>".
- Final message receipt: status, sha, files, validation, next.

