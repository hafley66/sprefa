# LANE boop5 — pass 5: the CONTROL facet (pass 1 of 2; a debur pass follows)

You are lane boop5 in worktree /Users/chrishafley/projects/sprefa-lanes/boop,
branch lane/boop. You own v6/boop/** ONLY. Do not touch v6/prolog, v6/tsv2,
v6/dl, or any fixture. If reality deviates from this brief, STOP and hail;
do not improvise.

FIRST ACTION (worktree dispatch law):
    git merge --ff-only 88e2ff44
On any failure: STOP and hail BLOCKED with the git output.

Read before coding: PASS4-CONTROL.md (the seed; its content is the spec),
NORTH-STAR-CODEGEN.md, QUERY-SURFACE.md, all at this worktree root.

Build tool: cargo, debug profile (`cargo build` + `cargo test` in v6/boop).
No release build needed this pass. Never run npm/pnpm.

## Step 0 — tmux test law, BEFORE any facet-3 code (own commit)

Pass 3's tests spawned 18 real sessions on the live `-L lanes` socket.
Retrofit EVERY existing test that touches tmux:
- each test uses a throwaway socket: `tmux -L boop-test-<pid or uuid>`
- teardown kills the whole server: `tmux -L boop-test-<...> kill-server`,
  including on panic (use a Drop guard).
Receipt: `grep -rn 'boop-test' v6/boop/tests v6/boop/src` shows every tmux
touch point; `tmux -L lanes ls` output is byte-identical before and after
`cargo test`.

## Step 1 — trait facet 3 (own commit)

Extend the Harness trait exactly per PASS4-CONTROL.md:

    fn capabilities(&self) -> Capabilities;
    fn spawn(&self, spec: &SpawnSpec) -> Result<SessionRef>;
    fn send(&self, s: &SessionRef, text: &str) -> Result<SendOutcome>;
    fn stop(&self, s: &SessionRef) -> Result<()>;

with `Capabilities { send_midflight, resume, spawn, subagent_visible }` and
`enum SendOutcome { Injected, QueuedForNextSpawn, Unsupported }`.

CAPABILITY HONESTY LAW: a capability is `true` ONLY if a test in this pass
exercises it and the test names its receipt. kimi/codex/ccz/pi adapters:
Capabilities all false, spawn/send/stop return Unsupported/error. Claude and
opencode impls encode the measured truths in PASS4-CONTROL.md:
- opencode `run`: send_midflight=false, send => QueuedForNextSpawn; the queue
  drains as a `-s <session>` respawn.
- claude/opencode TUIs: tmux send-keys => Injected.
- stop = kill the tmux session (throwaway-socket tested only).

## Step 2 — spawn defaults (own commit)

`spawn` creates a worktree by default: `git worktree add` + branch from
SpawnSpec's stated base sha, then `git merge --ff-only <sha>` semantics
(refuse if not fast-forward). Main-tree spawn requires explicit
`main_tree: true` in SpawnSpec (CLI flag `--main-tree`). The worktree gap
steps (pnpm install x2, cargo build) are represented as a `Vec<String>` of
setup commands on SpawnSpec, executed in order, empty by default in tests.
Tests use a throwaway git repo under a tempdir, never this repo.

## Step 3 — agent_edge first-class (own commit)

- `SessionRef` gains `parent: Option<String>`.
- New output record: `{"kind":"agent_edge","parent":"<sid>","child":"<sid>","edge":"spawned"}`
  emitted in the events and chat doors, shaped onto the rel
  `agent_edge(parent_session, child_session, edge_kind)`.
- Read parenthood from each harness's REAL source, fixtures copied from real
  files into v6/boop/tests/fixtures:
  claude `subagents/agent-<id>.meta.json`, codex `parent_thread_id` +
  `thread_spawn_edges`, opencode `session.parent_id`, kimi
  `state.json.agents[id].parentAgentId`.
  A harness whose fixture you cannot produce from a real file on this machine:
  leave parent=None and SAY SO in the report; never fabricate a fixture shape.

## Step 4 — CLI reroute (own commit)

`dispatch`/`hail`/`lane`-shaped verbs in the boop CLI route through facet 3
(trait calls), tmux demoted to a transport detail inside impls. Existing verb
flags and output stay byte-identical where behavior is unchanged; any verb
whose surface must change is listed in the report with before/after.

## Style laws (repo, non-negotiable)

- Comments: max 2 consecutive lines, only constraints the code cannot show.
  No change-log narrative, no pass numbers in code.
- Banned identifiers and prose: provenance, substrate, load-bearing, regime.
- Descriptive names, never single letters.
- No stray debug prints; boop's product output goes to stdout, everything
  else through the existing logging seam.
- Commit per step with prefix `boop:`; run `cargo test` green before each
  commit.

## Deliverable

PASS5-REPORT.md at worktree root, first line `lane boop5 pass 5`. Contents:
per-step receipts (commands + verbatim key output), the Capabilities matrix
per harness with the test name backing each `true`, deviations, files
touched. Then run exactly:

    bus hail --to fable-main --kind result --body "boop5 done: <one line>"

or on any blocker:

    bus hail --to fable-main --kind result --body "boop5 BLOCKED: <one line>"
