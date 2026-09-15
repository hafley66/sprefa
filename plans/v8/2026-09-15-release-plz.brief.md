# Brief: sprefa releases the way hafley-rs does: release-plz, conventional commits, a `just release` recipe

## 1. Job
Copy hafley-rs's release shape into sprefa. Chris's word: "do what hafley-rs does". No changesets, no new tool: release-plz reads conventional commits, bumps versions, writes CHANGELOG.md, tags, mints GitHub releases from this machine. The Actions release flow stays retired.

## 2. Base and first action
- Base sha: `b540384d18b370614c87cffb0a0a28603431a4c7` (`origin/main`). Branch `chore/release-plz`.
- FIRST command: `git merge --ff-only b540384d18b370614c87cffb0a0a28603431a4c7`. Failure = stop and report.
- Commits end with `Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>`.
- `release-plz --version`; if absent, `cargo install release-plz --locked` in the background, cap 10 min, report the version.

## 3. The template, verbatim from hafley-rs (read it first)
| piece | where |
|---|---|
| the recipe | `/Users/chrishafley/projects/hafley-rs/justfile:16-22` (`release:` = `release-plz update`, commit `CHANGELOG.md Cargo.lock crates/*/Cargo.toml` as `chore(release): version bumps and changelog`, push, `release-plz release --git-token "$(gh auth token)"`) |
| the config | `/Users/chrishafley/projects/hafley-rs/release-plz.toml` (`[workspace]`: `changelog_update = false`, `dependencies_update = false`, `publish = false`, `git_release_enable = true`, `git_tag_enable = true`, `release_commits = "^(feat|fix|perf|refactor)(\\([^)]*\\))?(!)?:"`, `semver_check = true`; then `[[package]]` rows with `version_group`, `changelog_update`, `changelog_path`, `changelog_include`) |
| semver check in CI | `/Users/chrishafley/projects/hafley-rs/.github/workflows/ci.yml:64` (`obi1kenobi/cargo-semver-checks-action@v2`) |

## 4. sprefa's shape, which differs from hafley-rs in one way: several manifests, not one workspace
| manifest | crate | version | in root workspace |
|---|---|---|---|
| `Cargo.toml` | `dl` (v5) | 0.12.0 | root, members `["tree-sitter-dl"]` |
| `v8/Cargo.toml` | `dl8` | 0.1.0 | no |
| `sqlite_ivm/Cargo.toml` | `sqlite-ivm` | 0.3.0 | no |
| `v6/sprefa-engine-rs/Cargo.toml` | `sprefa-engine-rs` | read it | no |
| `v6/sprefa-store` | read it | | no |
Existing: root `CHANGELOG.md` is hand-written Keep a Changelog for v5 with cargo-dist tags (`dist-workspace.toml`, `.github/workflows/release-dl6.yml`); keep it, release-plz appends under it. Do NOT add crates to the root workspace (the root `Cargo.toml:2-6` comment says why nested worktrees need it as is). Run release-plz once per manifest instead: `release-plz update --manifest-path <dir>/Cargo.toml` and `release-plz release --manifest-path ...`, one `release-plz.toml` beside each manifest, one `CHANGELOG.md` per crate directory. Tags are `<package>-v<version>`, release-plz's default, so the five crates never collide. Verify with `release-plz update --manifest-path v8/Cargo.toml --dry-run` (or the flag the installed version has; `release-plz update --help`) before wiring the recipe.

## 5. Deliverables
1. `release-plz.toml` at root and beside `v8`, `sqlite_ivm`, `v6/sprefa-engine-rs`, `v6/sprefa-store` (skip a directory whose crate has no `version` or is not buildable, and say so). Same `[workspace]` block as hafley-rs; `[[package]]` rows with `changelog_update = true`, `changelog_path = "CHANGELOG.md"`.
2. `justfile` recipe `release` at the repo root (`justfile` exists; read its style): loop over the manifests, `release-plz update`, commit the changed `CHANGELOG.md`, `Cargo.lock`, `Cargo.toml` files as `chore(release): version bumps and changelog`, push `main`, `release-plz release` per manifest with `--git-token "$(gh auth token)"`. Plus `release-dry` that runs the update step with the dry-run flag and touches nothing.
3. `.github/workflows/ci.yml` (read it first; add one job): `cargo-semver-checks-action@v2` for `v8` and `sqlite_ivm` only if the action supports `manifest-path`; else leave CI alone and say so.
4. `CHANGELOG.md` files created by the first `release-plz update` run, committed as the lane's second commit, so the PR shows what the tool writes. Do NOT run `release-plz release` and do NOT push tags; that is Chris's `just release`.
5. Receipt table in the PR: manifest, version before, version release-plz proposes, changelog lines it wrote, the command.

## 6. Ownership
You own: `release-plz.toml` (new, 5 files), `justfile` (two recipes only), `.github/workflows/ci.yml` (one job only, or untouched), the new `CHANGELOG.md` files under `v8/`, `sqlite_ivm/`, `v6/sprefa-engine-rs/`, `v6/sprefa-store/`, and root `CHANGELOG.md` only by what `release-plz update` appends. Forbidden: every `Cargo.toml` version field by hand (release-plz writes them), `src/**` anywhere, `.github/workflows/release-dl6.yml`, `dist-workspace.toml`. Never spawn subagents. Never `--no-verify`. Never `git tag`, never `gh release`.

## 7. Validation
```bash
release-plz --version
just release-dry                                   # exits 0, prints proposed bumps, no diff
git diff --stat origin/main...HEAD                 # section 6 files only
git log --format=%s origin/main..HEAD | grep -vE '^(feat|fix|perf|refactor|chore|docs)(\(.*\))?!?:' | wc -l   # 0
```
10 s per operation; a step over 10 s is reported, never waited out (the tool install runs in the background).

## 8. Style laws
No em dashes. Banned words: provenance, substrate, load-bearing, regime, ground truth, refusal, support, honest, distill, "here is", "below is", "the following". Comment budget: constraints only. Follow each file's existing style.

## 9. Reporting
PR title `chore: release-plz per manifest, conventional commits, just release`, base `main`. Then:
```bash
boop beep --no-wait --as <your-lane-name> sprefa-coordinator "release-plz: PR #<n>, manifests <k>, dry-run proposes <bumps>, ci job <added|skipped>, release-plz <version>"
```
Blocked or brief wrong: same command, one line, stop. One lane, one task.
