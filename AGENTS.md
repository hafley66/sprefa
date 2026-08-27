# Agent Instructions

## Agent lanes (boop)

Run `boop --help` before spawning or messaging any agent lane. The help text
is the doctrine: lane create flow, completion hail, the two liveness checks,
and ack semantics. Discover boop by using boop.

## V6 Status

V6 planning is underway — start at `v6/README.md`, plans in `v6/plans/`. V5
remains the shipping version. V6 exists because the same problems (storage
seam, daemon wire, build-vs-buy, repo/rev identity) kept being re-solved
inside V5's single crate; the V6 arc extracts crate and trait boundaries
instead of rewriting. Current phase: plans only, then hollow traits reviewed
by a human — no behavioral rewrites, no call-site migrations yet.

## Non-Interactive Shell Commands

**ALWAYS use non-interactive flags** with file operations to avoid hanging on confirmation prompts.

Shell commands like `cp`, `mv`, and `rm` may be aliased to include `-i` (interactive) mode on some systems, causing the agent to hang indefinitely waiting for y/n input.

**Use these forms instead:**
```bash
# Force overwrite without prompting
cp -f source dest           # NOT: cp source dest
mv -f source dest           # NOT: mv source dest
rm -f file                  # NOT: rm file

# For recursive operations
rm -rf directory            # NOT: rm -r directory
cp -rf source dest          # NOT: cp -r directory
```

**Other commands that may prompt:**
- `scp` - use `-o BatchMode=yes` for non-interactive
- `ssh` - use `-o BatchMode=yes` to fail instead of prompting
- `apt-get` - use `-y` flag
- `brew` - use `HOMEBREW_NO_AUTO_UPDATE=1` env var

## Rust Formatting

Formatting churn is not a review concern. Do not spend implementation or review
time minimizing, reconstructing, or undoing formatter-only changes.

Do not pass individual file paths to `cargo fmt`; Cargo may format every target
in the workspace anyway. Do not run the repository-wide formatter during
implementation. Run `cargo fmt` once immediately before commit and include all
resulting formatting in that commit.

## CI Reporting

CI means build, compile, and test execution. Report whether new work adds,
changes, or removes CI coverage. Do not report formatter or linter status.
Report only current results; omit stale, previous, and baseline-matching data.

## Issues (issuectl)

`issues/` edits made by `issuectl` commit directly on `main`. No branch, no
PR, no lane for an issue change. Push after committing.
