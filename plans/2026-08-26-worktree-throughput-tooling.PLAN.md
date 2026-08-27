# Worktree throughput tooling survey

Status: survey, not dispatched. User ask 2026-08-26: agents drop worktrees at arbitrary paths; find tools that make high-throughput worktree lanes cheaper and place them in one spot.

Decision context: `boop beep lane create` is the one creator and puts trees under `<repo>/.boop-worktrees/<branch>` (CLAUDE.md "Dispatch"). Every pick below keeps boop as the creator.

- [Problem](#problem)
- [Candidates](#candidates)
- [Picks](#picks)
- [Order of work](#order-of-work)
- [Out of scope](#out-of-scope)

## Problem

| symptom | where it shows |
|---|---|
| trees minted outside `.boop-worktrees/` | `git worktree list` on sprefa and hafley-rxjs; Claude Code `--worktree` defaults to `.claude/worktrees/` |
| stale trees after SIGKILL'd lanes | `boop beep lane list` DEAD rows with WORKTREE-UNTOUCHED |
| every lane pays `git status` walks and cold `cargo` builds | boop-start 32 s on a fresh lane (shared target dir already cached) |
| dependent arcs land by hand rebase | lane B waits on lane A in every multi-arc brief |

## Candidates

| tool | category | what it does | helps lanes how | link |
|---|---|---|---|---|
| worktrunk `wt` | worktree CLI (Rust, 6.7k) | worktree by branch name, path template, `wt switch -x claude -c feat -- "task"` | one line = worktree + agent; shares build cache | github.com/max-sixty/worktrunk |
| wtp | worktree CLI (Go) | post-create hooks: copy gitignored `.env`, run setup | the bootstrap boop-start hand-rolls | github.com/satococoa/wtp |
| gwq | worktree CLI (Go, ~400) | fzf finder, ghq-style global layout | one registry of trees across repos | github.com/d-kuro/gwq |
| git-worktree-switcher / git-worktree-cli | worktree CLI (sh / Node) | switch by name, auto-organized dirs | path templating reference only | github.com/yankeexe/git-worktree-switcher |
| Claude Code `--worktree` | native | trees at `.claude/worktrees/<branch>` | overlaps boop; a second place trees get dropped | |
| container-use | sandbox (Go, 4k) | Dagger container + `container-use/<env>` branch per agent | discard a bad run whole | github.com/dagger/container-use |
| Fly.io Sprites | sandbox | Firecracker microVM, checkpoint/restore ~300 ms | fork a prepared VM per lane | fly.io/learn/agent-sandbox |
| Claude Squad / ccmanager / uzi / Conductor / vibe-kanban | orchestrators | tmux or UI + worktree per agent | closest analogs to boop lanes; nothing boop lacks | github.com/smtg-ai/claude-squad |
| Clash | conflict tooling | predicts merge conflicts between live worktrees | lane ownership collisions before the PR | nimbalyst.com |
| git-spice `gs` | stacked diffs (Go, ~745) | branch stacks, stacked GitHub PRs, no server | PR-per-arc chains without manual rebases | github.com/abhinav/git-spice |
| git-branchless | stacked diffs (Rust, 3.9k) | smartlog, `move`, `restack`, undo | restack every lane after main moves | github.com/arxanas/git-branchless |
| Graphite `gt` / Aviator `av` | stacked diffs | commercial / OSS merge queue | queue many small lane PRs | graphite.com, github.com/aviator-co/av |
| jj | VCS | workspaces on one commit, no branch binding, no stash/index; no partial clone | throwaway anonymous heads; does not constrain placement | jj-vcs.github.io |
| Sapling `sl` | VCS | Meta's git-compatible stacks, `sl pr submit` per commit | stacks without branch names | sapling-scm.com |
| gitoxide `gix` / git2-rs | Rust libs | in-process worktree add/status/prune | boop stops forking `git` per lane | github.com/GitoxideLabs/gitoxide, docs.rs/git2 |
| `git maintenance start` | git config | hourly prefetch, commit-graph, `worktree-prune` | reaps SIGKILL'd lane trees, keeps `worktree add` fast | git-scm.com/docs/git-maintenance |
| `core.fsmonitor` + `core.untrackedCache` | git config | daemon file watcher, cached untracked scan | per-lane `git status` stops walking the tree | |
| sparse-checkout cone / partial clone | git feature | materialize only owned crates | smaller lane trees | |
| `git worktree add --detach` / `prune` | git feature | no branch reserved; reap stale metadata | dodges "branch already checked out" | git-scm.com/docs/git-worktree |

## Picks

| pick | cost | receipt |
|---|---|---|
| `git maintenance start`, `core.fsmonitor=true`, `core.untrackedCache=true` on the sprefa and hafley-rxjs main clones | config only | `time git status` in a lane tree before/after |
| PreToolUse hook denying `git worktree add` whose path is outside `.boop-worktrees/` | one hook script | hook fires on a probe command; `git worktree list` shows only `.boop-worktrees/` paths after a week |
| boop-start adopts `wtp`-style post-create hooks (gitignored files, setup) and `worktrunk` build-cache sharing | boop change, hafley-rs | boop-start wall on a fresh lane, before/after |
| boop worktree add/list/prune on `gix` (git2 where gix lacks worktree ops) | boop change, hafley-rs | no `git` subprocess in `lane create` trace |
| `git-spice` for PR-per-arc stacking | install + brief line | lane B lands on top of lane A with `gs stack submit`, no hand rebase |

## Order of work

1. Config knobs and the placement hook, sprefa main, no lane.
2. boop-start hooks and cache sharing, hafley-rs lane.
3. git-spice trial on one two-arc dispatch; adopt if the rebase step disappears.
4. gix migration, hafley-rs lane, after 2.

## Out of scope

- jj / Sapling: they remove the reason for many worktrees but do not constrain where a tree lands; the hook does. Revisit if the stash/index friction outweighs retraining every skill.
- container-use / Sprites: isolation beyond a worktree is not a current need.
- Orchestrator UIs: boop already is one.
