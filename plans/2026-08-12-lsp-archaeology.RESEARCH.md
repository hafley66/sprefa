# RESEARCH: recover every LSP/editor incarnation, price a standalone editor area

Base `0447d771`. Docs only. Two deliverables: this file and its unga twin
(`...visual.human.unga.md`). ZERO implementation code.

Prior recon that this doc builds on, not re-derives:
`plans/2026-08-12-v6-native-lsp.PLAN.md` (250 lines) and its unga twin
established the v6 `serve_tsv2` merge point, the `vscode-jsonrpc` over
`vscode-languageserver` verdict for the TS runtime, PR #202 hover being dead on
arrival, and the net-cost-of-dropping-v5 (one feature: diagnostics). This doc
extends that with the full version-to-version archaeology (v3 against v4
against v5), the thin-client feature table, a standing-law build-vs-buy candidate
set, and the shape forks for the standalone editor area.

## TOC

1. Scope and readings
2. Deliverable 1: archaeology, version by version
3. Comparison table, v3 against v4 against v5 against v6
4. Deliverable 2: the thin-client line
5. Deliverable 3: build-vs-buy, candidate by candidate
6. Deliverable 4: the shape recommendation (forks, priced)
7. Verification and receipts
8. Findings that contradict upstream docs

---

## 1. Scope and readings

Read (not catalogued, actually opened):

- `v3/crates/server/src/transport_lsp.rs` (69 lines), `v3/crates/server/src/bin/sprefa-server.rs` (248),
  `v3/crates/sprefa/src/server/transport_lsp.rs` (357), `v3/crates/sprefa/src/server/lsp_layer.rs` (173),
  `v3/crates/pipeline/src/ops/lsp.rs` (378), v3 smoke `_f_lsp_hover_ast.sh` / `_i_lsp_invalidate_kernel.sh` /
  `_k_lsp_diag.sh`.
- `v4/crates/sprefa-lsp/src/{main.rs,inlay.rs,semantic.rs,dsl_lookup.rs}` (1077 total),
  `v4/editors/vscode/src/extension.ts`, `v4/editors/vscode/package.json`,
  `v4/tests/{lsp_hover_smoke.rs,lsp_locate_dsl_smoke.rs}`, `v4/crates/sprefa-lsp/tests/inlay_smoke.rs`,
  the three v4 lint plans.
- `src/lsp.rs` (2000), `editors/vscode/src/extension.ts`, `editors/vscode/package.json`,
  `editors/vscode-dl/` (inventory only).
- `v6/tsv2/serve/main.ts`, `v6/tsv2/package.json`, `v6/tools/one-subscribe.sh`, `v6/sprefa-engine-rs/`.
- Workspace skills `sprf-lsp-server-libs`, `sprf-lsp-multi-dsl-patterns`.

Archives: `~/projects/sprefa-archive-20260701/{v3,v4,v5cozokuzu}`, `~/projects/sprefa-archive-20260428`.

LSP specification checked: **3.18 (current)**. Confirmed at
microsoft.github.io/language-server-protocol (3.18 marked "Current", 3.17 "Previous").
Feature capsules in section 4 are 3.17/3.18 where noted.

---

## 2. Deliverable 1: archaeology, version by version

### 2.1 v3

Files measured and read.

| element | where | line |
|---|---|---|
| methods implemented | `initialize, initialized, shutdown, did_open, did_change, did_close, did_save, hover, completion` | `crates/server/src/backend.rs:259,281,282,284,293,314,324,342,462` (LanguageServer impl) |
| advertised capabilities | text sync FULL, hover, completion | `crates/sprefa/src/server/transport_lsp.rs:193-197` |
| transport | stdio JSON-RPC via `tower_lsp::Server` on stdin/stdout; second transport is LSP-over-WebSocket bridged through a `tokio::io::duplex` pipe into the same `tower_lsp::Server` | stdio `crates/server/src/bin/sprefa-server.rs:233-238`; WS `crates/sprefa/src/server/transport_lsp.rs:43-89` |
| library | `tower-lsp = 0.20` (+ `axum` for WS) | `crates/sprefa/Cargo.toml:12,41`, `crates/server/Cargo.toml:20,33` |
| what lived editor-side that could be LSP | there is no in-tree editor client in v3. `crates/server/README.md:26-34` directed a generic LSP client (helix / generic-lsp-client / vscode-languageclient) at the `sprefa-lsp` binary. The custom `sprefa-lsp` proxy binary was merged into `--lsp-stdio` (`sprefa-server.rs:9-13`) |
| what could NOT be LSP and why | the WS transport itself: LSP-over-WebSocket is not a client interop standard; it existed to let `sprefa-run`/the daemon reach LSP without an editor spawn. The daemon-side FS watcher (`spawn_fs_watcher`, `crates/server/src/state.rs:28`) is not an LSP feature (LSP has no server-side file-watch push; the client sends `didChangeWatchedFiles`) |
| how server learned of changes | LSP notifications (did_open/did_change/did_save, full text sync) AND a pipeline FS watcher feeding the shared state | notifications `transport_lsp.rs:209-229`; watcher `crates/server/src/state.rs:28` |
| testing | `_f_lsp_hover_ast.sh` and `_i_lsp_invalidate_kernel.sh` drive the real wire: `Content-Length` framed JSON-RPC over stdio against `sprefa-server --lsp-stdio`, greping the response (hover: `_f:75,85-136`). Wire exercised, not just handlers. `_k_lsp_diag.sh` tests the `lsp[severity]` op surfacing as SSE Diag frames through `sprefa-run` (`_k:2-8`), not the LSP wire |
| why it did not survive | v3 itself was archived; the LSP was re-written for v4 as a distinct `sprefa-lsp` crate with an in-tree editor client. Both v3 and v4 were superseded by the v5 single-crate `src/lsp.rs` |

### 2.2 v4

Files measured and read. crate total 1077.

| element | where | line |
|---|---|---|
| methods implemented | `initialize, initialized, shutdown, semantic_tokens_full, inlay_hint, hover, completion, goto_definition, did_open, did_change, did_close`. Did_save absent (text sync fully pullable on change) | `main.rs:192-418` |
| advertised capabilities | text sync FULL, semantic tokens, inlay hints, hover, definition, completion (with trigger chars `$`/`:`/`.`) | `main.rs:200-234` |
| transport | stdio JSON-RPC via `tower_lsp::Server` | `main.rs:564-578` |
| library | `tower-lsp 0.20`, hand-rolled UTF-16/byte bridge (`crosswalk`) between two lsp-types versions | `main.rs:456-458` |
| what lived editor-side that could be LSP | extension.ts is 48 lines: `vscode-languageclient`, spawn `sprefa-lsp` stdio, `documentSelector` `language: sprf`, and one `FileSystemWatcher('**/*.sprf')` in `synchronize.fileEvents` (`extension.ts:32-35`). The client-side FS watcher is generic-client behavior, not an LSP gap |
| what could NOT be LSP | in v4 the semantic tokens legend types `regexp/macro/enumMember` are declared in `package.json:53-72` (editor-authored) but emitted through `textDocument/semanticTokens`, so they are LSP after all. The tmLanguage grammar + language-configuration are declarative editor contributions, unavoidably client-side |
| how server learned of changes | LSP did_open/did_change (debounced 80ms, version-coalesced `main.rs:123-189`)/did_close only. No server-side FS watcher for LSP in the crate. The daemon had a separate ghcache watcher, unrelated to LSP |
| testing | `lsp_hover_smoke.rs` (295) and `lsp_locate_dsl_smoke.rs` (97) drive `SprfClient` RPC handlers in-process (`build_in_process`), NOT the JSON-RPC LSP wire. `inlay_smoke.rs` is `#[ignore]`d and states `src/inlay.rs` is dead code post-fuser. So v4 had no wire-level test |
| why it did not survive | v4 was an extracted LSP crate + editor that the v5 rewrite folded back into the single compiler crate. The inlay feature died on the fuser (see 2.4) |

### 2.3 v5 (current repo root)

| element | where | line |
|---|---|---|
| methods implemented | didOpen/didSave notifications; requests: definition, references, dl/refs, dl/locate, hover, documentHighlight, documentSymbol, workspace/symbol, prepareCallHierarchy + incoming/outgoing, prepareTypeHierarchy + super/sub, dl/query, dl/hookEvent, workspace/executeCommand (dl.toggleDiagCode, dl.listDiagCodes), publishDiagnostics | dispatch `src/lsp.rs:248-331`; capabilities `src/lsp.rs:57-92` |
| advertised capabilities | text sync Options(open_close, change NONE, save supported), definition, references, documentHighlight, workspaceSymbol, documentSymbol, hover, callHierarchy, executeCommand; typeHierarchy spliced into raw JSON (crate gap, `src/lsp.rs:78-92`). No completion, no formatting | `src/lsp.rs:58-92` |
| transport | stdio JSON-RPC `lsp_server::Connection::stdio()` | `src/lsp.rs:56` |
| library | `lsp-server = 0.7.9` + `lsp-types = 0.97.0` | `Cargo.toml:122-123` |
| what lived editor-side that could be LSP | extension.ts is 56 lines: `vscode-languageclient`, spawn `sprefa-server --lsp-stdio`, wide `documentSelector [{scheme:file}]` filtered by `.sprf` middleware, one `FileSystemWatcher('**/*.sprf')` (`extension.ts:28-38`). TmLanguage + language-config declarative |
| what could NOT be LSP | the `.dl`-program editing experience (squiggles on the program file, `publish_dl_parse_errors`) is still LSP `publishDiagnostics`. Nothing in the editor needed to be client-native beyond the grammar and the watcher |
| how server learned of changes | LSP didOpen/didSave tick paths (`src/lsp.rs:330-353`; `didChange` deliberately ignored, sync NONE, disk-truth `src/lsp.rs:58-64`) AND a daemon watcher subscription: the LSP attaches to the shared-db daemon and receives `dl/diagChanged` pushes (`src/lsp.rs:196-243`). Also a `--diag-db` poll mode (`src/lsp.rs:495-633`) |
| testing | `docs/lsp.md:180-205` and `v6/tsv2/scripts/lsp_diag_driver.py` drive a real Content-Length stdio client against the real binary (the brief's cited v5 `publishDiagnostics` measure). Rust unit tests cover handler internals; the wire is exercised by the driver |
| why it did not survive | user constraint: no design may end in keeping the v5 binary running. The net cost of dropping v5 is one feature, diagnostics (v6-native-lsp plan section 2) |

### 2.4 v4 inlay and the three v4 lint plans (constraint 2, the interesting rows)

`v4/crates/sprefa-lsp/src/inlay.rs` (111) implemented `textDocument/inlayHint`:
one-shot `host_parse -> walk -> expand` with a probe sink, one `→ N cursors` hint
per op span (`inlay.rs:35-87`). It never shipped working: `inlay_smoke.rs` is
`#[ignore]`d and its header says the fuser changed pipe shape so the probe path
is no longer the inlay backbone and `inlay.rs` is dead code (`inlay_smoke.rs:1-39`).

The three v4 lint plans are exactly "generic enhancement through LSP":

| plan | intent | status |
|---|---|---|
| `lsp-fs-watcher-reactive-wake.md` | OS watcher + LSP did_change funnel into one daemon wake ingress, VFS overlay for unsaved bytes, <=100ms diagnostics; publish_diagnostics to IDE | plan only. The reactive daemon wake was re-architected in v6, never shipped as this v4 plan |
| `lsp-loop-justification-lint.md` | `loop-justify:` comment schema linted by ast_yaml(:rs) rules, antijoin `all_loops \ justified_loops`, surfaced as diagnostics | plan only. Rust-source lint, not .sprf; never shipped as an LSP diagnostic source |
| `lsp-sprf-component-n-plus-1-lint.md` | static lint over `impl Component` bodies flagging fork/exec, sync IO, per-row insert, git rev-parse | plan only. Motivated by two real N+1 bugs, never shipped |

None of the three shipped. They document the intent to push author-time
diagnostics-as-lint through LSP, which is the model LSP already serves and the
"place for generic enhancement" should adopt rather than re-invent.

---

## 3. Comparison table, v3 against v4 against v5 against v6

| | v3 | v4 | v5 | v6 |
|---|---|---|---|---|
| server language | Rust (tower-lsp 0.20) | Rust (tower-lsp 0.20) | Rust (lsp-server 0.7.9) | none native yet (reaches editor only via v5 today) |
| transport | stdio + LSP-over-WebSocket (axum) | stdio | stdio | planned: consumer of tsv2 `serve_tsv2` or separate |
| methods | init, hover, completion, didOpen/Change/Save/Close, diag publish | + semanticTokens, inlayHint, definition; no didSave | + definition, references, highlight, docSymbol, wsSymbol, call+type hierarchy, executeCommand, query, refs, locate; no completion, no formatting | not built |
| completion | yes | yes (DSL providers + sql) | absent | not built |
| formatting | absent | absent | absent | not built |
| inlay hints | absent | attempted, dead on fuser | absent | not built |
| semantic tokens | absent | yes | absent | not built |
| editor client | none in-tree (generic LSP client) | 48-line TS, vscode-languageclient | 56-line TS, vscode-languageclient | to be decided (thin-client line) |
| client-side FS watcher | absent (daemon watcher) | yes `**/*.sprf` | yes `**/*.sprf` | n/a |
| server-side change source | LSP notifications + FS watcher | LSP notifications only | LSP save/open + daemon watcher + diag-db poll | rewrite tick engine is render-push |
| test exercises wire? | yes (stdio Content-Length smoke) | no (RPC handlers) | yes (lsp_diag_driver.py) | driver reused as v6 receipt |
| graph viewer | none | none | `editors/vscode-dl` ~6103 lines, OUT OF SCOPE | out of scope (brief, four times) |
| fate | archived (`sprefa-archive-20260701/v3`) | archived (`.../v4`) | shipping | current planning phase |

---

## 4. Deliverable 2: the thin-client line

LSP 3.18 feature-to-editor map. Everything LSP covers must live on the server.
One row per feature the project has had or wanted.

| feature | LSP capability that covers it | or: why it cannot be LSP | verdict |
|---|---|---|---|
| diagnostics (parse, lint, type) | `textDocument/publishDiagnostics` (+ pull `diagnostic` 3.17) | | server |
| diagnostics-as-lint (the v4 lint plans) | publishDiagnostics with `relatedInformation`, `tags`, `code` | | server (this is where the "generic enhancement" slot lives) |
| inlay hints | `textDocument/inlayHint` (+resolve, +refresh) | | server |
| semantic tokens | `textDocument/semanticTokens` (full/range) | | server (v4 already did this; legend client) |
| hover | `textDocument/hover` | | server |
| definition / references / highlight | `textDocument/definition` / `references` / `documentHighlight` | | server (v3/4/5 all did these) |
| document symbols / workspace symbols | `textDocument/documentSymbol` / `workspace/symbol` | | server |
| call + type hierarchy | `textDocument/prepareCallHierarchy`/`callHierarchy/*`, `prepareTypeHierarchy`/`typeHierarchy/*` (3.18 type hierarchy) | | server |
| completion / snippet | `textDocument/completion` (+resolve, +insertReplace) | | server |
| rename / prepare rename | `textDocument/rename` / `prepareRename` | | server (net-new; v5 had none) |
| code actions | `textDocument/codeAction` (+ resolve) | | server (the hook for generated fixes) |
| formatting | `textDocument/formatting` / `rangeFormatting` | smol/rustfmt-style is a library call; keep on server | server |
| folding ranges | `textDocument/foldingRange` | | server |
| selection ranges | `textDocument/selectionRange` | | server |
| document links | `textDocument/documentLink` | | server |
| code lens | `textDocument/codeLens` | | server |
| execute-command / editor commands | `workspace/executeCommand` | | server (v5 already: dl.toggleDiagCode) |
| diagnostic muting (v5 `dl.toggleDiagCode`) | `workspace/executeCommand` (already LSP) | | server (keep, do not move to client) |
| workspace edits / apply edits | `workspace/applyEdit`, `textDocument/workspaceEdit` | | server |
| file-operation notifications | `workspace/willCreateFiles`, `did*/changedWatchedFiles` | | server (the VFS overlay wake the v4 plan wanted) |
| progress reporting | `window/workDoneProgress` / `window/createWorkDoneProgress` | | server |
| show document | `window/showDocument` (3.16) | | server |
| open file hyperlinks in hover | `textDocument/hover` returning Markdown links (v3 `rewrite_hover_paths`) | | server |
| syntax highlighting grammar | | tmLanguage/grammars are a declarative editor contribution by design; LSP has no grammar mechanism | client (declarative only, not logic) |
| language-config (brackets, comments) | | editor declarative contribution; LSP has no equivalent | client (declarative only) |
| **graph viewer (`editors/vscode-dl`)** | | OUT OF SCOPE (brief). Decomposed separately | out of scope |
| webview / custom panel UI | | LSP has no UI mechanism; any bespoke panel is genuinely outside LSP | client, but ONLY via the defined generic-enhancement slot (below) |

The generic-enhancement slot (the "place the user asked for"):

- Rule A: a feature ships in the editor client only if it has no LSP method
  (LSP 3.18) and is not declarative (grammar/config). Everything else is a
  server feature.
- Rule B: client logic must be a registry of declarative menu/command/grammar
  entries plus a single activation file that spawns the server. No feature
  state lives in the client; anything stateful goes to the server behind
  `workspace/executeCommand` or a custom `custom/` request.
- Rule C: a feature is rejected from the client until it names the LSP method
  it studied; the table above is the gate.

---

## 5. Deliverable 3: build-vs-buy (standing law)

Former plan `2026-08-12-v6-native-lsp.PLAN.md:79-120` priced the TS-side
candidates for the tsv2 runtime; this extends to the full wire + maintenance
status verified against registries.

### 5.1 Speaking LSP from TypeScript

| candidate | version (checked 2026-08-12) | what it gives | what it costs | maintenance / API shape |
|---|---|---|---|---|
| `vscode-languageserver` | 10.1.0 (npm) | full server: Connection, TextDocuments, feature registration, sendDiagnostics | owns connection + async handler model + document lifecycle | npm publish 10.1.0; from MS monorepo; promise/thenable handlers |
| `vscode-jsonrpc` | 9.0.1 (npm) | bare wire: MessageReader/Writer, Disposable, cancellation, correlation | you assemble features + framing | identical monorepo; reader is an event source that composes into the rxjs spine |
| `vscode-languageserver-protocol` | 3.18.2 | 3.18 request/response type surface | needs a transport | MS; the type layer both of the above use |
| `vscode-languageserver-types` | 3.18.0 | pure data types, zero deps | no transport | MS; single package, no transitive meaning |
| roll your own JSON-RPC + hand-typed LSP | 0 | nothing bought | ~150-250 LOC framing + error codes + cancellation; re-implements types 3.18 for free | fails infra-is-bought; the buy is the types, cheap |

Reading that respects the four constraints and the earlier verdict: buy the wire
and the types (`vscode-jsonrpc` + `vscode-languageserver-types`, or the
`vscode-languageserver-protocol` bundle); do not buy `vscode-languageserver`'s
connection because the user runs `serve_tsv2` and wants one process with one
merge point. This is the earlier plan's vote, kept and extended below.

### 5.2 Speaking LSP from Rust

| candidate | version (checked) | what it gives | maintenance | API shape |
|---|---|---|---|---|
| `lsp-server` | 0.10.0 live (0.7.9 in v5) | sync stdio JSON-RPC, ~808 LoC, you own the loop | rust-analyzer mainline, active (0.10.0 2026-07-16) | minimal; v5 already runs it |
| `async-lsp` | 0.2.4 (2026-04-24) | async, `&mut self` handlers, tower middleware (timing, catch-unwind, concurrency) | oxalica, active | the only one with real middleware |
| `tower-lsp` | 0.20.0 (2023-08-11) | async `&self`, tower-based | original repo effectively dormant (3 years) | `Arc<Mutex<..>>` everywhere; take over via `tower-lsp-server` fork if used |
| `tower-lsp-server` | community fork | same as tower-lsp | fork | only if staying on tower-lsp API |
| `lsp-types` | 0.97.0 (v5, also latest) | wire types all servers use | rust-analyzer mainline | shared |

Skill `sprf-lsp-server-libs` agrees: lsp-server sync/ra-style, async-lsp if you
want middleware, tower-lsp unmaintained. v5 already ships lsp-server in
production, so it is the proven Rust choice; async-lsp is the live alternative.

### 5.3 The client side

| candidate | version | what it gives | fit |
|---|---|---|---|
| `vscode-languageclient` | 10.1.0 (engines vscode ^1.91.0) | whole client: spawn, sync, middleware, all feature plumbing | conventional. v4/v5 both use ^9 |
| what a minimal client actually needs | | v4/v5 prove ~50-60 lines TS: spawn server stdio + documentSelector + FileSystemWatcher + middleware filter. The rest (hover/complete/inlay) needs no client code | the target is this subset; the extension stays thin by construction |

### 5.4 Position and offset math

| candidate | version | why | Rust fit |
|---|---|---|---|
| `line-index` | 0.1.2 | line/UTF-16/bijective offset map, O(log n); extracted from rust-analyzer | yes, buy (skill: never hand-roll; the emoji smell) |
| `lsp-positions` | 0.3.4 | keeps utf8/utf16/grapheme columns together; from stack-graphs | alternative, heavier |
| `ropey` | 1.6.1 | rope text buffer, not just position math | only if the server edits text (rename/format); not for a read-mostly server |

### 5.5 Testing an LSP over the wire

| candidate | what it is | fit |
|---|---|---|
| `@vscode/test-electron` | boots real VS Code for e2e | heavy; overkill for wire truth |
| `vscode-languageserver` test harness/`installServerIntoExtension` | harness for the server | couples to vscode-languageserver connection |
| plain stdio golden tests | `Content-Length` frames in, responses out | already the proven path: v3 smokes and `v6/tsv2/scripts/lsp_diag_driver.py` |

The receipt to reuse: the Content-Length stdio driver already in-tree.

### 5.6 Packaging a thin extension

| candidate | version | fit |
|---|---|---|
| `@vscode/vsce` | 3.9.2 | standard packager; v4/v5 package workflows already use the vsce command |
| generated vs hand-written extension | | a hand-written thin client beats a generator: the generated surface is bigger than the ~50 LOC the functionality needs, and the client is the anti-sprawl constraint. Generator adds a build step for no win here |

### 5.7 The language question, resolved as a fork

The tension: the earlier plan chose `vscode-jsonrpc` (TypeScript) because v6's
`serve_tsv2` is TypeScript and owns the merge point. But v6 is building the
rust engine `sprefa-engine-rs` ("the Rust runtime the emit_rust emitter lowers
against", `v6/sprefa-engine-rs/Cargo.toml:7`) in parallel. So the server may be:

- TS inside `tsv2/serve` (one process, one merge point, the earlier plan's shape), or
- Rust on `sprefa-engine-rs`/`lsp-server` (the language v4/v5 both shipped, the
  language of the emerging emit-rust runtime).

Both satisfy the standing law (neither keeps the v5 binary). Present as forks,
not a pick; the user rules. Cost delta between them is the price of composing
the wire into the TS rxjs spine versus adding `lsp-server` to the Rust engine.

---

## 6. Deliverable 4: the shape recommendation (forks, priced)

The user wants "its own lib/app area", not welded into the compiler tree.

### 6.1 Where it lives

| option | price | dependency direction |
|---|---|---|
| sibling package under `v6/` (e.g. `v6/lsp/` + `v6/editors/vscode-sprf/`) | lowest friction: reuses `pnpm-workspace`, `tsgo`, the one-subscribe harness; no new repo, no new CI | server depends on the store seam and on compiler-produced SQLite tables; compiler must not depend on the editor area |
| separate repo | cleanest boundary, but loses the shared rxjs/store harness and gains repo+CI+publish plumbing | same direction, enforced by the repo boundary |
| workspace member only | a middle case: a `v6`-side member with its own `package.json` | same |

Price note: the compiler's output is SQLite tables + the `serve_tsv2`/engine
wire, so the editor area only ever depends on produced artifacts, not compiler
crates. That is the dependency-direction origin: editor area is a consumer.

### 6.2 Dependency direction

Editor area depends on compiler output (served tables via the store seam, or
the executable). Compiler never imports the editor area. Enforce by keeping the
editor area out of the compiler's `Cargo.toml`/`package.json` dependents and by
only ever reading emitted tables/receipts.

### 6.3 The wire between extension and server

Stdio JSON-RPC (the `vscode-jsonrpc`/`lsp-server` buy), the only transport a
stock client spawns and speaks. SSE-over-HTTP fits `serve_tsv2` but no stock
editor speaks it as LSP, which forces a custom client, which is the sprawl the
brief bans.

### 6.4 Build, publish, and how thin

- Build: `tsgo` typecheck + `@vscode/vsce package`, the v4/v5 workflow.
- Publish: vsce / manual `.vsix`, as today.
- Thin target: extension holds grammar + language-config (declarative), one
  `extension.ts` that spawns the server, and a documentSelector. v4 managed on
  48 lines, v5 on 56; target <=100 lines with the depend-only-on-LSP rule and
  Rules A-C holding the rest server-side.

### 6.5 Disposition of the two existing trees

| tree | status |
|---|---|
| `editors/vscode` (the `.vsix`, spawns `sprefa-server --lsp-stdio`) | superseded: its README even still says it talks to `v3/crates/server` (`editors/vscode/README.md:5`), stale. Replaced by the standalone editor area. Its wiring (spawn stdio, watcher, `.sprf` middleware) is the template, not the artifact |
| `editors/vscode-dl` (graph viewer, ~6103 lines) | OUT OF SCOPE, keeps working independently or parked untouched; decomposed separately per the brief |

---

## 7. Verification and receipts

| probe | what it proves | source |
|---|---|---|
| base `0447d771` | lane base | `git log --oneline -1` |
| v3 smoke `_f`/`_i` (Content-Length stdio) | v3 exercised the wire | `_f_lsp_hover_ast.sh:72-151` |
| v4 tests drive `SprfClient` in-process | v4 did not exercise the wire | `tests/lsp_hover_smoke.rs:6-20` |
| `inlay_smoke.rs` `#[ignore]` + dead-code note | v4 inlay never shipped | `tests/inlay_smoke.rs:1-39` |
| v5 `publishDiagnostics` measure + driver | v5 exercised the wire | plan §2.1; `v6/tsv2/scripts/lsp_diag_driver.py` |
| LSP 3.18 confirmed | spec version for section 4 | microsoft.github.io/language-server-protocol |
| candidate versions | build-vs-buy prices | npm registry / crates.io, checked 2026-08-12 |

---

## 8. Findings that contradict upstream docs

- `editors/vscode/README.md:5` claims the extension talks to the `sprefa-lsp`
  binary from `v3/crates/server`; the live `extension.ts:16` spawns
  `sprefa-server --lsp-stdio`. The README is stale. (Confirms the anti-cheat:
  READMEs go stale; go by code.)
- The brief's pre-merit line "v3 had no editor client" holds by code: no
  TypeScript/JSON editor lives under `v3/`; the generic-client instructions are
  only a README. (Spelled out rather than assumed.)
- Nothing contradicts the v6-native-lsp plan's core findings (merge point, dead
  hover, net-two-features inventory). It is built on, not corrected.
