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
`decode/2` refusal in `diag-rail.dl6`), and the binary has a shutdown hang after
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
