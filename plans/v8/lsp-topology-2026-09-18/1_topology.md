# LSP topology proposal

Status: proposal based on the archaeology in [0_archaeology.md](./0_archaeology.md). No part of this file is implemented or tested in the current compiler. The objective function is bounded resource use with deterministic workspace/revision identity, prompt cancellation, stale-result exclusion, and reuse of durable relational facts.

## The topology distinction

Shared wire property:

- An editor-spawned LSP process and a harness-spawned LSP process have the same JSON-RPC wire shape.
- Their lifetimes differ.

| surface | owner | lifetime | analysis state | external servers | close contract |
|---|---|---|---|---|---|
| harness-spawned LSP per session | one harness session | one session | session-local hot state; disk facts are shared read-only | child handles belong to the session | cancel requests, close child servers, stop watchers, drain writer, join workers, drop session state |
| shared live workspace broker | broker daemon | workspace lease | shared hot state partitioned by repo/revision and overlay | broker owns pooled or leased child servers | lease count reaches zero or idle deadline; fenced shutdown; all session subscriptions removed |
| language adapter inside compiler | compiler runtime | compiler or daemon lifetime | compiler-owned snapshots and facts | adapter owns a bounded child-server pool or no child server | compiler shutdown propagates cancellation and waits for adapter tasks |

Protocol boundary:

- LSP remains the editor protocol.
- MCP and ACP are harness or agent integration surfaces with different capability and lifecycle contracts.
- A harness can start an LSP child process.
- Starting an LSP child process does not make the harness integration an LSP server.
- The documented LSP claim is JSON-RPC between development tools and language servers.
- Harness-specific plugin capabilities require their own official documentation before use.

## Ownership partition

| object | hot or cold | owner | read/write access | identity | teardown |
|---|---|---|---|---|---|
| open document overlay | hot | LSP session | session writes; analyzer reads immutable snapshot | `(session, uri, version)` | delete on `didClose` and session crash cleanup |
| parsed CST and lowered file state | hot | workspace broker or adapter | one writer per file; readers use snapshot | `(repo, revision, path, content_hash)` | replace atomically; old snapshot drops after readers finish |
| cross-file semantic facts | warm | compiler/IVM store | compiler writes; LSP reads | `(repo, revision, fact_schema, source_digest)` | retract source unit or revision; retain disk policy decides cold retention |
| sqlite_ivm result and arrangement rows | cold durable | SQLite connection and virtual table | source tables write; result SELECT read | view name plus source schema and query definition | transaction rollback, source retraction, view drop through supported DDL |
| repo/import dependency tree | warm durable | compiler resolver | resolver writes; analysis reads | `(repo_id, revision_id, path)` | retract changed unit; preserve unchanged revision facts |
| external language-server process | hot external | session or broker lease | owner sends requests; child owns analysis | `(adapter, repo, revision, process_generation)` | cancellation, stdin close, wait/kill escalation, generation increment |
| watcher registration | hot external | broker workspace lease | watcher emits events to bounded ingress | `(workspace_root, watcher_generation)` | unregister and await callback task |
| request queue | hot | transport owner | producer submits; worker consumes | request id plus document version | bounded capacity, reject or coalesce, cancel on close |

Ownership summary:

- Compiler: analysis and durable facts.
- LSP: protocol framing, overlays, request IDs and response serialization.
- Shared broker: process pooling and watchers.
- Harness session: its lease and request stream.

## Proposed default partition

Proposed default:

- Thin per-client LSP transport.
- One shared broker and analysis owner per canonical workspace/revision family.
- sqlite_ivm durable cold facts.
- Explicitly leased external adapters.
- One per-session overlay and generation counter per client.
- Bounded broker request intake.
- Shared analysis snapshot owned by the broker.
- Publication limited to generation-matching answers.

| layer | shared across clients in one canonical workspace/revision family | session-local or explicitly leased |
|---|---|---|
| LSP transport | no. Each client has its own JSON-RPC stream, request IDs and response writer. | per-client stdio/socket transport and writer shutdown |
| workspace identity | yes. Canonical root, repository identity, revision identity and import resolution are shared. | a session selects the family and cannot mutate another family |
| parser/CST and lowered dependency tree | yes for immutable `(repo, revision, path, content_hash)` snapshots. Unchanged imported units are reused. | current unsaved overlay snapshot is session-local until committed |
| semantic analysis owner | yes. One broker `AnalysisHost` or equivalent owns the family snapshot and query cache. | request cancellation and generation fencing are per session |
| sqlite_ivm facts | yes. Source facts, import edges, diagnostics and derived views are durable cold state. | transaction handles and bounded result pages belong to the broker storage task |
| external language adapters | shareable only through an explicit lease keyed by adapter, repo, revision and process generation. | child process, stdin/stdout, pending requests, cancellation token and memory budget belong to the lease |
| editor overlays | no. Overlay bytes and open-document version are session-local. | `(session_id, uri, version)` and its generation |

Shareable dependency-tree layers:

- Canonical path resolution.
- Repository and revision identity.
- Import edges.
- Immutable parsed units.
- Lowered units.
- Persistent fact rows.
- Derived sqlite_ivm views.

Non-shareable by default:

- Unsaved document bytes.
- LSP request state.
- Response writers.
- Cancellation tokens.
- Pending request maps.
- Subscriber senders.
- Watcher registrations owned by one lease.
- External adapter process handles.

Sharing any non-shareable layer requires an explicit owner and lease protocol.

Default consequences:

- One analysis owner exists per identity family.
- One compiler engine is reused across LSP clients.
- Client isolation remains at the overlay and transport boundary.
- The documented v5 writer-channel exit failure remains separate from durable fact maintenance and future external adapter lifecycle.

## Hot and cold partitions

Hot state must have explicit bounds:

- open overlays: one current snapshot per `(session, uri)`;
- pending requests: at most `Q_request` per session and `Q_workspace` per workspace;
- semantic snapshot generations: current plus readers still executing;
- child processes: at most `P_adapter` per workspace and adapter kind;
- watcher roots: one registration per normalized root and generation;
- result pages: bounded by `limit` and server-side maximum `L_result`.

Cold state can live in SQLite:

- Source rows.
- Derived rows.
- Repo and revision identities.
- Import edges.
- Dependency facts.
- Diagnostic facts.
- Persistent index metadata.

sqlite_ivm can maintain derived views when the query fits its accepted SQL contract. It does not bound:

- Stored-row size.
- Prepared-statement caches.
- Rust maps.
- Child-process memory.
- Result serialization.

## Capability partition

| capability | LSP transport | compiler or broker owner | persistence |
|---|---|---|---|
| document sync | `didOpen`, `didChange`, `didClose`, versions | overlay and snapshot coordinator | current overlay hot; saved source cold |
| hover, completion, definition | request/response | semantic query facade | response transient; facts durable where useful |
| diagnostics | push or pull diagnostics | compiler facts and version fence | diagnostic rows durable; publication transient |
| semantic tokens and document symbols | request/response | CST/lowering surface | CST hot; symbols may be cold facts |
| external language server | LSP child transport | adapter lease | child memory hot; optional shard/index cold |
| staged compiler writes | code action, execute command, workspace edit | effect runtime and staging store | journal/staging policy decides durability |
| programmable hooks | custom LSP methods or compiler effects | compiler effect runtime | fact rows and staged effects are separate from result views |

Injected DSLs need a virtual-file or offset-map boundary for semantic features. Tree-sitter injections alone cover parsing and highlighting. The Volar pattern supplies virtual code plus source maps. The proposed adapter surface is:

```rust
trait DslGrammar { fn language() -> Language; }
trait Lower { type State; fn lower(node: Node, src: &[u8], ctx: LowerCtx) -> Result<Self::State, Diagnostics>; }
trait Surface { fn hover(&self, state: &State, offset: usize) -> Option<Hover>; }
```

Feature ownership:

- The lowerer owns semantic state.
- The surface turns state into one LSP response.
- A request carries the snapshot generation used to produce that response.

## Repo, revision and import-tree reuse

The cache key is structural rather than URI-only:

```text
WorkspaceKey = normalize(root)
RepoKey      = (WorkspaceKey, repo_identity)
RevisionKey  = (RepoKey, revision_identity)
UnitKey      = (RevisionKey, path, content_hash)
OverlayKey   = (session_id, uri, document_version)
```

Reuse and invalidation:

- An import edge is reusable when its source unit and resolved target identity remain equal.
- An edit replaces one `UnitKey`.
- The edit retracts facts owned by that source unit.
- Dependent views are rederived.
- A revision change creates a new `RevisionKey`.
- Retaining the old revision is a cold-storage policy, not a hot-map default.

Storage ownership:

- SQLite source tables represent the dependency tree.
- sqlite_ivm views expose derived dependency facts.
- The compiler resolver owns path normalization.
- The compiler resolver owns missing-import diagnostics.
- The compiler resolver owns revision identity.
- The IVM layer maintains rows after the compiler writes those facts.

## Queue, cancellation and stale-result fencing

```rust
type RequestId = u64;
type Generation = u64;

struct Request { id: RequestId, uri: Uri, generation: Generation, method: Method, params: Params }
struct Answer { id: RequestId, generation: Generation, payload: Payload }

async fn submit(req: Request) -> Result<Answer, QueueFull>;
async fn cancel(id: RequestId);
async fn apply_change(uri: Uri, version: i32, text: String) -> Generation;
async fn close(uri: Uri) -> Result<(), CloseError>;
```

Request controls:

- The ingress queue is bounded.
- `didChange` coalesces superseded versions before semantic work begins.
- A long request receives a cancellation token.
- The response path checks `(session_id, uri, generation)` against the current overlay before publishing.
- A response from generation 41 cannot publish after generation 42 becomes current.

The close sequence is ordered:

1. Mark the URI closed and increment its generation.
2. Cancel requests for the URI.
3. Remove the URI from the session overlay map and subscriber registry.
4. Retract or supersede the session-owned fact source.
5. Stop watcher callbacks owned only by the session.
6. Close child-server stdin and wait with a deadline. Escalate to kill when the deadline expires.
7. Drain or reject the transport writer and join worker tasks.

Crash and restart:

- Use the same generation fence.
- A new child process receives `process_generation + 1`.
- Results from the old child are discarded even if a late pipe read completes.

## Concrete timelines

### Request and edit

```text
t0  didOpen(uri, version=7) -> overlay[uri]=7 -> generation=11
t1  hover(id=1, generation=11) -> bounded worker starts
t2  didChange(uri, version=8) -> cancel(id=1), overlay[uri]=8, generation=12
t3  worker returns answer tagged generation=11
t4  fence compares 11 with current 12 -> discard answer
t5  hover(id=2, generation=12) -> query current snapshot -> publish
```

### Close and child server

```text
t0  session leases adapter(child_generation=4)
t1  didClose(uri) -> URI generation increments and adapter request cancels
t2  session subscriber is removed; watcher callback is unregistered
t3  child stdin closes; wait deadline starts
t4  child exits -> drop process handle and adapter state
t5  broker has zero leases -> idle timer or immediate workspace teardown
```

### Durable IVM retraction

```text
t0  compiler writes source fact for (repo=R, rev=V, path=p, digest=D1)
t1  sqlite_ivm trigger maintains import and diagnostic result rows
t2  edit creates digest D2
t3  compiler transaction deletes source rows owned by D1 and inserts D2
t4  sqlite_ivm propagates negative and positive maintenance inside the transaction
t5  LSP reads the committed view using a bounded SELECT and generation fence
```

Transaction and process boundaries:

- The database transaction gives atomic fact visibility.
- It does not cancel a request already holding an old row.
- It does not free request-worker memory.
- The v5 exit incident is transport retention.
- Subscriber and poll-thread `Sender` clones keep the writer channel alive.
- `finish_lsp` bypasses `IoThreads::join` (`v5/src/lsp.rs:362-377`).
- A future broker must close subscriber senders before joining I/O threads.
- The incident is separate from resident memory growth, persistent store size, and duplicated compiler or language-server engines.
- The recorded v5 36 GB resident-swap result requires its own retaining-path analysis.

## sqlite_ivm reuse boundary

Reusable storage boundary:

- A `rusqlite::Connection` is owned by one broker storage task or a disciplined connection pool.
- `extension::register` runs on every reader and writer connection.
- Writers enable `recursive_triggers=ON` and `trusted_schema=ON`.
- Source writes and supported source DDL occur in caller transactions.
- Results are read with ordinary `SELECT`.
- Result CRUD is rejected.

Recursive retraction:

- It is implemented in the crate's private `Plan::fixpoint` path.
- It is suitable when compiler facts fit ordinary SQLite tables and accepted recursive SQL.
- It is not an LSP event bus.
- The broker still needs a bounded notification channel, commit-to-generation mapping, cancellation tokens, child-process ownership and cleanup accounting.

## Options by defined objectives

| option | process count objective | hot-state sharing | external server reuse | isolation | measured status |
|---|---:|---|---|---|---|
| per-session LSP | one process per session | none across sessions | session-local | strongest | no benchmark in this report |
| shared workspace broker | one broker per workspace lease set | shared within repo/revision | pooled by adapter key | requires lease fences | no benchmark in this report |
| compiler adapter | compiler lifetime | shared with compiler snapshots | adapter-owned | compiler controls all state | no benchmark in this report |

Selection receipt:

- Selected shape: shared workspace broker with thin per-client transport and explicit adapter leases.
- Objective: bounded resource use, deterministic identity reuse, cancellation and teardown.
- Required measurements: latency, RSS, cache hit rate and close latency.
- Required workload dimensions: session count, workspace count, request rate, child-server memory and result cardinality.
- No optimality claim is made without those measurements.

## Acceptance probes for a future implementation

- Open, edit, close and reopen the same URI for 10,000 iterations. Report live handles, task count, queue depth, overlay rows and child processes after warmup.
- Submit hover requests faster than the worker can complete. Confirm queue capacity, cancellation count and zero stale publications.
- Kill an external language server during an in-flight request. Confirm generation increment, child reap and successful restart.
- Exit after subscriber and poll-thread shutdown. Confirm every writer `Sender` clone is dropped before `IoThreads::join`, and confirm zero hung LSP processes across the battery.
- Change one imported unit. Confirm only the unit-owned facts and dependent IVM rows retract and rederive.
- Switch repository revision. Confirm old revision rows are inaccessible to the current session unless explicitly selected.
- Drop a workspace lease while a watcher event and database commit race. Confirm unregister, transaction rollback or commit result, and no callback after teardown.

Boop-Status: done
Validation: proposal review against archaeology, sqlite_ivm source and upstream architecture sources complete
