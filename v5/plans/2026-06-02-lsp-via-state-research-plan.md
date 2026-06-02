# LSP-via-state: the reactive/redux model as the LSP implementation mechanism

Date: 2026-06-02. Status: RESEARCH PLAN (no code). Companion to
[programmable-LSP arc](2026-06-01-programmable-lsp-ops-frontend-arc.md) Phase C
and [kill-where / seed-closures](2026-06-02-kill-where-seed-closures-by-nesting.md).

## The one model (state, not coroutines)

LSP is the first *interaction app* on the existing reactive core. The mapping:

| redux term | sprefa term | code today |
|---|---|---|
| store | facts in SQLite (`_strings`/`_where_bytes`/`_file` + declared rels) | `db.rs`, `engine.rs` |
| action | one editor event (didChange / definition req / …) | `lsp.rs` message loop |
| reducer | a rule (lowered to the SQL fixpoint) | `engine.rs` `rebuild_derived` |
| subscriber | LSP publish / response render | `lsp.rs` `publish` |
| **dispatch** | **THE TICK** (`tick` / `tick_paths`) | `engine.rs:702` / `:798` |

An *async / pausable* LSP interaction is a persisted **state row** that rules
ADVANCE one step per tick, e.g. `request(id, kind, status, …)`. It is NOT a
coroutine, continuation, future, or parked-wake. The request enters the store as
a fact; rules derive its answer; the subscriber drains answers; the tick is
driven only by the editor message that delivered the action.

### Hard invariant: NO RULE MAY SCHEDULE A TICK

Every tick is external (an editor notification or request). A rule that
self-schedules a wake rebuilds v4's durable parked-wake queue, rejected. The
plan keeps the tick INLINE on the message loop (no worker thread) as long as
each tick stays sub-100ms (Phase B incremental closures, already landed via
`refresh_cond_cache` digest-skip, makes that true). Any feature that *appears*
to need self-scheduling is re-expressed as a state row advanced by the next
external tick, or flagged as a risk (see §Risks).

---

## What exists today (cite)

**LSP loop** (`lsp.rs:23-75`): parse `.dl`, drop `?` queries (`lsp.rs:29`, their
stdout would corrupt the protocol channel), `Engine::new`, declare caps with
`TextDocumentSyncKind::NONE` (`lsp.rs:37`) and save-only (`lsp.rs:38`). Cold
full `tick` (`lsp.rs:46`), publish all. Then a blocking `for msg in
&connection.receiver` (`lsp.rs:51`):
- `Message::Request` → only `handle_shutdown` (`lsp.rs:54`); no definition /
  references / hover handler exists.
- `Message::Notification` → didSave / didOpen extract one abs path
  (`lsp.rs:57-63`), call `eng.tick_paths(&prog, &[abs], true)` (`lsp.rs:65`),
  then `publish` for that file (`lsp.rs:66`). didChange is dropped.

The tick is INLINE here already (`lsp.rs:65`), satisfying the invariant by
construction today.

**`diag` → diagnostics** (`engine.rs:656-698`, `lsp.rs:80-120`): `diag` is an
ordinary declared relation, populated by the fixpoint like any other
(`lib.rs:48-49` notes "the `diag` relation is just a relation; LSP is one
renderer"). `Engine::diags(only: Option<&str>)` maps rows by column NAME (path,
line, col, end_line, end_col, severity, code, msg, hint; `engine.rs:679-689`),
optionally filtered to one path. `publish` groups by path, always re-publishes
the ticked file even with zero rows so a fixed lint clears (`lsp.rs:84`).
`--check` (`lib.rs:50-77`) is the CLI renderer of the same relation;
`--diag-json` emits JSON (`lib.rs:60-66`).

**Tick model**:
- `tick` (`engine.rs:702`) = full: declare, `reconcile_sources` (walks/stats the
  tree), refresh builtins/module/type/scip/spine rels, `rebuild_derived` +
  `rebuild_closures` if any source fact changed (`engine.rs:768-771`),
  `refresh_cond_cache`, seed-closure rules, queries.
- `tick_paths` (`engine.rs:798`) = incremental: reconciles ONLY the passed paths
  (`engine.rs:857-905`, never walks the tree), digest-prunes source rels whose
  bytes moved but rows didn't (`engine.rs:911-913`, `prune_unchanged_by_digest`
  at `:1110`), then rebuilds ONLY the derived rels dependency-reachable from what
  changed (`affected_derived`, `engine.rs:960-972`). Cold/empty falls back to
  full (`engine.rs:950`).
- Cross-tick closure cache: `closure_cache: HashMap<String, ClosureCache>`
  persists on `Engine` across ticks (`engine.rs:501`). `refresh_cond_cache`
  (`engine.rs:2031`) recondenses an edge ONLY if it is dirty AND its content
  digest moved (`engine.rs:2038-2045`); a comment edit ⇒ 0 recondensations
  (`recondensed` counter, `engine.rs:484`). This is what makes a per-keystroke
  tick fast.

**Coordinate types** (`ast.rs:3-4`): `Type { Text, Int, Path, File, Dir, Repo,
Rev }`. The ref-spine adds byte spans: `ref(id, string, file, lo, hi)` is a view
over `_where_bytes ⋈ _strings` (`engine.rs:48-54`, `spine_rel_decls` at
`engine.rs:98-104`); `id` is the `_where_bytes` id = the rewrite coordinate.
`located_spans()` (`engine.rs:613`) returns `(path, lo, hi, text)` for every
located span (the `--move` feed). `string(id, text, norm)` resolves an interned
StringId.

**Seeded closures from a rule body** (landed): `closure_seed_of`
(`engine.rs:217`) classifies a derived rule whose body reads a 2-ary closure
head with one endpoint pinned to a literal; `eval_closure_seed_rule`
(`engine.rs:2084`) walks the condensation (`scc::reaches_from` /
`reached_by`, `engine.rs:2121`) and writes the head table with one
`insert_rows` (`engine.rs:2130`, no N+1). A literal head term on a closure query
also seeds (`run_reaches_point`, `engine.rs:2057`; `run_query` dispatch at
`engine.rs:2138-2148`). **find-references and blast-radius are therefore already
expressible** by pinning one endpoint — no new closure machinery needed.

**Edit sink** (`refactor.rs`): `Edit { path, lo, hi, old_text, new_text }`
(`refactor.rs:14`), `splice_file` applies edits DESC-by-lo with overlap
rejection (`refactor.rs:26`). `--move OLD=NEW` already drives this from
`located_spans` + `rspath::rewrite_import`. This is the rename backing.

---

## LSP capability → backing facts (the map)

| capability | LSP message | backed by | new state? |
|---|---|---|---|
| publishDiagnostics | didOpen/didChange | `diag` relation (`engine.rs:656`) | no (live: needs didChange buffer) |
| go-to-definition | `textDocument/definition` (request) | `def(name,path,line)` ⋈ `ref` at cursor span | request row + answer rel |
| find-references | `textDocument/references` (request) | seeded closure / `ref(id,string,file,lo,hi)` (`engine.rs:48`) | request row + answer rel |
| hover | `textDocument/hover` (request) | `ref` ⋈ `string` / `diag` at cursor byte | request row + answer rel |
| rename | `textDocument/rename` (request) | edit sink (`refactor.rs`) + `ref` spans | request row + edit-answer rel |

The unifying shape: a **request is a fact**; its **answer is a derived
relation** keyed by request id; the tick that ingested the request also derived
the answer (synchronous query response). No request is ever "parked".

---

## 1. Type signatures

### State-row relations (the store, declared as builtins like `file`/`ref`)

```
# An open editor buffer's current text, addressed by path. RAM-truth overlay on
# the disk-truth `_file` cache. One row per open doc; didChange REPLACES it.
rel doc(path:path, version:int, content:text).

# A point request the editor dispatched this tick. status advances across ticks
# ONLY if a multi-tick answer is ever needed (today: answered same tick, then
# retracted). kind ∈ definition|references|hover|rename. byte = cursor offset.
rel request(id:text, kind:text, path:path, byte:int, arg:text, status:text).

# The answer relation(s), keyed by request id. A reducer rule fills these from
# request ⋈ (ref/def/closure/diag). The subscriber drains by id, then the row is
# retracted (request lifetime = one tick).
rel answer_loc(id:text, path:path, lo:int, hi:int).      # def / references / rename targets
rel answer_text(id:text, value:text).                    # hover markdown
rel answer_edit(id:text, path:path, lo:int, hi:int, new_text:text).  # rename
```

`doc`, `request`, and the `answer_*` rels are declared the same way as `repo` /
`file` / `ref` (the builtin `RelDecl` path, `engine.rs:56-104`). They are
ordinary tables; the tick reads and writes them like any source/derived rel.

### Rust handler + driver signatures (`lsp.rs`)

```rust
// In-memory open buffers. RAM-truth overlay; mirrors `doc` rows into the store.
struct DocStore { open: HashMap<PathBuf, (i32 /*version*/, String /*text*/)> }
fn DocStore::apply_change(&mut self, uri: &Uri, ver: i32,
                          changes: &[lsp_types::TextDocumentContentChangeEvent]);
// pseudo: for each change, splice the range edit into open[path] (or replace on
//   full-sync). Bump version. This is the ONLY mutable buffer state; it is the
//   action payload, not a scheduler.

// The single dispatch: turn ONE editor message into ONE inline tick.
// kind=Notification(didChange/didSave/didOpen) -> mark dirty path, tick_paths.
// kind=Request(definition/references/hover/rename) -> insert request row,
//   tick_paths over [path], read answer_* by id, send Response, retract request.
fn handle_message(eng: &mut Engine, prog: &Program, docs: &mut DocStore,
                  conn: &Connection, root: &Path, msg: Message) -> Result<()>;
// pseudo:
//   match msg {
//     Notification(didChange) => { docs.apply_change(..); upsert doc row;
//                                  eng.tick_paths(prog, [path], true)?;       // INLINE
//                                  publish(conn, eng, root, Some(path))?; }
//     Request(req) if req.method == "textDocument/definition" => {
//       let id = req.id; let (path, byte) = pos_to_byte(docs, params);
//       eng.set_request(id, "definition", path, byte, "")?;   // INSERT request row
//       eng.tick_paths(prog, [path], true)?;                  // INLINE; reducer fills answer_loc
//       let locs = eng.answer_locs(id)?;                       // SELECT answer_loc WHERE id
//       conn.sender.send(Response{ id, result: to_lsp_locations(locs) })?;
//       eng.clear_request(id)?;                                // retract request + its answers
//     }
//     ... references / hover / rename symmetric ...
//   }

// Cursor (line,char) in the open buffer -> byte offset (ref-spine lo/hi are bytes).
fn pos_to_byte(docs: &DocStore, path: &Path, pos: Position) -> Option<u32>;

// Locate what the cursor is on: the ref-spine span (or def/diag) covering `byte`.
// READS only; no schedule. Backs hover + go-to-def's "what symbol am I on".
fn Engine::locate_at(&self, path: &str, byte: u32) -> Result<Vec<LocatedRow>>;
// pseudo: SELECT id, string, lo, hi FROM ref WHERE file = <content-id-of(path)>
//   AND lo <= byte AND byte < hi  (index _where_bytes_file_span_idx, engine.rs:1017).
//   NOTE ref.file is a content FileId, not a path (per CLAUDE.md ref-spine note);
//   resolve path -> content id via _file before the join.

struct LocatedRow { ref_id: String, string_id: String, lo: u32, hi: u32, text: String }
```

### Engine seam additions (`engine.rs`) — request/answer as plain rows

```rust
// Insert one request fact (one INSERT, the store mutation for this action).
fn Engine::set_request(&self, id:&str, kind:&str, path:&str, byte:u32, arg:&str) -> Result<()>;
// Drain answer rows for a request id (the subscriber read).
fn Engine::answer_locs(&self, id:&str) -> Result<Vec<(String,u32,u32)>>;
fn Engine::answer_text(&self, id:&str) -> Result<Option<String>>;
fn Engine::answer_edits(&self, id:&str) -> Result<Vec<(String,u32,u32,String)>>;
// Retract the request and its answers after responding (request lifetime = 1 tick).
fn Engine::clear_request(&self, id:&str) -> Result<()>;
```

No new tick entry point. Requests ride `tick_paths` (`engine.rs:798`) over the
single touched path — the reducer rules that read `request` are ordinary derived
rules (`request` is a source/builtin rel; the answer rels are their heads).

---

## 2. Pseudo-code bodies (reducer rules, authored in `.dl`)

These are NORMAL rules; the engine already evaluates them. They are written in
the lint/program `.dl`, not in Rust.

```
# go-to-definition: the def whose name matches the symbol under the cursor.
answer_loc(id, dpath, dlo, dhi) <-
    request(id, "definition", path, byte, _, _),
    ref(_, sid, fpath, lo, hi),  fpath = path, lo <= byte, byte < hi,   # symbol at cursor
    string(sid, name, _),
    def(name, dpath, dline),                                            # its definition
    ref(_, dsid, dpath, dlo, dhi), string(dsid, name, _).               # def's span

# find-references: every ref to the symbol under the cursor (NO closure needed).
answer_loc(id, rpath, rlo, rhi) <-
    request(id, "references", path, byte, _, _),
    ref(_, sid, path, lo, hi), lo <= byte, byte < hi, string(sid, name, _),
    ref(_, sid2, rpath, rlo, rhi), string(sid2, name, _).

# find-references, transitive (blast radius): seeded closure from the symbol.
# Already expressible — pin the symbol as the closure seed (engine.rs:2084).
answer_loc(id, rpath, rlo, rhi) <-
    request(id, "references", _, _, sym, _),               # sym = literal symbol arg
    reaches(sym, dep),                                      # seeded BFS, dst free
    ref(_, sid, rpath, rlo, rhi), string(sid, dep, _).

# hover: the diag (or symbol text) at the cursor.
answer_text(id, msg) <-
    request(id, "hover", path, byte, _, _),
    diag(path, line, col, _, _, _, _, msg, _), /* line/col→byte containment */ .

# rename: every ref to the symbol becomes an edit to the new name.
answer_edit(id, rpath, rlo, rhi, newname) <-
    request(id, "rename", path, byte, newname, _),
    ref(_, sid, path, lo, hi), lo <= byte, byte < hi, string(sid, name, _),
    ref(_, sid2, rpath, rlo, rhi), string(sid2, name, _).
```

The Rust side never computes answers; it inserts the request fact, ticks, reads
the answer rows, sends the Response, retracts. The tick IS the dispatch.

---

## 3. Instance lifetimes

| holds state | scope | writer | notes |
|---|---|---|---|
| `Engine` (`engine.rs:476`) | whole LSP session | `tick`/`tick_paths` | single DB owner; never moves to a thread (invariant) |
| `closure_cache` (`engine.rs:501`) | across ticks | `refresh_cond_cache` | the perf cache; requests READ it via seeded closures, never recondense |
| SQLite DB (`db.rs`) | session (or persisted `--db`) | the tick | store of all facts incl. `doc`/`request`/`answer_*` |
| `DocStore` (new, `lsp.rs`) | session | didChange handler | RAM-truth buffers; mirrored into `doc` rows |
| `doc` rows | until didClose | didChange/didOpen tick | one per open file |
| `request` rows | **one tick** | `set_request` → `clear_request` | NEVER persisted across ticks (that would be a parked wake) |
| `answer_*` rows | one tick | reducer rule | drained then retracted with the request |

The request-row lifetime of exactly one tick is the load-bearing discipline
(plain word: the critical discipline) that keeps this out of v4's parked-wake
queue: a request is born, answered, and retracted inside the single external
tick its message triggered.

---

## 4. Storage layout → read/write sequence → uniqueness

### Storage
- `doc`, `request`, `answer_loc`, `answer_text`, `answer_edit`: declared tables
  (builtin `RelDecl` path). No new meta tables; reuse `_strings` /
  `_where_bytes` / `_file` for spans and content.
- PK / uniqueness: `doc(path)` unique; `request(id)` unique; `answer_*` keyed by
  `(id, …)`, dedup via `INSERT OR IGNORE` (matches existing `insert_rows`).

### Sequence per LSP message type

**didChange / didSave / didOpen** (notification → `tick_paths`):
1. `DocStore::apply_change` (or read disk on save).
2. UPSERT `doc(path, version, content)`.
3. `eng.tick_paths(prog, [path], true)` — INLINE. Reconciles the one path
   (`engine.rs:857`), digest-prunes (`engine.rs:912`), rebuilds affected derived
   (`engine.rs:960-972`), refreshes cond cache (skips unchanged edges,
   `engine.rs:980`), runs seed rules, runs queries (dropped in LSP mode).
4. `publish(conn, eng, root, Some(path))` — read `diag` for that path, send
   publishDiagnostics (`lsp.rs:80-95`).

**definition / references / hover / rename** (request → `tick_paths`):
1. `pos_to_byte` (line/char → byte via the `doc` buffer).
2. `set_request(id, kind, path, byte, arg)` — one INSERT.
3. `eng.tick_paths(prog, [path], true)` — INLINE. The `request`-reading reducer
   rules are affected-derived (they read the just-written `request` row), so they
   re-derive; answer rows land in `answer_*`.
4. `answer_locs(id)` / `answer_text(id)` / `answer_edits(id)` — SELECT.
5. Send `Response { id, result }`.
6. `clear_request(id)` — DELETE the request + its answers.

### Uniqueness conditions
- A request id is unique per editor request (LSP guarantees); `clear_request`
  retires it so ids never accumulate.
- `doc` is the single RAM-truth per path; the tick prefers `doc.content` over
  disk for an open file (the deferred "unsaved-buffer support" from `lsp.rs:3-5`
  — this plan implements it as the `doc` overlay rel).
- Seeded closures stay O(reachable subgraph), never O(V²) (`engine.rs:2118-2128`).

---

## Phased build sequence

Each phase: state rows, tick wiring, LSP message, no-self-schedule check.

**Phase 1 — live diagnostics (didChange).** State: `doc(path,version,content)`.
Wiring: declare `change: INCREMENTAL`, add `DocStore`, on didChange upsert `doc`
+ `tick_paths([path])` + `publish`. Engine reads `doc.content` over disk for open
files (implements the deferred RAM-truth overlay, `lsp.rs:3-5`). Message:
didChange. No-schedule check: tick is the existing inline call (`lsp.rs:65`); a
debounce, if added, is a TIMER in the Rust loop, NOT a rule wake — it only
coalesces external actions. PASS.

**Phase 2 — go-to-def + find-refs (already reachable).** State: `request`,
`answer_loc`. Wiring: handle `definition`/`references` requests; insert request,
inline `tick_paths`, drain `answer_loc`, respond, clear. Reducer rules join `ref`
⋈ `def`; transitive refs use the LANDED seeded-closure body-read
(`engine.rs:2084`, pin the symbol). Message: `textDocument/definition`,
`textDocument/references`. No-schedule check: request born + answered + retracted
in one external tick; closure walk READS `closure_cache`, never recondenses
(`engine.rs:2118`). PASS. (Capability gap: needs a `def(name,path,line)` rel and
a `ref`-at-cursor join, both authorable in `.dl` today.)

**Phase 3 — hover.** State: `request(kind="hover")`, `answer_text`. Wiring:
`locate_at(path, byte)` (`_where_bytes_file_span_idx`, `engine.rs:1017`) plus a
reducer joining `diag`/`string` at the cursor; respond with markdown. Message:
`textDocument/hover`. No-schedule check: pure read + one inline tick. PASS.

**Phase 4 — rename via the edit sink.** State: `request(kind="rename", arg=new)`,
`answer_edit`. Wiring: reducer emits `answer_edit(id,path,lo,hi,new)` from `ref`
spans; Rust folds rows into a `WorkspaceEdit` (reuse `refactor::Edit` /
`splice_file` shape, `refactor.rs:14-43`). Message: `textDocument/rename`.
No-schedule check: editor applies the WorkspaceEdit and sends back didChange,
which is the NEXT external tick — the rename does not self-apply or self-schedule.
PASS.

---

## Risks to the no-self-schedule invariant

**Biggest risk: progress/work-doneToken & long operations.** If a request
(transitive find-refs on the kernel-sized graph, or a workspace-wide rename)
exceeds the inline budget, the reflex is a worker thread + `$/progress`
notifications, which is a self-scheduled wake loop. Mitigation under the model:
keep it ONE inline tick (the seeded closure is already bounded by the reachable
subgraph, `engine.rs:2118-2128`; the digest-skip keeps the rest of the tick near
zero). If a single operation genuinely cannot fit one tick, express it as a
multi-tick `request(id, …, status)` advanced by editor-driven follow-up actions
(e.g. the client's resolve request for a code action), NOT by an engine timer.
Flag any design that adds a server-side timer firing a tick: that is the parked
wake, rejected.

Secondary: a **debounce** on didChange is allowed only as a coalescer of
external actions in the Rust loop (drop intermediate keystrokes, tick on the
latest). It must not be a rule scheduling itself; it never originates a tick
without a pending editor action.

---

## Reachable today vs needs new machinery

**Reachable today (no engine change):**
- publishDiagnostics on save/open — shipping (`lsp.rs:46-66`, `engine.rs:656`).
- find-references / blast-radius via seeded closure — the body-read landed
  (`engine.rs:2084`, `closure_seed_of` at `:217`); pin the symbol literal.
- go-to-def, hover, rename answers — all expressible as reducer rules over
  `ref`/`string`/`def`/`diag` (`engine.rs:48-54`) joined to a `request` rel.
- rename edit application — `refactor::splice_file` + `--move` path exist
  (`refactor.rs:26`); only the LSP `WorkspaceEdit` glue is new.
- byte-span cursor hit-testing — `_where_bytes_file_span_idx` exists
  (`engine.rs:1017`); `located_spans` already projects (`engine.rs:613`).

**Needs new (small, additive) machinery:**
- `doc(path,version,content)` rel + `DocStore` + `change: INCREMENTAL` and the
  engine reading `doc.content` over disk for open buffers (the deferred
  RAM-truth overlay, `lsp.rs:3-5`). — Phase 1.
- `request` / `answer_*` rels + `set_request` / `answer_*` / `clear_request`
  engine seams (`engine.rs`) and the `Message::Request` arm in the loop
  (currently only `handle_shutdown`, `lsp.rs:54`). — Phases 2-4.
- `locate_at(path, byte)` reader (`engine.rs`) for hover/def cursor resolution. —
  Phase 3.
- A `def(name,path,line)` source rule in the served `.dl` program (authorable
  today via `sg`/`ast`; not engine work).

No new evaluation semantics: every reducer is an ordinary rule; the request is an
ordinary fact; the answer is an ordinary derived relation; the tick is the only
dispatch.
