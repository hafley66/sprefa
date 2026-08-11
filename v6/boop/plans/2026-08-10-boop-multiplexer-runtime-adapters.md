# Boop Multiplexer Runtime Adapters and Herdr 0.8 Study

Date: 2026-08-10

## Goal

Keep `boop beep` as the coordinator-facing command surface while allowing tmux-like runtimes to own terminal state. tmux, Herdr, Zellij, and later runtimes become implementations of one Boop multiplexer boundary.

Boop remains a short-lived Rust CLI plus SQLite transcript and analytics index. Boop does not need to become a terminal server. An adapter may address an external runtime that owns a server.

## Responsibilities

### Multiplexer runtime

- PTY ownership
- Process lifetime
- Terminal buffer and scrollback
- Pane or terminal identity
- Attach and detach
- Input delivery
- Resize
- Runtime layout and containment
- Foreground process inspection when available
- Viewport coordinates when available
- Agent lifecycle state when available

### Boop

- `boop beep` CLI contracts
- Lane identity
- Parent-agent edges
- Goals and briefs
- Git branch and worktree associations
- Harness session identity
- Transcript ingestion
- SQLite turns, usage, commands, facts, fetches, skills, PRs, and historical edges
- Normalized records returned to Instant and other consumers

## Type Signatures

```rust
pub trait Multiplexer {
    fn identity(&self) -> RuntimeIdentity;
    fn capabilities(&self) -> MultiplexerCapabilities;
    fn snapshot(&self) -> Result<MultiplexerSnapshot, MultiplexerError>;
    fn create(&self, request: CreateTerminal) -> Result<TerminalHandle, MultiplexerError>;
    fn close(&self, target: &TerminalTarget) -> Result<(), MultiplexerError>;
    fn send(
        &self,
        target: &TerminalTarget,
        input: TerminalInput,
    ) -> Result<SendReceipt, MultiplexerError>;
    fn read(&self, request: ReadTerminal) -> Result<TerminalRead, MultiplexerError>;
    fn viewport(
        &self,
        target: &TerminalTarget,
    ) -> Result<Option<Viewport>, MultiplexerError>;
    fn process(
        &self,
        target: &TerminalTarget,
    ) -> Result<Option<ForegroundProcess>, MultiplexerError>;
    fn wait(&self, request: WaitTerminal) -> Result<WaitResult, MultiplexerError>;
}

pub struct RuntimeIdentity {
    pub kind: RuntimeKind,
    pub instance: String,
    pub version: Option<String>,
}

pub enum RuntimeKind {
    Tmux,
    Herdr,
    Zellij,
    Other(String),
}

pub struct MultiplexerCapabilities {
    pub topology: bool,
    pub viewport: bool,
    pub semantic_agent_state: bool,
    pub output_wait: bool,
    pub live_frames: bool,
    pub remote_attach: bool,
}

pub struct MultiplexerSnapshot {
    pub runtime: RuntimeIdentity,
    pub containers: Vec<RuntimeContainer>,
    pub terminals: Vec<RuntimeTerminal>,
    pub edges: Vec<ContainmentEdge>,
}

pub struct RuntimeTerminal {
    pub id: RuntimeTerminalId,
    pub label: Option<String>,
    pub cwd: Option<PathBuf>,
    pub process: Option<ForegroundProcess>,
    pub viewport: Option<Viewport>,
    pub agent: Option<RuntimeAgentState>,
}

pub struct RuntimeAgentState {
    pub harness: Option<String>,
    pub name: Option<String>,
    pub status: AgentStatus,
    pub state_change_seq: Option<u64>,
    pub session_id: Option<String>,
    pub evidence: Option<StateEvidence>,
}

pub enum AgentStatus {
    Idle,
    Working,
    Blocked,
    Done,
    Unknown,
}

pub struct Viewport {
    pub offset_from_bottom: u64,
    pub max_offset_from_bottom: u64,
    pub rows: u32,
    pub columns: u32,
}

pub enum ReadSource {
    Visible,
    Recent,
    RecentUnwrapped,
    Detection,
}

pub struct ReadTerminal {
    pub target: TerminalTarget,
    pub source: ReadSource,
    pub lines: Option<u32>,
    pub ansi: bool,
}

pub enum TerminalTarget {
    RuntimeId(RuntimeTerminalId),
    Lane(LaneId),
}

pub struct LaneRuntimeBinding {
    pub lane: LaneId,
    pub runtime_kind: RuntimeKind,
    pub runtime_instance: String,
    pub runtime_terminal_id: RuntimeTerminalId,
    pub runtime_agent_id: Option<String>,
    pub harness_session_id: Option<String>,
}

pub enum MultiplexerError {
    RuntimeUnavailable,
    TargetMissing,
    UnsupportedCapability(&'static str),
    CommandFailed(CommandFailure),
    InvalidOutput(ParseFailure),
    Timeout,
}
```

## Adapter Shapes

```rust
pub struct TmuxMultiplexer {
    pub bin: PathBuf,
    pub socket: Option<String>,
}

pub struct HerdrMultiplexer {
    pub bin: PathBuf,
    pub session: Option<String>,
}

pub struct ZellijMultiplexer {
    pub bin: PathBuf,
    pub session: Option<String>,
}
```

The first implementations can shell out to public CLIs and parse captured machine-readable output. A later implementation can use a runtime socket without changing Boop domain types or command output.

## Command Mapping

```text
boop beep lane list
  -> Multiplexer.snapshot
  -> join Boop lane metadata

boop beep ps
  -> Multiplexer.snapshot/process
  -> normalized pid, rss, cpu, uptime, children

boop beep pstree
  -> runtime containment topology
  -> join Boop parent-agent edges

boop beep hail
  -> Multiplexer.send or runtime-native agent prompt
  -> SendReceipt states whether input landed

boop beep lane wait
  -> Multiplexer.wait

boop beep lane create
  -> create Git worktree
  -> Multiplexer.create
  -> launch or detect harness
  -> store LaneRuntimeBinding

boop beep screen
  -> Multiplexer.read

boop beep viewport
  -> Multiplexer.viewport

boop beep state --explain
  -> runtime agent state and evidence
```

## Instance Timeline

1. `boop beep lane create` resolves the configured runtime kind and instance.
2. Boop creates the Git worktree and asks the adapter to create a terminal.
3. The adapter returns a runtime-native terminal ID. If it starts an agent directly, it also returns an agent ID.
4. Boop persists `LaneRuntimeBinding` before returning the creation receipt.
5. Later commands resolve the lane binding and invoke the same adapter.
6. The adapter executes one runtime operation and returns normalized records.
7. Boop joins runtime records with lane metadata, harness session identity, and SQLite history.
8. The Boop process exits. Terminal continuity remains owned by the selected runtime.
9. On a later invocation, Boop validates stored bindings against a fresh runtime snapshot and reports missing or replaced terminals explicitly.

## Storage, Reads, Writes, and Uniqueness

- Lane identity remains a Boop domain identity.
- Runtime terminal IDs remain opaque strings.
- Runtime instance identifies a tmux socket, Herdr session/socket, Zellij session, or equivalent namespace.
- The durable runtime key is `(runtime_kind, runtime_instance, runtime_terminal_id)`.
- Harness session IDs remain separate from runtime terminal IDs.
- Boop parent-agent edges remain separate from pane containment edges.
- Runtime topology and lifecycle state are sampled reads.
- Viewport samples remain ephemeral unless trace recording is enabled.
- Every persisted binding records creation time, last validation time, and last observed runtime revision when the runtime supplies one.
- Missing capabilities return typed absence or `UnsupportedCapability`; adapters do not fabricate equivalent values.

## Herdr 0.8 Source Analysis

Version inspected: `herdr 0.8.0`, protocol `19`, schema version `1`.

Repository: `https://github.com/herdrdev/herdr`

License: Apache-2.0.

### Package structure

Herdr 0.8 is one Rust binary crate:

- `src/main.rs` declares the complete module tree.
- No `src/lib.rs` exists.
- No Cargo workspace crates expose terminal, protocol, detection, or client libraries independently.
- Internal modules frequently use `pub(crate)`.
- `portable-pty` is patched to a vendored Herdr copy.
- Ghostty terminal bindings are included directly.
- Terminal state connects to application state, persistence, pane layout, input, server events, rendering, and agent detection.

Direct Cargo library reuse therefore requires source restructuring or a maintained fork. The public process and socket protocols provide a smaller adapter boundary.

### Socket protocol

Herdr exposes a local socket and a versioned JSON Schema:

```bash
herdr api schema --json
```

The Rust client performs this sequence:

```rust
let mut stream = connect(socket_path)?;
stream.write_all(serde_json::to_string(&request)?.as_bytes())?;
stream.write_all(b"\n")?;
stream.flush()?;
let response = read_one_json_line(stream)?;
```

This protocol is usable behind a future `HerdrMultiplexer`. Initial integration can shell out to the documented CLI JSON commands and avoid coupling to protocol internals.

### Operational capabilities observed

- Persistent background terminal server
- Workspace, tab, pane, and terminal identities
- Pane containment and layout rectangles
- Agent names and harness detection
- `idle`, `working`, `blocked`, `done`, and `unknown` states
- Monotonic `state_change_seq`
- Agent prompt and wait
- Visible, recent, recent-unwrapped, and detection output reads
- Terminal frame observation and control streams
- Output waits
- Pane process information
- Viewport coordinates
- Agent-state explanations with rule, region, evidence, priority, and manifest provenance
- Session restore integrations
- Remote attachment

### Controlled runtime trial

A temporary Herdr workspace named `boop-compare` created pane `w1:p1` and terminal `term_658b4297e9a2c1`.

Herdr started an interactive Codex agent named `compare-codex` in the Instant repository. It reported:

```json
{
  "agent": "codex",
  "agent_status": "idle",
  "interactive_ready": true,
  "name": "compare-codex",
  "pane_id": "w1:p1",
  "state_change_seq": 1
}
```

The trial submitted:

```text
Reply with exactly HERDR_BOOP_COMPARE_OK
```

Codex returned:

```text
HERDR_BOOP_COMPARE_OK
```

Herdr observed `idle -> working -> idle`; `state_change_seq` advanced from `1` to `3`.

The final pane viewport record was:

```json
{
  "max_offset_from_bottom": 91,
  "offset_from_bottom": 0,
  "viewport_rows": 23
}
```

Herdr detected Codex through its remote screen manifest even though the optional Codex integration was not installed. `agent explain` identified the matched rule, priority, input region, evidence, manifest path, and version.

### Trial against Boop

Boop did not report the Herdr pane through `boop beep ps` or `boop beep pstree`. The pane was outside tmux and had no Boop lane registration.

After `boop db sync create`, Boop independently ingested the Codex transcript written by the Herdr-launched process. It stored the prompt and response under session:

```text
019fec96-5cb6-7bf3-ac3c-778a49478735
```

The working shared boundary during this trial was the harness transcript filesystem:

```text
Herdr terminal
  -> Codex process
  -> Codex transcript files
  -> Boop transcript scanner
  -> Boop SQLite
```

Herdr and Boop had no direct runtime bridge during the trial.

### Herdr concepts to reproduce at the normalized boundary

- State authority belongs to one source per harness/runtime observation.
- Lifecycle detection reads the live bottom buffer independently from the user's scrolled viewport.
- Passive reads do not mark an agent as seen.
- Wait completion requires an observed lifecycle transition when requested after a prompt.
- Runtime output distinguishes visible, recent, unwrapped, and detection reads.
- State explanations preserve evidence and provenance.
- Viewport state uses semantic offsets from the bottom rather than screen pixels.
- Stable IDs join agent, pane, terminal, workspace, and harness session records.
- Protocol versions and output schemas are machine-readable.

## Proposed Boop Commands

```bash
boop beep mux list
boop beep mux capabilities [--runtime tmux|herdr|zellij]

boop beep screen <lane> \
  --source visible|recent|recent-unwrapped|detection \
  --lines 80 \
  --format text|ansi|ndjson

boop beep viewport <lane> --format ndjson

boop beep state <lane> --explain --format ndjson

boop beep wait <lane> \
  --until idle,done,blocked \
  --timeout 120s
```

## Transcript Minimap Composition

The minimap does not require Boop to own terminal buffers.

```text
Multiplexer viewport coordinates
  + multiplexer visible text
  + Boop SQLite transcript messages
  + message fingerprints and ordinals
  -> viewport-to-message match
  -> Instant Signal state
  -> bounded minimap rendering
```

The adapter supplies viewport and visible text when supported. Boop correlates those observations with stable transcript message IDs. Instant owns polling cadence, rendering, windowing, hover, selection, and highlighter interaction.

## Work Slices

### Slice 0: Inventory

- [ ] Count direct tmux command construction sites in Boop.
- [ ] Record every tmux output shape currently parsed.
- [ ] Identify call sites consuming lane, process, hail, and wait results.
- [ ] Inventory Zellij machine-readable commands and viewport fields.

### Slice 1: Types

- [ ] Add normalized runtime identity, terminal identity, topology, process, read, viewport, state, and receipt types.
- [ ] Add capability reporting and typed unsupported-capability errors.
- [ ] Add captured fixture tests before moving existing tmux behavior.

### Slice 2: Tmux adapter

- [ ] Move existing tmux shellouts behind `TmuxMultiplexer`.
- [ ] Preserve current `boop beep` output snapshots.
- [ ] Store tmux socket/session identity as runtime instance data.
- [ ] Add visible/recent/detection read semantics supported by tmux capture operations.
- [ ] Add viewport fields available from tmux formats and copy-mode state.

### Slice 3: Lane bindings

- [ ] Persist `LaneRuntimeBinding`.
- [ ] Resolve lanes through bindings rather than deriving runtime targets from lane names.
- [ ] Validate stale or replaced bindings on every relevant command.
- [ ] Preserve compatibility for existing registry rows during migration.

### Slice 4: Herdr adapter

- [ ] Add optional Herdr CLI discovery and version check.
- [ ] Parse Herdr JSON output from captured fixtures.
- [ ] Map agent list, pane process info, prompt, wait, read, viewport, and snapshot.
- [ ] Map Boop lane creation onto Herdr terminal and agent creation.
- [ ] Preserve Boop parent, goal, brief, branch, and worktree metadata.
- [ ] Add protocol compatibility reporting.

### Slice 5: Zellij adapter

- [ ] Add Zellij runtime identity and terminal targeting.
- [ ] Implement the capabilities supported by Zellij's public CLI.
- [ ] Return typed absence for viewport or semantic lifecycle fields that Zellij cannot report.

### Slice 6: Minimap inputs

- [ ] Add `boop beep screen` and `boop beep viewport` normalized records.
- [ ] Add exact visible-text-to-message matching.
- [ ] Add confidence and evidence fields.
- [ ] Add fixtures for wrapping, resizing, repeated text, truncated history, and alternate screens.

## Acceptance Fixtures

Each adapter should cover captured fixtures for:

- Empty runtime
- One shell terminal
- One idle agent
- Working agent
- Blocked agent
- Dead terminal
- Reused display name with a new stable ID
- Nested runtime topology
- Missing parent
- Visible read
- Recent unwrapped read
- Viewport at live bottom
- Viewport scrolled into history
- Input delivered
- Input target missing
- Wait transition
- Wait timeout
- Unsupported capability
- Runtime version mismatch

Tests should snapshot the complete normalized records and command receipts.

## Open Questions

- Which Boop SQLite table should own `LaneRuntimeBinding`, or should it remain in the operational lane registry?
- Should one Boop invocation address multiple runtime instances concurrently?
- How should Boop discover runtimes when tmux, Herdr, and Zellij are all active?
- Does `lane create` select a runtime from CLI input, repository configuration, or coordinator identity?
- Which runtime containment edges should be exposed separately from Boop parent-agent edges?
- What tmux fields provide stable viewport coordinates outside copy mode?
- Which Zellij APIs expose scroll position and recent unwrapped text?
- Should Herdr socket protocol support begin after the CLI adapter proves the normalized boundary?
