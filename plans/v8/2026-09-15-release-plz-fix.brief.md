# Brief: finish the release-plz arc: make `just release-dry` run, commit the changelogs, post the PR

## 1. Job
Branch `chore/release-plz` (pushed, head `cc543483fd04a0d832be6b90773238c82d65a6df`) carries the config and recipes. Three things are missing, measured by the coordinator in that tree: `just release-dry` fails (`cannot determine current branch`: the throwaway worktree is detached and release-plz runs `git rev-parse @{upstream}`), no `CHANGELOG.md` was written for `v8`, `sqlite_ivm`, `v6/sprefa-engine-rs`, `v6/sprefa-store`, and no PR was posted. Finish those three and nothing else.

## 2. Base and first action
- Base sha: `cc543483fd04a0d832be6b90773238c82d65a6df` (`origin/chore/release-plz`). Branch `chore/release-plz-fix`. PR base `main`.
- FIRST command: `git merge --ff-only cc543483fd04a0d832be6b90773238c82d65a6df`. Failure = stop and report.
- Commits end with `Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>`. `release-plz 0.3.160` is installed.

## 3. Ownership
You own: `justfile` recipe `release-dry` only (lines 53 onward), the new `CHANGELOG.md` files under `v8/`, `sqlite_ivm/`, `v6/sprefa-engine-rs/`, `v6/sprefa-store/`, and root `CHANGELOG.md` only by what `release-plz update` appends. Forbidden: every `release-plz.toml`, the `release` recipe, every `Cargo.toml` version by hand, `src/**`. Never `git tag`, never `gh release`, never `release-plz release`. Never spawn subagents. Never `--no-verify`.

## 4. The fix, decided
`release-dry` must give release-plz a branch with an upstream. Replace `git worktree add --detach "$scratch" HEAD` with a branch: `git worktree add -b release-dry-$$ "$scratch" HEAD` and, inside the scratch, `git branch --set-upstream-to=origin/main`; extend the trap to `git branch -D release-dry-$$`. Then run it: `just release-dry` exits 0 and prints the proposed bumps per manifest. Paste that output in the PR.

The changelogs: in the lane tree, run `release-plz update --manifest-path <m>/Cargo.toml --config <m>/release-plz.toml --allow-dirty` for the four non-root manifests, on a branch with upstream set to `origin/main` (`git branch --set-upstream-to=origin/main`). Commit the new `CHANGELOG.md` files and any `Cargo.toml`/`Cargo.lock` version bumps release-plz wrote, as `chore(release): first changelogs from release-plz`. If release-plz writes no changelog for a crate, say which and why (its printed reason).

## 5. Validation
```bash
just release-dry            # rc 0, no diff in this tree afterwards: git status --short | wc -l unchanged
ls v8/CHANGELOG.md sqlite_ivm/CHANGELOG.md v6/sprefa-engine-rs/CHANGELOG.md v6/sprefa-store/CHANGELOG.md
git diff --stat cc543483fd04a0d832be6b90773238c82d65a6df...HEAD    # section 3 files only
```
10 s per operation; over the cap is reported by name.

## 6. Style laws
No em dashes. Banned words: provenance, substrate, load-bearing, regime, ground truth, refusal, support, honest, distill, "here is", "below is", "the following". Comment budget: constraints only.

## 7. Reporting
PR title `chore: release-plz per manifest, conventional commits, just release`, base `main`, body carrying the receipt table (manifest, version before, proposed version, changelog lines, command) and the `just release-dry` output. Then:
```bash
boop beep --no-wait --as <your-lane-name> sprefa-coordinator "release-plz fix: PR #<n>, release-dry rc=<0|1>, changelogs <k>/4, bumps <list>"
```
Blocked or brief wrong: same command, one line, stop. One lane, one task.
