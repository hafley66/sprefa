# LSP architecture fix plan — sprefa v4 — 2026-05-19

Companion to `plans/2026-05-19-v4-worst-audit.md` items 9 and 14, and to `plans/2026-05-19-host-lsp-trait-architecture.md` (orthogonal — that one restructures how host concepts participate; this one fixes the request-path plumbing).

Scope: seven defects across `v4/crates/sprefa-lsp/` and `v4/src/`. Out of scope: editor-extension UX, daemon transport, host-LSP trait registry.

## Layer 0 — fault inventory

| # | Defect | Files | Severity |
|---|--------|-------|----------|
| A | Two doc stores, no transactional close | `crates/sprefa-lsp/src/main.rs:51,367`, `app.rs:504,1452` | wrong state |
| B | Five mutexes, sync-CPU under `std::sync::Mutex` in async | `app.rs:710,769,1443`, `main.rs:50-51,209,254` | latency cliff |
| C | Three byte↔Position implementations | `main.rs:422,449`, `inlay.rs:91`, `cst/lsp/position.rs:9,45` | panic on UTF-8 |
| D | Two `lsp_types` versions, serde round-trip | `Cargo.toml:25`, `main.rs:404` | item drop |
| E | `version: i32` never compared | `app.rs:769`, `main.rs:140,353` | stale diags |
| F | Capability lies (`.`, inlay refresh, completion) | `main.rs:178,170,273`, `crates/sprefa-lsp/src/inlay.rs:1` | misadvertise |
| G | Two diag converters disagreeing on `source` | `main.rs:374`, `cst/lsp/shift.rs:37` | inconsistent UI |

## Layer 1 — ordering (prerequisite DAG)

```
C  byte<->Position collapse  ─┐
                              ├─►  D  lsp_types unify  ─►  E  version gating  ─►  B  spawn_blocking + cancel
G  diag-source unify  ────────┤                                                     │
A  doc-store collapse  ───────┘                                                     ▼
                                                                          F  capability audit
```

1. C first. Every subsequent step calls position math.
2. D before E (version gating wants to live next to publish diags). E touches same call sites.
3. G cheap, parallel, gated on nothing.
4. A before B (one store to wrap in spawn_blocking).
5. B is the big one. Defer until A, C, D, E, G land.
6. F last (set flags to reflect what works).

## Layer 2 — fix C: collapse byte↔Position

```rust
// v4/src/cst/lsp/position.rs (single home)
pub fn position_to_byte(text: &str, pos: lsp_types::Position) -> Option<usize>;
pub fn byte_to_position(text: &str, byte: usize) -> lsp_types::Position;
```

Both total over UTF-8: `position_to_byte` walks `char_indices` and returns boundary at or past requested utf16 column; `byte_to_position` snaps `byte` via `.min(text.len())` and walks `char_indices`. Neither panics, unlike `main.rs:449` which does raw byte indexing.

Migration: delete `main.rs:422-464` and `inlay.rs:91-110`. Replace call sites in main.rs (`235, 257, 282, 327, 342, 343, 377, 378`) with `v4::cst::lsp::position::*` imports. Eight edits.

`position_to_byte` becomes `Option<usize>`; old saturating callers update to `.unwrap_or(text.len())` OR return `None` when out of doc. Prefer the latter — silent saturation was the bug at `main.rs:343`.

Delete dead `inlay.rs` module (`main.rs:39`). `tests/inlay_smoke.rs` reimplements expand pipeline inline; keep.

Test: extend `cst/lsp/position.rs:71-103`. Mid-CRLF; past-end → None; surrogate-pair emoji round-trip; fuzz over random valid UTF-8.

Files: 2 deletions, ~10 import edits, 1 test extension. ~30 LOC net deletion. 1 hour.

## Layer 3 — fix G: collapse diag conversion

Two converters, one target: `cst/lsp/shift.rs`.

```rust
pub enum DiagOrigin {
    HostParse,        // "sprefa.parse"
    HostWalk,         // "sprefa.walk"
    Runtime,          // "sprefa.runtime"
    Dsl(&'static str) // e.g. "sprefa.sql"
}
```

`Diag` carries `origin: DiagOrigin`. `SprfDiag` (wire DTO, `app.rs:131`) gains `origin: String`. `From<&Diag>` maps.

Migration:
1. Add `DiagOrigin` to `cst/diag.rs`.
2. Defaults at constructor sites: `host_parse.rs` → `HostParse`; `walk.rs` → `HostWalk`; runtime → `Runtime`; DSL → `Dsl(self.name())`.
3. `SprfDiag` adds origin field, default `"sprefa"` for back-compat.
4. Delete `to_lsp_diag` in `main.rs:374`. LSP server reads `source` from `SprfDiag.origin` directly.
5. `diag_to_lsp` in `shift.rs` likewise reads `Diag.origin`.

Test: `crates/sprefa-lsp/tests/diag_source.rs` (new). Drive `lsp_open` with parse + walk + DSL diags; assert `source` reflects origin.

Files: `cst/diag.rs`, ~40 constructor sites, `app.rs:131,139`, `main.rs:134,374`, 1 new test. 1.5 days due to constructor churn.

## Layer 4 — fix A: collapse doc stores

Current:
- `Backend.docs: tokio::sync::Mutex<HashMap<Url, DocEntry>>` (`main.rs:51`).
- `SprfState.docs: std::sync::Mutex<HashMap<String, DocState>>` (`app.rs:504`).

Two stores, keyed differently. Race: close that crashes between two leaves stale Backend.docs forever.

Target: Backend.docs removed (text fetched via new RPC `lsp_text` or kept as bounded LRU cache).

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LspTextReq { pub uri: String }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LspTextResp {
    pub text:    Option<String>,
    pub version: Option<i32>,
}

fn lsp_text (LspTextReq) -> LspTextResp => "/lsp/text";
```

Cost: keystroke handlers add one RPC round-trip. In-process this is `Mutex.lock + String.clone`. Daemon path is HTTP per hover/completion — 10s of KB.

Mitigation: keep `Backend.docs` as a TEXT-ONLY LRU cache (32 buffers max), write-through on refresh, RPC fallback on miss. Documented as cache, not authority. Loss of cache entry is safe; loss of RPC entry is what matters.

Test: `crates/sprefa-lsp/tests/state_authority.rs` (new). Open X → SprfState.docs has X. Close X via LSP route → not present. Open X, evict from Backend cache, hover → succeeds via RPC.

Files: `app.rs` (~40 lines new RPC), `main.rs` (~100 lines edits), 1 new test. 4 hours.

## Layer 5 — fix D: tower-lsp / lsp_types unification

`Cargo.lock:2004` has tower-lsp 0.20.0 with lsp-types 0.94.1. Crate effectively unmaintained as of 2026-05-19.

Three paths:

**P1. Switch to `tower-lsp-server`** (community fork): bumped to lsp-types 0.97, drop-in API.

**P2. Switch to `async-lsp`**: different model, larger rewrite of `LanguageServer` impl.

**P3. Pin v4 lsp-types to 0.94**: downgrade `v4/Cargo.toml:42` from 0.97. All DslBodyLsp impls use 0.94. Smaller diff, stuck on 2023 spec.

**Recommendation: P1.** If unmaintained at fix time, fall back to P3.

Migration (P1):
1. `crates/sprefa-lsp/Cargo.toml:15` → `tower-lsp-server = "0.x"`.
2. Update `use tower_lsp::...` → `use tower_lsp_server::...`.
3. Delete `crosswalk` (`main.rs:404`) and `v4_lsp_types` module. `let v4_items: Vec<v4_lsp_types::CompletionItem>` at `main.rs:293` → `Vec<lsp_types::CompletionItem>`.
4. Delete `.filter_map(|it| crosswalk(it))` at `main.rs:312` — becomes `.cloned().collect()`.
5. Bump Cargo.lock.

Editor extension: `v4/editors/vscode/src/extension.ts` is 50 lines of glue using `vscode-languageclient`. No tower-lsp binding. Zero impact expected.

Wire compat check: lsp-types 0.94→0.97 added optional fields. Forward-compatible. Smoke test with extension before merge.

Test: `crates/sprefa-lsp/tests/lsp_types_e2e.rs` (new, ignored by default). Send completion in sql DSL, assert items round-trip non-empty.

Files: `Cargo.toml` (1 line), Cargo.lock, `main.rs` (~30 lines), 1 new e2e test. 2 hours if P1 works first build; 1 day if API drift.

## Layer 6 — fix E: version gating

Current:
- `app.rs:769` `ingest` clobbers regardless of incoming version.
- `main.rs:353` `did_change` pops last `content_changes` and discards earlier (fine for FULL sync but reads tolerant).
- `main.rs:140` publish race after async refresh.

Target: `SprfState.docs` tracks `latest_seen_version` per uri. Drop if `incoming <= latest_seen`. Diagnostics carry version of analysis INPUT.

```rust
pub struct DocState {
    pub text:    String,
    pub version: i32,
    pub latest_seen: i32,
    // ...
}

enum IngestOutcome { Stored, Stale }
```

```rust
fn ingest(&self, uri: String, text: String, version: i32) -> IngestOutcome {
    {
        let docs = self.docs.lock().unwrap();
        if let Some(d) = docs.get(&uri) {
            if version <= d.latest_seen { return IngestOutcome::Stale; }
        }
    }
    let result = compute_doc_state(&text);  // long, no lock held

    let mut docs = self.docs.lock().unwrap();
    let latest_seen = docs.get(&uri).map(|d| d.latest_seen).unwrap_or(-1).max(version);
    if version < latest_seen { return IngestOutcome::Stale; }
    docs.insert(uri, DocState { text, version, latest_seen, ..result });
    IngestOutcome::Stored
}
```

`refresh` in `main.rs:106` reads outcome via RPC; if Stale, don't publish. If Stored, publish with version of analysis input.

Test: `crates/sprefa-lsp/tests/version_gating.rs` (new). Open v=1, v=3, v=2 rapidly. Assert SprfState.docs holds v=3. Assert publish at v=2 suppressed.

Files: `app.rs` (~20 lines), `main.rs` (~10 lines), 1 new test. 3 hours.

## Layer 7 — fix B: spawn_blocking + cancel-on-newer-version

Current: `ingest` runs synchronously inside `async fn lsp_open`/`lsp_change`. Holds `std::sync::Mutex`. 5-50 ms per keystroke blocks tokio worker; queue grows unbounded.

Target: `ingest` in `tokio::task::spawn_blocking`. Per-uri at most one analysis in flight; newer didChange cancels in-flight via `CancellationToken`.

```rust
pub struct SprfState {
    // ...
    in_flight: tokio::sync::Mutex<HashMap<String, InFlight>>,
}

struct InFlight {
    version: i32,
    cancel:  tokio_util::sync::CancellationToken,
    handle:  tokio::task::JoinHandle<()>,
}

async fn ingest_async(self: &Arc<Self>, uri: String, text: String, version: i32) -> IngestOutcome {
    // 1. version gate (fix E)
    // 2. cancel previous in-flight for uri
    // 3. spawn_blocking the compute
    // 4. await; if cancelled return Stale; if Stored insert under version-guard
}
```

```rust
async fn ingest_async(self: &Arc<Self>, uri: String, text: String, version: i32) -> IngestOutcome {
    {
        let docs = self.docs.lock().unwrap();
        if let Some(d) = docs.get(&uri) {
            if version <= d.latest_seen { return IngestOutcome::Stale; }
        }
    }

    let token = CancellationToken::new();
    {
        let mut in_flight = self.in_flight.lock().await;
        if let Some(prev) = in_flight.remove(&uri) {
            prev.cancel.cancel();
        }
    }

    let me = self.clone();
    let uri_c = uri.clone();
    let text_c = text.clone();
    let cancel = token.clone();
    let handle = tokio::task::spawn_blocking(move || {
        if cancel.is_cancelled() { return; }
        let result = me.compute_doc_state_blocking(&uri_c, &text_c, version);
        if cancel.is_cancelled() { return; }
        let mut docs = me.docs.lock().unwrap();
        let latest = docs.get(&uri_c).map(|d| d.latest_seen).unwrap_or(-1);
        if version <= latest { return; }
        docs.insert(uri_c.clone(), result);
    });

    {
        let mut in_flight = self.in_flight.lock().await;
        in_flight.insert(uri.clone(), InFlight { version, cancel: token, handle });
    }

    IngestOutcome::Stored
}
```

### Publish path inversion

Today: server calls `refresh`, then `get_diags`, then publishes. With async analysis, `get_diags` returns previous version's results.

Fix: publish from analyzer via outbound channel.

```rust
pub struct PublishEvent {
    pub uri:     String,
    pub version: i32,
    pub diags:   Vec<SprfDiag>,
}

pub trait SprfClient {
    fn subscribe_publish(&self) -> tokio::sync::mpsc::Receiver<PublishEvent>;
}
```

In-process: real mpsc. HTTP daemon: defer (not keystroke-critical).

### Lock ordering (post-refactor)

- `SprfState.docs`: take last, hold briefly.
- `SprfState.in_flight`: take before spawn. Never held across await.
- `Backend.sprf`: held only for swap on `set_workspace_root`. Read path clones Arc.

Document at top of `app.rs`.

### Back-pressure

`spawn_blocking` uses tokio's 512-thread default pool. At 5 ms typing, 200 ms compile, steady-state concurrent compiles ~40. Cancellation halves that. No back-pressure needed if we cancel-on-newer-version. If profile shows pool exhaustion, add `Semaphore::new(num_cpus)`.

### Quantified latency

Before: tokio worker blocked 5-50 ms per keystroke. At 2 workers + >2 docs being edited, publish_diagnostics for unrelated URIs queues. Observable: 50-200 ms publish latency under load.

After:
- Protocol parse: ~0.1 ms
- Spawn blocking: ~0.05 ms
- Compute: 5-50 ms (unchanged; CPU-bound)
- mpsc publish + serialize: ~0.5 ms
- Client publish_diagnostics: ~0.5 ms

Typing-feel decoupled from compile time. Worst-case publish under 200 keystrokes/sec falls from ~10x compile (queue depth) to ~1x compile (latest only).

Test: `crates/sprefa-lsp/tests/concurrency.rs` (new). Submit v=1..50 in 1ms intervals; assert exactly one publish lands with version ≤50, never publish v=1. Two URIs concurrently, slow A; B's publish completes within 2x compile. Close after submit; in_flight cleared, no post-close publish.

Files: `app.rs` (~150 new lines), `main.rs` (~80 lines subscribe loop), 1 new test, 1 new dep `tokio-util` for `CancellationToken`. 2 days.

## Layer 8 — fix F: capability audit

**F1: `.` trigger.** `main.rs:178` declares `.` with no handler. Remove from list until implemented. One-line.

**F2: inlay_hint_provider with no refresh.** Send `workspace/inlayHint/refresh` from publish path consumer in `main.rs`. Advertise `InlayHintWorkspaceClientCapabilities.refresh_support`. Check client caps in `initialize`. One line after publish_diagnostics.

**F3: completion routing.** Re-read: `main.rs:302` already routes non-sql through `dsl_lookup::provider_for(&op_name)`. Audit comment ("only sql; others empty") partially wrong. Actual issue: `dsl_lookup::provider_for` returns `None` for unregistered ops. Document asymmetry; SQL has facts dependency that warrants host RPC. Out of scope: extending `DslBodyLsp::completions` with optional context. Defer to broader `HostLspDef` plan.

**F4: dead inlay.rs.** Covered in Layer 2 — delete.

Test: `crates/sprefa-lsp/tests/capabilities.rs` (new). Drive `initialize`, read `InitializeResult.capabilities`, assert each advertised has a handler returning non-default on hand-crafted buffer.

Files: `main.rs:170,178`, 1 new test. 1 hour F1; half day F2.

## Layer 9 — test strategy

| Fix | Test file | Asserts |
|-----|-----------|---------|
| C | `cst/lsp/position.rs` (extend) | UTF-8 boundary, surrogate pair, past-end |
| G | `tests/diag_source.rs` | source reflects origin |
| A | `tests/state_authority.rs` | single store, close removes, cache fallback |
| D | `tests/lsp_types_e2e.rs` | no items dropped through completion |
| E | `tests/version_gating.rs` | newest wins, older suppressed |
| B | `tests/concurrency.rs` | cancel collapse, cross-URI fairness |
| F | `tests/capabilities.rs` | each advertised flag honored |

Integration harness: `crates/sprefa-lsp/tests/common/mod.rs`:

```rust
pub struct TestBackend {
    pub backend: Arc<Backend>,
    pub publish: tokio::sync::mpsc::Receiver<(Url, Vec<Diagnostic>, Option<i32>)>,
}
pub fn make_test_backend() -> TestBackend { ... }
```

Mock `Client` captures `publish_diagnostics`. tower-lsp `LspService::new` returns `Service<Request, Response = Option<Response>>` — tests call `service.call(req).await` directly without stdio loop.

CI: `cargo test -p sprefa-lsp` already runs under `cargo test --workspace`. Verify.

## Layer 10 — risk to editor extension

`v4/editors/vscode/src/extension.ts` does NOT depend on tower-lsp internals, lsp_types versions, or doc-store layout.

DOES depend on:
- Wire-format `textDocument/publishDiagnostics` stability. After E + B, `version` reliable; client may use for staleness.
- Wire-format `textDocument/inlayHint` stability. After F2, server sends refresh; newer `vscode-languageclient` accepts.
- `completionItem` shape. After D, items don't pass through `crosswalk`; shape may differ slightly.

Risk: low. PR description checklist:

- [ ] Open `.sprf` file, see syntax tokens
- [ ] Type fast, diagnostics update without lag
- [ ] Hover a host token, see hover text
- [ ] Trigger completion in `sql` body, see items
- [ ] Trigger completion in `re` body, see items
- [ ] Invalid line shows diag with source `"sprefa.parse"` or similar

## Layer 11 — files touched summary

| File | Lines (est) |
|------|---------------------|
| `crates/sprefa-lsp/src/main.rs` | ~200 |
| `crates/sprefa-lsp/src/inlay.rs` | DELETE (110) |
| `crates/sprefa-lsp/src/dsl_lookup.rs` | ~10 |
| `crates/sprefa-lsp/Cargo.toml` | ~3 |
| `v4/src/app.rs` | ~250 |
| `v4/src/cst/lsp/position.rs` | ~30 |
| `v4/src/cst/lsp/shift.rs` | ~10 |
| `v4/src/cst/diag.rs` | ~30 |
| Diag constructors across `v4/src/**` | ~40 sites, 1 token each |
| `crates/sprefa-lsp/tests/*.rs` | ~600 new lines across 6 files |
| `v4/Cargo.toml` (if P3 fallback) | ~1 |
| `editors/vscode/*` | NONE expected |

Total: ~700 LOC net change, ~600 lines new tests. One file deleted. One dep swap. 5-7 working days for one engineer.

## Layer 12 — deliberately deferred

- `HostLspDef` / `HostLspNode` redesign (separate plan).
- Daemon-mode publish via SSE.
- Incremental textDocumentSync (today FULL only).
- ast DSL completions (`dsl_lookup.rs:21` TODO).
- Cross-doc workspace symbol search.
