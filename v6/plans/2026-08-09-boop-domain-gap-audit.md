# boop domain-gap audit for live Instant session views

Date: 2026-08-09

## Goal

Supply Instant with relational, incremental facts for a normal interactive tmux view beside an
evolving graph of the same session. The graph must support stable identities, historical state,
bidirectional terminal evidence, and reduced DL6 projections without reparsing transcripts in
the UI.

```text
Claude, OpenCode, tmux, processes, and control messages
  -> boop flat facts and resumable deltas
  -> DL6 derived session state and causal relations
  -> grapht revisions and projections
  -> Instant terminal plus graph workspace
```

## Existing boop surface

Implemented base facts include sessions, current liveness, parent-child edges, turns, touches,
spans, commands, fetches/searches, skills, PRs, and usage. The Rust library exposes snapshot
queries through `Store::query_status`, `Store::query_sessions`, `Store::query_facts`, and
`Store::usage_report`. The CLI exposes sync, follow, chat, `beep` control, and `db` analytics.

The Instant v2 contract was measured on 2026-08-09 against schema 5:

```text
1,689 sessions
230,953 turns
117,219 usage rows
Claude and OpenCode adapters
```

Source contracts:

- `NORTH-STAR-CODEGEN.md`
- `QUERY-SURFACE.md`
- `plans/boop-instant-v2-contract.md`
- `plans/boop-self-id-and-status.md`
- `plans/boop-tmux-visibility.md`
- `v6/boop/src/lib.rs`
- `v6/boop/examples/instant_views.rs`

## Required boop additions

### 0. Public record and delta identity

Instant and boop need a shared identity for the same harness record. Session and dense turn are
already exposed; transcript byte offsets and record IDs remain internal or absent from public
query rows.

```rust
pub struct FactCursor {
    pub harness: HarnessId,
    pub session: SessionId,
    pub transcript: TranscriptId,
    pub byte_offset: u64,
    pub record_id: RecordId,
    pub turn: u64,
    pub timestamp: u64,
}
```

The cursor is the resume, deduplication, acknowledgement, and terminal-evidence coordinate.
Every streamed row carries it. Snapshot rows expose enough of it to reconnect to a stream.

### 1. Linked-library delta stream

`boop follow` is a coarse-poll CLI path. The Rust library exposes snapshot queries and no typed
delta iterator or subscription for an embedded Instant host.

```rust
pub enum BoopDelta {
    Session(SessionRow),
    Turn(TurnRow),
    Fact(FactRow),
    Status(StatusRow),
}

pub trait DeltaSource {
    fn read_after(&self, cursors: &mut Cursors) -> anyhow::Result<Vec<BoopDelta>>;
}
```

The implementation reuses per-transcript sync cursors and byte-offset tailing. It emits bounded
batches. The host selects its scheduler, debounce, and cancellation behavior.

### 2. Historical liveness intervals

`agent_live` is a current-state cache. Overwrite semantics cannot answer when an agent became
live, idle, dead, or changed state within a window.

```sql
CREATE TABLE agent_live_span (
  session_id   INTEGER NOT NULL,
  from_ts      INTEGER NOT NULL,
  to_ts        INTEGER,
  status_id    INTEGER NOT NULL,
  pid          INTEGER,
  tmux_pane_id INTEGER,
  PRIMARY KEY (session_id, from_ts)
) WITHOUT ROWID;
```

An observation closes the open interval only when state changes. `agent_live` remains the
current-state cache. `agent_live_span` supplies history and revision playback.

### 3. Historical tmux visibility intervals

The graph needs to know which tmux session, window, and pane was visible to the human at a given
time. `v6/boop/src/tmux.rs` contains a control-mode client; the researched `agent_visible`
schema and persistent subscription fold have not landed in the store.

Required evidence includes:

- client attach and detach;
- client session changes;
- active window and active pane changes;
- pane focus intervals;
- pane, lane, and agent tags;
- viewport top and bottom record identity when Instant can supply them.

Visibility uses the same `[from_ts, to_ts)` interval rule as liveness. A query at time `T` uses
`from_ts <= T AND (to_ts IS NULL OR to_ts > T)`.

### 4. Typed public query rows

The public library currently returns `Vec<serde_json::Value>`. Add stable Rust rows while
retaining JSON serialization at the CLI boundary:

```rust
pub struct SessionRow { /* flat scalar columns */ }
pub struct StatusRow { /* flat scalar columns */ }
pub struct TurnRow { /* flat scalar columns */ }
pub struct TouchRow { /* flat scalar columns */ }
pub struct CommandRow { /* flat scalar columns */ }
pub struct FetchRow { /* flat scalar columns */ }
pub struct EdgeRow { /* flat scalar columns */ }
pub struct UsageRow { /* flat scalar columns */ }
```

Rows remain flat and declarative for direct mapping to relations and later DL6 code generation.

### 5. Transcript and terminal evidence correlation

Bidirectional selection needs this chain:

```text
fact cursor
  -> harness record ID
  -> transcript byte range
  -> session and turn
  -> terminal or rendered-message range
```

Boop owns transcript evidence. Instant owns its terminal viewport and rendered message bounds.
The shared record ID joins them without text matching.

### 6. Historical control relations

`agent_edge` records parent-child relations. Its primary key collapses repeated actions. Add
temporal/count evidence where repeated hail, result, retry, resume, and cancellation operations
matter:

```text
first_ts
last_ts
n
```

This distinguishes one structural spawn edge from repeated communication across that edge.

### 7. Completion, blockage, and message-state evidence

The reduced view needs active, changed, blocked, completed, and failed states. Preserve raw
events in boop when a harness or control mechanism states them:

```text
agent_started
agent_idle
agent_blocked
agent_resumed
agent_completed
agent_failed
agent_cancelled

message_sent
message_delivered
message_observed
message_acknowledged
message_answered
message_expired
```

DL6 derives higher-level state from these facts, liveness intervals, tool results, and process
exits. Rust does not hard-code the reduction policy.

### 8. Shell-only lanes and hand-started sessions

A registered tmux shell lane without a Claude transcript or OpenCode session appears through
`boop beep lane list` and remains absent from `boop db session list`. Give registry lanes a flat
base relation so database consumers can join them instead of performing a client-side union.

Sessions without a registry route currently report `state=unknown`. Correlate later transcript
records, tmux pane identity, PID evidence, and process cwd while retaining the source and
confidence of each identity observation.

### 9. Missing working directories

The measured Instant contract found 288 of 1,369 Claude sessions without `cwd`. Candidate
evidence sources are later transcript records, tmux pane current path, process cwd, route data,
and worktree metadata. Preserve observations rather than silently replacing provenance.

### 10. Canonical verbs with raw evidence

Harnesses emit spellings such as `Read`/`read`, `Write`/`write`, and `Edit`/`edit`. Preserve the
raw verb and add a canonical verb so every consumer does not repeat normalization.

```text
raw_verb = "Write"
verb = "write"
```

### 11. Network activity inside shell commands

`agent_fetch` records native fetch and search tools. URL-bearing shell commands remain outside
that table. The measured sample contained 119 such Bash commands across 120 transcripts.
Preserve `agent_cmd` as ground truth and derive shell-network claims in DL6:

```prolog
shell_network(Session, Turn, Program, Target) <-
  agent_cmd(Session, Turn, _, Program, Argline),
  network_command(Program),
  extract_target(Argline, Target).
```

### 12. File evidence granularity

`agent_touch` means an agent tool call touched a path. It does not mean filesystem mtime or a
non-agent edit. Where harness evidence permits, retain:

```text
read span
written span
patch before span
patch after span
revision before
revision after
```

Symbol and AST extraction remain in `sprefa-extract`; boop stores the agent-event evidence that
points at those artifacts.

## Required DL6 additions

### 13. Boop base-fact bridge

The north star requires a one-to-one mapping from flat boop rows to relations. Implement a
SQLite or streaming source adapter that declares and loads the base relations without consumer
transcript parsing.

```text
boop SQLite rows
  -> DL6 base relations
  -> derived session facts
  -> query output
  -> grapht
```

### 14. Derived session digest

These relations belong in DL6:

```text
active_session
blocked_session
recent_touch
changed_artifact
causal_edge
unanswered_message
expensive_subtree
visible_agent
session_phase
```

Rules retain evidence IDs so every digest row can expand back into source facts and terminal
ranges.

## Domain boundaries

| Concern | Owner |
|---|---|
| cross-harness capture, transcript identity, tmux control, relational storage | boop |
| derived state, classification, reduction, causal rules | DL6 |
| symbol, AST, type, and source structure | sprefa-extract |
| graph revisions, placement, transitions, projections | grapht |
| tmux UI, dock composition, camera, linked selection | Instant |
| non-agent filesystem changes | filesystem and Git scanner |

## Dependency order

```text
0. public record and cursor identity
1. typed public rows
2. linked-library delta stream
3. liveness intervals
4. tmux visibility intervals
5. transcript-to-terminal evidence join
6. temporal control and message facts
7. shell-lane and hand-started-session coverage
8. DL6 base-fact bridge
9. derived active, blocked, changed, visible, and causal rules
10. grapht and Instant consumption
```

## Verification receipts required per slice

- Fixture tests for Claude and OpenCode use the existing corpus convention.
- Tmux tests use unique throwaway sockets and remove their servers and socket files.
- Delta tests prove resume without duplication or skipped records.
- Interval tests prove adjacent state changes, repeated identical observations, open intervals,
  and historical point queries.
- Cross-harness tests prove canonical verbs while retaining raw spellings.
- Library examples consume typed rows and deltas without importing binary internals.
- DL6 fixtures prove every derived row retains base-fact evidence identity.
- Idle following reports CPU, RSS, database writes, and wake count.
- No UI, graph layout, SVG, HTTP, or Instant-specific entry point enters the boop crate.

## Existing documented gaps preserved

| Gap | Effect |
|---|---|
| 288 of 1,369 sampled Claude sessions lack `cwd` | project grouping drops them |
| no registry route | hand-started liveness remains `unknown` |
| shell lane without harness store | absent from database session queries |
| URL inside Bash command | absent from `agent_fetch` |
| `agent_touch` records tool activity | non-agent filesystem edits remain outside boop |
| `agent_live` overwrites current state | death or state change within a time window is unanswerable |
| `agent_edge` key collapses repeated actions | communication frequency and retry history disappear |
| snapshots return `serde_json::Value` | public schema errors appear at runtime |
| follow is a CLI poll loop | embedded Instant host lacks a typed delta surface |

## Implementation review: `lane/boop-rows` at `b92c4f74`

Reviewed on 2026-08-09 after a feedback-driven implementation run. The commits are on
`lane/boop-rows`; they were not present on the reviewing checkout
`docs/boop-help-doctrine` or `origin/main` at review time.

### Verification

The branch was tested from a detached clean worktree. Tmux tests ran outside the filesystem
sandbox so their unique throwaway sockets could be created.

```text
library tests: 85 passed
binary tests:   1 passed
bench tests:    2 passed
total:         88 passed
cargo clippy -- -D warnings: passed
```

### Audit items implemented

| Audit item | Commit | Observed implementation |
|---|---|---|
| temporal agent edges | `30b673b5` | `first_ts`, `last_ts`, and `n`; upsert and typed/query rows |
| historical liveness spans | `e93ee77f` | schema, observation fold, current cache, interval query, point query |
| canonical verbs with raw evidence | `e7530ce1` | lowercase canonical verb and original harness spelling in `agent_touch` |
| typed public rows | `137735c1` | session, status, turn, touch, command, fetch, edge, and usage rows |
| public `FactCursor` | `eb6b0e2f` | public type and per-transcript `query_cursors()` surface |

Additional commits accumulate Codex and Kimi same-turn usage snapshots instead of replacing
them, format the resulting code, and make `boop beep ps` hide dead routes unless addressed by
name or requested through `--all`. Schema version moved from 5 to 6.

### Production-complete path: canonical verbs

The ingestion path is connected:

```text
raw tool name
  -> lowercase canonical verb
  -> agent_touch.verb_id
  -> original spelling in agent_touch.raw_verb_id
  -> TouchRow { verb, raw_verb }
```

The acceptance test writes mixed `Read` and `read` evidence, observes one canonical `read`, and
retains both raw spellings.

### Partial path: `FactCursor`

`FactCursor` has the requested fields:

```rust
pub struct FactCursor {
    pub harness: String,
    pub session: String,
    pub transcript: String,
    pub byte_offset: u64,
    pub record_id: String,
    pub turn: u64,
    pub timestamp: u64,
}
```

`query_cursors()` fills harness, session, transcript path, and byte offset from `sync_cursor`.
It currently emits these placeholders:

```text
record_id = ""
turn = 0
timestamp = 0
```

Per-record population and a delta stream remain open. Snapshot turns already carry turn and
timestamp, but no public operation joins those values to transcript byte offsets and record IDs.

### Partial path: historical liveness

The storage and fold are implemented and tested:

- state changes close the open interval and open another;
- repeated identical observations insert nothing;
- `agent_live` remains the current-state cache;
- historical point queries use the half-open interval predicate.

`record_status()` has no production call site outside tests. Sync, follow, tmux observation, and
process observation do not currently populate `agent_live_span`.

```text
remaining wire:
  tmux/process observation
    -> Store::record_status(...)
```

### Partial path: temporal edges

The store can accumulate repeated edge observations. Production sync calls `add_edge()` for
`spawned`. No hail, result, retry, resume, or cancellation path calls `add_edge_at()`.

```text
production-populated edge kind: spawned
test-populated edge kind:       hail
```

The columns and API are ready for control-message producers. Communication frequency does not
enter the store until those producers call it.

### Partial path: typed rows

Implemented public rows:

```text
SessionRow
StatusRow
TurnRow
TouchRow
CommandRow
FetchRow
EdgeRow
UsageRow
FactCursor
```

`StatusRow` currently omits fields named by the Instant contract:

```text
lane
state
pid
tmux_pane
rss_kb
cpu_pct
uptime_sec
first_seen_ts
last_seen_ts
died_ts
```

`live_span()` and `query_live_at()` still return `Vec<serde_json::Value>`; `LiveSpanRow` does not
exist yet.

### Updated remaining dependency order

```text
0. wire record_status into real tmux/process observations
1. populate FactCursor record_id, turn, and timestamp per emitted record
2. expose a bounded linked-library delta stream
3. wire hail/result/retry/resume/cancel events into temporal edges
4. add LiveSpanRow and complete StatusRow
5. implement tmux visibility intervals
6. add transcript-record to terminal-viewport correlation
7. expose boop base facts to DL6
8. derive active, blocked, changed, visible, and causal relations
9. feed grapht and Instant projections
```
