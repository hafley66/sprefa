# Lane brief: two boop lane-lifecycle defects (hafley-rs)

First action: `git merge --ff-only d4e9f5d`. Failure = STOP AND REPORT.
Repo: ~/projects/hafley-rs. All work in `crates/boop/**`.

## Defect 1: claude-harness lanes exit rc=1 after completing (RCA + fix)

Issue: sprefa `issues/claude-lane-exit-one`. Four claude-harness lanes
(2026-08-15/16) finished their deliverable and still delivered exit_code=1;
two also left work uncommitted. Coordinators currently ignore claude-lane rc
entirely, which destroys the signal.

Spec:
1. Read `crates/boop/src/harness/claude.rs` and the spawn/result path in
   `crates/boop/src/lane.rs` / `runtime.rs`. Find where the lane's on-exit rc
   is captured and what command's status it captures.
2. Likely shapes to check, in order: the wrapper records the rc of a FINAL
   shell command in the tmux pane (not the claude CLI itself); the claude CLI
   itself exits nonzero after a completed session under some flag boop passes;
   the wrapper conflates a post-session hook rc with the harness rc.
3. Write the RCA as a comment on nothing — put it in the PR/commit body, with
   file:line receipts.
4. Fix so a completed session reports rc=0 and a genuinely failed spawn
   reports nonzero.
5. Test WITHOUT invoking the real claude CLI: a fake harness binary (shell
   script fixture) that prints a completed-session transcript then exits 1,
   spawned through the same wrapper path; assert boop's recorded rc. Add the
   inverse case (script exits 0) and a genuine-failure case (script exits
   before emitting anything).

## Defect 2: lane-main-tree-escape rail

Issue: sprefa `issues/lane-main-tree-escape`. A pro4 lane did all work in the
MAIN tree and committed to local main; its registered worktree stayed at base
sha; `lane wait` returned rc=0.

Spec:
1. At `lane wait` result time (and in `lane list` output), compare the lane's
   registered worktree HEAD against its recorded base sha AND check whether
   the lane's branch has any commit. Files: `crates/boop/src/lane.rs`,
   `crates/boop/src/worktree.rs`.
2. If the worktree has zero new commits, the wait result line prints a loud
   `WORKTREE-UNTOUCHED` flag (stdout, part of the result row text). Do not
   hard-fail the wait; the coordinator decides.
3. If commits exist on the repo's local main that are not on origin/main and
   are newer than lane spawn, print a `MAIN-TREE-COMMIT-SUSPECT` line with the
   sha list. Detection only, no automatic reset.
4. Tests: fixture repo + fake lane registration; one test per flag; one
   control test where the lane committed properly and neither flag prints.

## Receipts (three runs each)

```bash
cd ~/projects/hafley-rs && cargo test -p boop
cargo clippy -p boop -- -D warnings
```

Commit per defect (two commits), never pipe a commit, check `git log` before
finishing.

## File ownership

OWNS: `crates/boop/**`. FORBIDDEN: everything else in hafley-rs.

## Laws

- No `eprintln!` in src/** unless the line is CLI-UX and carries
  `@eprintln-ok` (the flag lines in defect 2 qualify).
- Comment budget: only constraints code cannot show; RCA narrative goes in
  the commit body, never in code comments.
- A permission denial ends the approach; report, never work around.
