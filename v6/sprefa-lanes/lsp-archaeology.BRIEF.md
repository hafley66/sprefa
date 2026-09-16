# BRIEF: recover every LSP and editor incarnation, price a standalone editor area

## Base
Confirm the base with `git log --oneline -1` before your first commit. The spawn
printed the sha; that is your base. The ordering is not a gate. If a procedural
line in this brief seems to forbid otherwise-correct work, the work wins: note
the conflict in your report and keep going.

**Docs only. Write ZERO implementation code.** Two plan docs are the deliverable.

## The user's words, verbatim

> "also dig out all the lsp plugin work for vscode. i dont want tons of code in
> vscode and stick to lsp as much as possible. i just want to generically
> enhance it / have a place for that. but i want that as its own lib/app area as
> well bc im tired of dragging it. do not think i am talking about sprefa v5's
> giant graph viewer ui / that is a different ambition getting decomposed later
> when this lang satisfies me"

Four constraints, and every recommendation must satisfy all four:
1. **Thin client.** As little TypeScript in the VS Code extension as possible.
   Anything that CAN be an LSP capability MUST be one.
2. **A place for generic enhancement.** There must be a defined slot for
   editor features that LSP does not cover, so they do not sprawl.
3. **Its own lib/app area.** Not welded into the compiler tree. The user is
   tired of dragging it between versions.
4. **The graph viewer is OUT OF SCOPE.** `editors/vscode-dl` is 6103 lines and
   it is a different ambition for a later day. Inventory it in one row, then
   leave it alone. Do not design for it, do not price it, do not fold it in.

## What exists. Measured, verify each line.

| version | LSP server | lines | editor client |
|---|---|---:|---|
| v3 | `v3/crates/server/src/transport_lsp.rs`, `v3/crates/sprefa/src/server/transport_lsp.rs`, `v3/crates/sprefa/src/server/lsp_layer.rs`, `v3/crates/pipeline/src/ops/lsp.rs` | measure | none found |
| v3 tests | `v3/tests/smoke/_f_lsp_hover_ast.sh`, `_i_lsp_invalidate_kernel.sh`, `_k_lsp_diag.sh` | | |
| v4 | `v4/crates/sprefa-lsp/` incl. `main.rs` 579 and `inlay.rs` 111 | **1077** | `v4/editors/vscode/` with `syntaxes/sprf.tmLanguage.json`, `language-configuration.json` |
| v4 plans | `v4/plans/lsp-fs-watcher-reactive-wake.md`, `lsp-loop-justification-lint.md`, `lsp-sprf-component-n-plus-1-lint.md` | | |
| v4 tests | `v4/tests/lsp_hover_smoke.rs`, `v4/tests/lsp_locate_dsl_smoke.rs` | | |
| v5, current | `src/lsp.rs` | **2000** | `editors/vscode` (a built `.vsix`), `editors/vscode-dl` **6103, OUT OF SCOPE** |
| v6 | none native | 0 | reaches an editor only through v5 |

Archives: `~/projects/sprefa-archive-20260701` holds `v3/`, `v4/`, `v5cozokuzu/`.
`~/projects/sprefa-archive-20260428` is the original.

v5's `src/lsp.rs` answers these methods, measured by grep:
`textDocument/didOpen`, `didSave`, `publishDiagnostics`, `hover`, `definition`,
`references`, `documentHighlight`, `documentSymbol`, `prepareCallHierarchy`,
`prepareTypeHierarchy`.

## READ THIS FIRST so you do not redo it

`plans/2026-08-12-v6-native-lsp.PLAN.md` (250 lines) and its unga twin already
exist from a recon earlier today. It established, with citations:

- v6's `serve_tsv2` is already a fused CLI + HTTP server whose channels merge at
  one point; adding LSP is one more source in that merge.
- the build-vs-buy verdict was `vscode-jsonrpc` (bare wire) over
  `vscode-languageserver` (which wants to own the connection), precisely because
  of that merge.
- PR #202's hover through v5 could never have worked: v5 has `--diag-db` but no
  `--hover-db`, and `hover_notes_at` reads `rel_hover_note_txt`, never the bare
  table v6 emits.
- the net cost of dropping v5 is ONE feature, diagnostics, not a suite.

Do not re-derive any of that. Cite it and build on it. If you find it WRONG,
that is a top-of-report finding with the evidence.

## Deliverable 1: the archaeology

For each of v3, v4, v5, answer with `file:line` citations:

| question |
|---|
| which LSP methods were implemented |
| what transport was used (stdio, HTTP, custom) and what library, if any |
| what lived in the editor client that could have been an LSP capability instead |
| what could NOT be LSP, and why (the interesting rows) |
| how the server learned about file changes: LSP notifications, a watcher, both |
| what testing existed and whether it exercised the wire or only the handlers |
| why it did not survive to the next version, where you can tell |

A comparison table across versions is the centrepiece. Where a version lacks a
row, write "absent" rather than leaving a blank.

Pay attention to v4's `inlay.rs` (111 lines) and the three v4 lint plans. Inlay
hints and diagnostics-as-lints are exactly "generic enhancement through LSP",
which is constraint 2. Say what those plans intended and whether it shipped.

## Deliverable 2: the thin-client line

Produce a table with one row per editor-facing feature the project has ever had
or wanted, and place each one:

| feature | LSP capability that covers it | or: why it cannot be LSP | verdict |

LSP covers far more than most extensions use. Check the current specification
for: inlay hints, code lens, semantic tokens, document links, folding ranges,
selection ranges, call and type hierarchy, code actions, rename, formatting,
diagnostics with related information, workspace symbols, execute-command,
workspace edits, file operation notifications, progress reporting, and
`window/showDocument`. State the specification version you checked.

For anything genuinely outside LSP, that is the "place for generic enhancement"
the user asked for. Define what that slot is and what its rules are, so it does
not become the next 6103-line thing.

## Deliverable 3: build-vs-buy. STANDING LAW, not optional.

Never assert "write our own" for a common-shaped problem without library
research and a written candidate-by-candidate analysis first. No one-line
dismissals. Infra is bought, never built.

Research and price, with maintenance status and API shape:

| problem | candidates (find others) |
|---|---|
| speaking LSP from TypeScript | `vscode-languageserver`, `vscode-jsonrpc`, `vscode-languageserver-protocol` |
| speaking LSP from Rust | `tower-lsp`, `async-lsp`, `lsp-server`, `lsp-types` |
| the client side | `vscode-languageclient`, and what a minimal client actually needs |
| position and offset math | `line-index`, `lsp-positions`, `ropey` |
| testing an LSP over the wire | `@vscode/test-electron`, `vscode-languageserver` test harnesses, plain stdio golden tests |
| packaging a thin extension | `@vscode/vsce`, and whether a generated extension beats a hand-written one |

Two repo skills cover the Rust side and you should read rather than re-derive:
`sprf-lsp-server-libs` and `sprf-lsp-multi-dsl-patterns`. Find them under the
repo's `.agents/skills/` or `~/projects/claude-research/`. Cite what you use.

Note the tension to resolve explicitly: the earlier plan chose `vscode-jsonrpc`
in TypeScript because v6's server is TypeScript and already owns a merge point.
v4 wrote its server in Rust. Say which language the server should live in NOW,
given that v6 is TypeScript and a Rust engine is being built alongside it, and
given the user's standing decision that no design may end in "keep the v5 binary
running".

## Deliverable 4: the shape recommendation

The user wants "its own lib/app area". Answer concretely:

- where does it live: a sibling package under `v6/`, a separate repo, a
  workspace member? Price each.
- what is its dependency direction? It must depend on the compiler's output, and
  the compiler must not depend on it.
- what is the wire between the editor extension and the server?
- how does the extension get built and published, and how thin is thin? Give a
  target line count for the extension and justify it against what v4's client
  actually needed.
- how do the two existing trees end: is `editors/vscode` (the `.vsix`)
  superseded, and does `editors/vscode-dl` (out of scope) keep working
  independently or get parked?

Present these as FORKS with prices. The user rules. Your job is to make the
choice cheap by having measured it.

## Anti-cheat

| tempting shortcut | why it is a lie |
|---|---|
| summarising a version from its README | READMEs go stale; cite lines you read |
| "v3 was basically v4" | measure it or say you did not |
| designing for the graph viewer | explicitly out of scope, four times now |
| recommending a bespoke server with no candidate table | violates a standing law |
| skipping the LSP spec check | the whole point is that LSP covers more than we used |
| picking the fork yourself | you price, the user rules |
| skipping the unga doc | a plan without it is undelivered |

## Deliverables, exactly two files

1. `plans/2026-08-12-lsp-archaeology.RESEARCH.md` — opens with a table of
   contents. Every claim carries `file:line` or a command and its output.
2. `plans/2026-08-12-lsp-archaeology.RESEARCH.visual.human.unga.md` — plain
   words, diagrams, zero citations. REQUIRED.

Form: tables, lists, mermaid. Prose is a one-line caption under a diagram. Use a
mermaid sequenceDiagram for the editor-to-server flow and a comparison table for
v3 against v4 against v5 against v6.

## File ownership
YOURS: the two plan docs only. Everything else, including both archives, is READ
ONLY for this lane.

## Style laws, inline
- No em dashes. Banned in prose AND identifiers: `provenance`, `substrate`,
  `load-bearing`, `regime`. Use source/origin, base layer, critical, mode.
- The word "refusal" is banned in prose; an error for an unbuilt construct is
  "TODO" or "not built yet".
- No sycophancy, no negative parallelism ("not X, Y" / "this isn't X. it's Y").
- The 10-second law: any operation over 10s is a defect, not a budget.
- Docs open with a table of contents.
