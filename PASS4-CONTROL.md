# LANE boop — PASS 4 seed: the control trait (user word, 2026-08-08)

User: "is boop making it easy for us to control anything in tmux and send
messages like that in kimi/opencode/codex/claude-code/ccz/pi etc. with uniform
interface for that and the impls determine how that works per for
mutations/reads/subscribes"

## INCIDENT FIRST: your pass-3 tests spawned 18 real tmux sessions

Numbered sessions (5..37) appeared on the LIVE `-L lanes` socket during pass 3
and were left behind; the coordinator killed them. Law from now on: every test
that touches tmux uses a throwaway socket (`tmux -L boop-test-<pid>`) and a
teardown that kills the whole server (`tmux -L boop-test-<pid> kill-server`).
Add that to the existing tests before any pass-4 work.

## The trait grows from one facet to three

```rust
pub trait Harness {
    // facet 1: READ (built, pass 1)
    fn id(&self) -> &'static str;
    fn sessions(&self) -> Result<Vec<SessionRef>>;
    fn read_from(&self, s: &SessionRef, offset: u64) -> Result<ReadChunk>;

    // facet 2: SUBSCRIBE (pass 4)
    // follow = read_from in a loop + new-session discovery; no polling API
    // leaks to callers, they get an iterator/stream of AgentEvent
    fn follow(&self, filter: &SessionFilter) -> Result<EventStream>;

    // facet 3: CONTROL (pass 4)
    fn capabilities(&self) -> Capabilities;
    fn spawn(&self, spec: &SpawnSpec) -> Result<SessionRef>;
    fn send(&self, s: &SessionRef, text: &str) -> Result<SendOutcome>;
    fn stop(&self, s: &SessionRef) -> Result<()>;
}

pub struct Capabilities {
    pub send_midflight: bool,   // claude TUI true; `opencode run` false
    pub resume: bool,           // opencode -s, claude --resume, ...
    pub spawn: bool,
    pub subagent_visible: bool, // does the harness record spawn edges
}

pub enum SendOutcome { Injected, QueuedForNextSpawn, Unsupported }
```

Measured truths the impls encode (from the bus skill + this repo's logs):
- `opencode run` takes its prompt from argv; send_midflight = false, send
  returns QueuedForNextSpawn and the queue drains as a `-s <session>` respawn.
- claude/opencode TUIs accept tmux send-keys; Injected.
- No lane of ANY harness emits a completion event; `follow` + process-death is
  how boop synthesizes one. That synthesized done-event is the turnkey goal.
- kimi/codex/ccz/pi adapters: sessions()/read_from first, Capabilities all
  false until measured. NEVER claim a capability without a receipt in tests.

CLI: the existing verbs reroute through the trait (`dispatch`/`hail`/`lane`
stop being tmux-only helpers and become facet-3 calls; tmux is one transport an
impl may use). North-star files still bind: NORTH-STAR-CODEGEN.md (rows stay
rel-shaped), turnkey done-events.

## Agent-to-agent edges are first-class (user word, same session)

"we should have agent<-relationship->other agent, usually parent child."

- `SessionRef` gains `parent: Option<String>` (the spawning session id).
- A new record kind in `events`/`chat` output:
  `{"kind":"agent_edge","parent":"<sid>","child":"<sid>","edge":"spawned"}`.
- Every harness already writes this natively; read the real source, never
  guess from directory nesting: claude `subagents/agent-<id>.meta.json`
  (agentType, toolUseId, spawnDepth, model); codex `parent_thread_id` +
  `thread_spawn_edges`; opencode `session.parent_id`; kimi
  `state.json.agents[id].parentAgentId`.
- Rel shape it must map onto (codegen north star):
  `agent_edge(parent_session, child_session, edge_kind)`.

## Spawn defaults (user word, 2026-08-08)

"when making subagent that making a worktree as required but to turn it off
and work in main requires flag."

`spawn`/`dispatch` CREATE A WORKTREE by default (branch from a stated base
sha, per the repo's worktree dispatch law). Working in the main tree requires
an explicit `--main-tree` flag. The default path also runs the worktree gap
steps the brief names (pnpm install x2, cargo build) before handing the lane
its prompt.

Do not start pass 4 until dispatched; this file is the brief seed (now pass 5).
