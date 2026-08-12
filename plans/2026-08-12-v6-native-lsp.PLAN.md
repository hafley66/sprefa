# PLAN: v6 stops borrowing v5's LSP. Recon and price.

Base `154ae23c`. Constraint, user 2026-08-12 verbatim: "I DO NOT WANT TO RUN V5
ANYTHING ANYMORE". Every price holds under it. No option whose answer is "keep
the v5 binary running".

## TOC

1. Context: what the one sentence means in code
2. Part 1, dependency inventory: every place v6 needs a running v5
3. Part 2, build-vs-buy: LSP server candidates, priced for the tsv2 runtime
4. Part 3, the forks: separate process vs consumer of the existing serve
5. Part 4, disposition of PR #202
6. Verification
7. Staffing

---

## 1. Context: what the one sentence means in code

v6's editor face today is a set of `.dl6` rels whose tables the v5 Rust binary's
LSP polls. The naming trick that makes this work is `table_name(Name/_Arity,
Name)` (`v6/prolog/lower.pl:176`): a rel compiles to a bare-named SQLite table,
so a `.dl6` rel literally named `diag_v5` IS the table v5's `--diag-db` reader
selects. `src/` at the repo root is v5; everything under it is on the wrong side
of the user's line.

This brief recon lines up all the v6-written tables, cross-references them
against the ONLY foreign table v5's LSP ever selects, prices a v6-owned LSP
front, and states what to do with the PR #202 hover sink.

## 2. Part 1, dependency inventory

Method: enumerated every table a `.dl6` program in `v6/dl/fixtures/` can head,
cross-referenced against every `SELECT`/`txt_tbl` read in `src/lsp.rs` and the
engine lenses it populates. The complete v5 LSP method dispatch is
`src/lsp.rs:248-331`; the complete foreign-db read is `src/lsp.rs:633` (the one
`SELECT ... FROM <literal>` in the LSP).

### 2.1 The editor dependencies (v6 writes -> v5 reads)

| # | feature | how v6 reaches it today | v5 code that serves it | works E2E today |
|---|---|---|---|---|
| D1 | diagnostics | `.dl6` rel named `diag_v5`, 9 cols, compiles to bare table `diag_v5` (`lower.pl:176`); served by tsv2; v5 `dl --lsp --diag-db <file>` polls it every 500ms | `run_diag_db_mode` `src/lsp.rs:495`; `diag_db_poll_loop` `src/lsp.rs:515`; `SELECT path,line,col,end_line,end_col,severity,code,msg,hint FROM diag_v5` `src/lsp.rs:633`; `publishDiagnostics` `src/lsp.rs:738`; relative-path resolve `publish_diag_v5_path` | YES, measured. `lsp-diags.sh` phase B drove the real v5 binary over real Content-Length JSON-RPC stdio; `publishDiagnostics` appeared (`b.ts` no-eval + unused-def) and retracted on the same session. `docs/lsp.md:180-205` shows first-poll publish before `initialized`/`didOpen`. Caveats below. |
| D2 | hover (note over span) | `.dl6` rel named `hover_note`, 6 cols, compiles to bare table `hover_note` (`lower.pl:176`); no serve-side reader; intended v5 hover | `hover_notes_at` call `src/lsp.rs:884`; merge `src/lsp.rs:886-897`; `MarkupContent` `src/lsp.rs:906`; impl reads `txt_tbl("hover_note")` `src/engine/lens.rs:294-296` | NO. Broken even with v5. `hover_notes_at` selects `rel_hover_note_txt` (`txt_tbl` naming, `src/lower.rs:10`), never the bare `hover_note` table v6 emits. v5 has no `--hover-db` foreign mode (only `--diag-db`, `src/lsp.rs:495`); `hover_notes_at` runs only in full `--lsp` engine mode against v5's own compiled db. PR #202's own lane measured it could not put a note into an editor. |

D1 caveats: spans are whole-file only (line=col=end_line=end_col=0, the
the `decode/2` wall in `diag-rail.dl6`), and the binary has a shutdown hang after
`exit` + stdin EOF (`lsp-diags.sh` header, downgraded to a SIGKILL-with-grace in
the driver). Neither blocks the measure: D1 is the one v6 editor feature that
reaches an editor through v5 today, and it is the only one.

### 2.2 Findings: what does NOT depend on v5

| # | feature | verdict | evidence |
|---|---|---|---|
| F1 | definition / references / documentSymbol / documentHighlight / call + type hierarchy / workspace/symbol / dl/refs / dl/locate / dl/query / executeCommand mute | NOT a v6-v5 dependency today. None reads a v6-written table; all read v5's own in-proc engine over v5's compiled db via the `Engine` (e.g. `handle_definition` `src/lsp.rs:827`, `handle_references` `src/lsp.rs:1111`, `handle_document_symbol` `src/lsp.rs:1310`). v6 owns none of these; they are v5-only features. Under the constraint they vanish unless v6 buys/builds its own server. | dispatch `src/lsp.rs:248-331`; only foreign literal select `src/lsp.rs:633` |
| F2 | formatting, completion | do NOT exist in v5's LSP at all. Nothing to borrow. If v6 wants them they are net-new build/buy. | dispatch `src/lsp.rs:248-331` (no `textDocument/formatting`, no `textDocument/completion`) |
| F3 | parity / gate scripts that spawn v5 once | Transient v5 spawns for parity, not persistent LSP: `v5-parity.sh`, `comment-parity.sh`, `crawl-bench.sh`, `flagship-callgraph.sh`, `flagship-flow.sh` (all resolve `DL_V5_BIN`/`target/release/dl`). These RUN v5, so the constraint bites them too, but they are not the editor delivery path and are priced separately from part 1. | grep of `v6/tsv2/scripts/*.sh` |

Net inventory: exactly ONE v6 editor feature is delivered through a running v5
and measured working (diagnostics), and ONE was shipped dead on arrival even
with v5 (hover). Everything else v5's LSP offers is independent of v6, not a
v6-v5 dependency, and must be owned by v6 from zero if v6 wants it without v5.

---

## 3. Part 2, build-vs-buy: LSP server candidates, priced for the tsv2 runtime

An LSP server is a common-shaped problem; the LAW requires researched candidate
analysis before "write our own". The runtime under test is fixed: TypeScript,
rxjs spine, `@libsql/client` store seam, an HTTP `serve` path already in the
tree (`v6/tsv2/serve/4_http.ts:559` lines, `serve/main.ts` the single
subscribe). The store's one Promise seam is `ISqlRunner` (`v6/sprefa-store/
js/src/engine/types.ts:58`); everything above it is observables, and the
one-manual-subscribe ratchet (`v6/tools/one-subscribe.sh:39-40`, baselines
dl/src=1, tsv2/serve=1) is a standing law.

### 3.1 Candidates

| name | language | what it gives you | what it costs | what it forces on the architecture | fits TS+SQLite tsv2? |
|---|---|---|---|---|---|
| `vscode-languageserver` (npm 10.1.0) | TS | the full LSP server: Connection over stdio/pipe/socket, TextDocuments manager, feature registration, `sendDiagnostics`, handler types for every LSP 3.18 method (deps: vscode-languageserver-protocol 3.18.2) | pulls the whole connection + its async handler model; owns document text lifecycle; promise/thenable handler returns | an adapter between the rxjs spine and a promise/callback handler surface; diagnostics must be pushed through the connection's own send path, so the spine subscribes once and the connection drives the rest | partial. Small buy, but its async handler model collides with the promise-above-seam law and the one-subscribe rail unless the connection is walled behind one cold observable |
| `vscode-jsonrpc` (npm 9.0.1) | TS | only the JSON-RPC wire: MessageReader/MessageWriter over stdio, Disposable, cancellation, request/response correlation. No feature server, no protocol types in this package | you assemble the LSP feature handling yourself; protocol types come separately | the reader is an event source that composes INTO the rxjs spine: one cold observable per process, requests answered by querying the served store. Keeps the one-subscribe law | yes. Smallest buy that still buys the wire; the store join is v6-owned |
| `vscode-languageserver-types` (npm 3.18.0, zero deps) | TS | types only: every 3.18 data structure (Position, Range, MarkupContent, PublishDiagnosticsParams, ...). No transport, no connection | you also own Content-Length framing + error codes + cancellation | pure types, no runtime; pairs with any framing you choose | yes. Typed wire, all lifecycle is yours |
| roll your own JSON-RPC + hand-typed protocol | TS | nothing bought | ~150-250 LOC Content-Length framing + error mapping + cancellation, and hand-rolling the ~hundreds of 3.18 types is unbounded; would re-implement vscode-languageserver-types almost for free | the full LSP owns you anyway | no. Fails infra-is-bought; the "buy" is the protocol types, not worth rebuilding |
| `langium` (npm 4.3.1) | TS | a complete language-server framework: chevrotain parser, workspace manager, documents, references, default services; bundles vscode-languageserver ~10 + protocol | heavy: imposes its own parser + grammar + AST + document lifecycle | v6 has no parser here (tree-sitter extraction is the Rust crate) and a table-push server that needs none; langium wants to own the thing v6 deliberately does not have | no. Its one strength (grammar-driven AST) is the one thing this program lacks; adopting it means re-homing extraction in TS to feed a model v6 does not use |
| `tower-lsp` / `lsp-server` crate (Rust, from the earlier v6-crate-map era) | Rust | battle-tested Rust LSP server on axum/tokio: `v6/plans/2026-07-19-v6-daemon.md:55` already picked `tower-lsp LspService` per UDS with a `dl lsp-proxy` stdio shim (biome pattern) | the current runtime is TS/tsv2, not the Rust crate the 0719 plan assumed; adopting it splits the editor face across two languages and two process models | the whole v6 runtime would have to be re-homed to Rust, or the Rust LSP server would talk to the TS store over a wire | no. The 0719 daemon direction predates the pivot; the runtime is TS now, so a Rust LSP is a second runtime, not a continuation |
| `vscode-languageclient` / `monaco-languageclient` | TS | client-side / browser-host libraries | they are CLIENT libs; we build the server | / | N/A (opposite side of the wire), listed so the candidate set is not silently missing it |

### 3.2 Neutral reading

The wire and the types are the cheap, high-lottery buy
(`vscode-jsonrpc` 9.0.1 + `vscode-languageserver-types` 3.18.0, both tiny, both
from the MS monorepo, no transitive meaning). `vscode-languageserver` itself
(10.1.0) is also a legitimate buy and is the conventional answer, but it arms
its own async handler model and TextDocuments; under the promise-above-seam and
one-subscribe laws it costs more integration than the bare wire does.
`vscode-languageserver` is NOT dismissed: priced, it is the "convenient but
architecture-hostile" option (3.1 row 1, partial fit); `vscode-jsonrpc +
lsp-types` is the "typed wire, own the join" option (row 2, full fit). langium
and the Rust crate are dismissed with the concrete reasons above (imposed AST /
wrong runtime), not in one line. Roll-your-own framing is the counterfactual
that makes the buy look cheap and is therefore what prices the two winners.

### 3.3 The same service, one process or two (the full forks are part 4)

The LSP is a read-mostly consumer of the same SQLite store the serve path
already computes. Two honest shapes, priced by the `vscode-jsonrpc + lsp-types`
buy and by not-yet-existing feature handlers:

| shape | what it means | incremental cost | fits the standing laws |
|---|---|---|---|
| the LSP is a consumer of the existing serve process | `serve/main.ts` grows an LSP transport: requests answered by the same store, deltas flowed from the same tick chain | wire + feature handlers only (see forks); one process, one subscribe baseline stays 1 | best. Preserves one-subscribe and promise-above-seam; the connection is one more cold observable inside `serve_tsv2` |
| a separate LSP server | a second entrypoint reusing the same runtime + store over the same SQLite file, mirroring v5's `--diag-db` poll or an SSE/EDB bridge | wire + feature handlers + a second process, a second `main.ts` subscribe (baseline 2) unless the entrypoint reuses `serve_tsv2` | weaker. A second manual subscribe unless reused; more process-contract surface |

The pricing body for features lives in part 4. The candidate vote here: buy
`vscode-jsonrpc` + `vscode-languageserver-types`, compose the connection into
the existing serve spine, do not buy `vscode-languageserver`'s connection or
langium.

---

## 4. Part 3, the forks (user rules on design; each is priced, none chosen)

### 4.1 Primary fork: one process vs two

Fork A, the LSP as a consumer of the existing serve process.
- What: the serve process grows an LSP transport; `serve/main.ts`'s single cold
  observable gains an LSP connection that answers requests by querying the same
  store and pushes diagnostics from the same tick chain.
- Costs in files and lines: new `v6/tsv2/serve/5_lsp.ts` (the connection +
  feature handlers), deps `vscode-jsonrpc` + `vscode-languageserver-types` in
  `v6/tsv2/package.json`, a launch flag or alias so the binary is editor-spawned
  over stdio (`TSV2_LSP=1 node --experimental-transform-types serve/main.ts`).
  Diagnostic handlers are ~the delta path; hover handlers read the `hover_note`
  table the engine already derives. Each new class declares its interface in
  `v6/tsv2/serve/types.ts` (the standing law).
- Forces later: the serve process becomes editor-resident (attaches to an editor
  launch, not a daemon); HTTP and LSP requests share one store; the
  one-subscribe ratchet must stay at 1, so the LSP connection is one more cold
  observable composed inside `serve_tsv2`, not a second manual subscribe
  (`serve/main.ts:22`, ratchet `v6/tools/one-subscribe.sh:40`).
- Forecloses: the multi-session daemon shape. A stdio LSP is one process per
  editor launch; the "one engine, many LSP sessions over UDS" plan the 0719
  docs wanted (`v6/plans/2026-07-19-v6-daemon.md:55-57`, `:92-99`) is not this.
- Throw sites: `serve/main.ts:22` (the single subscribe), `4_http.ts` app
  assembly, the store seam `v6/sprefa-store/js/src/engine/types.ts:58`.

Fork B, a separate LSP server process.
- What: a second entrypoint (e.g. `v6/tsv2/serve/lsp-main.ts`) reuses the same
  runtime + store over the same SQLite file, either polling it like v5's
  `--diag-db` or bridging the existing delta tick.
- Costs: a second entrypoint + a second manual subscribe (ratchet baseline `2`
  in `one-subscribe.sh:40` unless the entrypoint re-delegates to `serve_tsv2`,
  in which case it is Fork A in disguise), a second process contract (which file,
  how deltas cross), duplicate boot wiring. Reproduces the v5 external-db
  handshake: poll cadence `src/lsp.rs:515`, path-cwd agreement
  `docs/lsp.md:143-169`, poll latency `docs/lsp.md:171-174`.
- Forces later: process-pair coordination; two processes on one SQLite file
  (WAL lock choreography the single-process shape does not have).
- Forecloses: the single-process story; it is the v5 `--diag-db` shape
  (`src/lsp.rs:495-537`) re-homed to a v6-owned process, not removed.
- Throw sites: the mode being mirrored `src/lsp.rs:495`, `:537`, the gotchas
  `docs/lsp.md:152-174`.

### 4.2 Sub-forks, whichever process shape the user picks

Transport: stdio JSON-RPC (the `vscode-jsonrpc` buy, 4.1) vs SSE-over-HTTP
(the existing `/ticks` SSE, `4_http.ts:414`).
- stdio is the only transport a stock generic-LSP client (VS Code, coc, neovim
  lspconfig) spawns and speaks; it forces a real editor-spawned process, which
  is what forks A and B both already are. `lsp_diag_driver.py` proves a real
  Content-Length stdio client against the v5 binary (`v6/tsv2/scripts/lsp_diag_
  driver.py`).
- SSE over HTTP fits the existing serve shape with fewer new parts, but no
  stock editor client speaks it as LSP; it would force a custom client, which is
  more code than the stdio buy, so it only wins if the editor face is a custom
  panel (the own flow-panel path) rather than a generic LSP.

Feature slice, first v6-owned LSP: diagnostics only (drop-in for the
`diag_v5`-to-v5 path, the one measured working) vs diagnostics + hover (also
replaces the dead `hover_note` sink, part 4, and disposes of PR #202's broken
bridge). Diagnostics alone is the smallest replace; hover is cheap once the wire
exists because the `hover_note` table the engine derives is already present
(`diag-rail.dl6`, `import-hover-rail.dl6`).

None of these is chosen here. The user rules.

---

## 5. Part 4, disposition of PR #202 (the `hover_note` sink)

PR #202 landed a `hover_note` sink (`.dl6` side) that carries no path to an
editor, even with v5 running. Measured evidence: v5's hover reads `txt_tbl(
"hover_note")` = `rel_hover_note_txt` (`src/engine/lens.rs:294-296` via
`src/lower.rs:10`), which the bare `hover_note` table a `.dl6` compile emits
never is; v5 has no `--hover-db` foreign mode (`run_diag_db_mode`, `src/lsp.rs:
495`); `hover_notes_at` runs only in the full `--lsp` engine mode against v5's
own compiled db. The PR's own lane could not put a note into an editor
(`plans/2026-08-12-import-openapi-hover.md:70-90`, `:159-180`).

Disposition options, each true to what was measured:
- useful unchanged: no. As a v5 bridge it is dead weight today; nothing reads
  the bare table.
- useful with a different consumer: yes, as the data source for a v6-owned LSP
  hover (Fork A/B feature slice, 4.2). The derive rules and the 6-column
  `hover_note` schema are exactly the shape a v6 hover handler would query; the
  `import_hover_rail.dl6` and `import-hover-receipt.sh` pieces stay as the
  emit-side receipt.
- dead weight to revert: the V5-BRIDGE part specifically (the claim that naming
  the rel `hover_note` reaches v5) is dead and should be corrected in the
  `v6/prolog/lower.pl:176` bare-name story and the `lsp-diags.sh` header, which
  both overstate what the name buys. The rel itself and its derive receipt stay;
  reverting the sink would cost a working data surface for zero gain.

The one true fix is architectural: treat `hover_note` as v6 data, not as a v5
wire. Under the constraint, PR #202's deliverable becomes "the data side of a
future v6 hover", and its broken v5-reading claim is corrected, not re-shimmed.


---

## 6. Verification

This lane is recon only: zero production code. The plan proves out when a later
lane that owns `v6/tsv2/serve` and the lower seam lands a v6-owned LSP. The
receipts to reuse are already in the tree and do not need v5:

| probe | what it proves | source |
|---|---|---|
| served `/idb/diag_v5` rows appear and retract | the store, not v5, already computes diagnostics | `lsp-diags.sh` phase A (`:200-230`) |
| a real Content-Length stdio LSP client exchanges diagnostics | the wire works against a v6-owned server once it exists | `lsp_diag_driver.py` (`v6/tsv2/scripts/`) |
| `hover_note` rows derive from a served program | the data surface a v6 hover would read | `import-hover-receipt.sh`, `import-hover-rail.dl6` |

A v6-owned LSP's own receipts (HOLDS/FAIL) are a build lane's contract; this
plan only fixes the edge that the current bridge cannot be the receipt host.
`PLANS.md` regen and the `plans-index-drift` rail are not run here: this worktree
owns exactly two paths and touching PLANS.md would violate `git status`.

## 7. Staffing

| slot | value |
|---|---|
| lane type | plan/recon only, zero production code |
| base SHA | `154ae23c` |
| owned files | `plans/2026-08-12-v6-native-lsp.PLAN.md`, `plans/2026-08-12-v6-native-lsp.PLAN.visual.human.unga.md` |
| executes | no build lane here; the first build lane owns `v6/tsv2/serve` file ownership, `v6/tsv2/package.json` deps, and the lower seam (`v6/prolog/lower.pl` bare-name story) |
| parallel lanes | three in-tree (compiler, emitter seam, CI ledger); this doc touches no `src/` and no shared file |
| style | no em dashes; no `here is`/`below is`; tables carry the prose load |
