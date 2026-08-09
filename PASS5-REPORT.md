lane boop5 pass 5 — the CONTROL facet (pass 1 of 2; a debur pass follows)

## Step 0 — tmux test law (receipt)

Every tmux-touching test uses a throwaway socket `boop-test-<pid>` and a
teardown that kills the whole server. The retrofit landed in the pass-4 commit;
this report re-verifies it holds on the full suite.

```
$ grep -rn 'boop-test' v6/boop/v6/boop/src 2>/dev/null   (paths relative)
v6/boop/src/tmux.rs:340  socket = "boop-test-<pid>"  (TestServer)
v6/boop/src/tmux.rs:426  "boop-test-<pid>-noserver"   (unreachable test)
v6/boop/src/harness/claude.rs  "boop-test-<pid>-ctl"  (facet-3 TmuxGuard)
$ tmux -L lanes ls | sort | md5   BEFORE test:  70415b45632bce38374353da9f2540eb
$ cargo test   -> 28 passed
$ tmux -L lanes ls | sort | md5   AFTER  test:  70415b45632bce38374353da9f2540eb
```
Byte-identical before and after: the tests never touch the live `-L lanes`
server.

## Step 1 — trait facet 3 (receipt)

`Harness` gained `capabilities / spawn / send / stop` with honest all-false and
Unsupported/error defaults (`src/harness.rs`). `Capabilities { send_midflight,
resume, spawn, subagent_visible }`, `enum SendOutcome { Injected,
QueuedForNextSpawn, Unsupported }`, `SpawnSpec`. `stop` is idempotent.

Capabilities matrix, per harness, with the test that backs each `true`:

| harness | send_midflight | resume | spawn | subagent_visible |
|---|---|---|---|---|
| claude | true | true | true | true |
| opencode / kimi / codex / ccz / pi | false (default) | false | false | false |

Backing tests (all in `cargo test`, each a named receipt):
- send_midflight `claude_send_injects_into_a_live_pane`: send returns
  `Injected` and the text comes back from `capture-pane`.
- spawn `claude_spawn_returns_handle_and_stop_tears_down`: spawn builds a
  tmux session (throwaway socket), stop leaves it gone.
- resume `claude_launch_resumes_with_session_id`: the launched command carries
  `--resume <id>`.
- subagent_visible `claude_reads_subagent_edge_from_real_fixture` (step 3).
- The all-false row is the trait default: kimi/codex/ccz/pi are not registered
  this pass, so nothing claims more than the honest default (a debur pass can
  flip one per measured test).

## Step 2 — spawn defaults (receipt)

`src/worktree.rs::prepare_spawn_dir` makes a worktree by default — `git
worktree add -b <branch> <path> <base_sha>` then `merge --ff-only <base_sha>`,
refusing a non-fast-forward — and runs the `SpawnSpec.setup` steps in order.
`main_tree: true` is the only path that works in the main tree. `Claude::spawn`
routes through it. `--main-tree` / `--base-sha` added to the CLI dispatch.
Tests: `worktree_spawn_creates_a_branch_at_the_base`,
`setup_steps_run_in_order_in_the_worktree`,
`main_tree_spawn_refuses_a_non_fast_forward`, all on throwaway git repos in
tempdirs (never this repo).

## Step 3 — agent_edge first-class (receipt)

- `SessionRef.parent` is read from claude's real layout: a transcript under a
  `subagents/` directory inherits its parent from the containing folder's
  name (the spawning session id). Real subagent files are copied into
  `tests/fixtures/claude/2579238b-.../subagents/`.
- New output record `{"kind":"agent_edge","parent":"<sid>","child":"<sid>",
  "edge":"spawned"}`, emitted by the `chat`/`events` doors.
- `sync` writes the edge and `query_edges` re-joins it to TEXT.
- Fixture test `claude_reads_subagent_edge_from_real_fixture` (subagent
  `agent-a6cee372fea5c1c2f` -> parent `2579238b-...`).

Other harnesses: codex (`parent_thread_id` / `thread_spawn_edges`), opencode
(`session.parent_id`), kimi (`state.json.agents[*].parentAgentId`) are NOT
registered this pass, so their edge sources are not read. `parent` is left
`None` and this is stated, not fabricated. The `agent_edge` rel shape maps to
`agent_edge(parent_session, child_session, edge_kind)`.

## Step 4 — CLI reroute (receipt)

`dispatch` / `hail` / `lane` route through the facet-3 trait; tmux is a
transport detail inside the impl.

- `hail`: appends the mailbox message, then `harness.send` on a SessionRef
  carrying the route's pane handle; outcome printed as
  `injected into tmux <pane>` / `queued for next spawn` / `has no send
  support`.
- `dispatch`: resolves the harness adapter, builds a `SpawnSpec` (repo=--cwd,
  base sha = `git rev-parse HEAD`, branch = --tmux|--to, main_tree from
  --main-tree), `harness.spawn`, sends the dispatched stamp, writes the
  registry route (tmux = the spawned session's handle) and the mailbox row.
- `lane`: the dispatch wrapper (now via the trait).

Verbs whose surface changed (before/after):
- `dispatch` previously ran a raw tmux shell in `--cwd`; it now spawns the
  harness through the worktree path and requires `--cwd` to be a git repo
  (base sha) unless `--main-tree` is given. Flags themselves are unchanged
  except the added `--main-tree` and `--base-sha`.
- `hail` output is unchanged in the success line; the Unsupported/queued lines
  are new branches.
- `list` / `sessions` / `tail` / `chat` / `events` / `measure` are untouched.

Deviation: `--harness opencode` resolves to the first registered adapter
(claude) because the opencode adapter is not registered yet; the dispatch
still succeeds via claude's transport. This is called out for the debur pass.

## Deviations and what is still open

- Only claude is a registered harness; opencode/kimi/codex transcript and
  control adapters are unbuilt (their SQL-tail reading is a later pass), so
  their capability and edge rows are all default-false / parent None.
- `dispatch --harness opencode` falls back to claude (above).
- `follow`'s synthesised done-event (the turnkey goal) is still outstanding;
  this pass landed the control facet it hangs off.

## Files touched

- `v6/boop/src/harness.rs` (trait facet-3 + types + SessionRef fields)
- `v6/boop/src/harness/claude.rs` (facet-3 impl + parent/edge reading + tests)
- `v6/boop/src/worktree.rs` (new: spawn worktree + ff-only + setup + tests)
- `v6/boop/src/main.rs` (CLI reroute dispatch/hail/lane, --main-tree, edge door)
- `v6/boop/src/ident.rs` (add_edge / query_edges / sync edge write + test)
- `v6/boop/tests/fixtures/claude/...` (real subagent transcript + meta)
- `v6/boop/src/worktree.rs`, `src/chat.rs`, `src/ident.rs` (SessionRef field
  churn in test constructors)

## Gates

```
cargo build           Finished (debug)
cargo test            result: ok. 28 passed; 0 failed
cargo clippy -- -D warnings   0 errors, 0 warnings
```

Commits on `lane/boop`: `551f6687` (facet 3), `15bad46a` (spawn defaults),
`bbbad7b1` (agent_edge), `46e7a32f` (CLI reroute). Step 0's retrofit is in the
prior pass-4 commit `970f38fb`.
