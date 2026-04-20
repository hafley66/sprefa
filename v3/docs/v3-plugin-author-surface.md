# Appendix: v3 plugin author surface

The op-author lifecycle, tabled. Goals:

1. **Maximize pleasantness** — one folder per op; no central enums to edit.
2. **Minimize least harmonics** — each touchpoint has one clear home,
   one trait slot, one invocation site. No cross-cutting match arms.
3. **Minimize inputs** — an op author fills only the rows that apply
   to their op. Every row has a default.

The four phase groups below are the full set of hooks the framework
invokes. Nothing else exists. If you want to add a capability, it
maps to one of these rows or the row does not exist yet.

---

## Phase A — Static (parse + lower)

Fires at `.sprf` load / edit. Pure over source text. Output feeds LSP
and runtime both.

| # | touchpoint | when fires | what op provides | framework consumes for | v2 today | v3 |
|---|---|---|---|---|---|---|
| A1 | op name registry | startup | `NAME: &str`, `parse(OpArgs)` | host-parse dispatch | `inventory::submit` in `ops/*.rs` | same, one per op folder |
| A2 | args grammar | host-parse | `parse(args)` → op struct or `Err<Diag>` | build `OpInvocation` | inline in op | same |
| A3 | sub-lang body extract | host-parse (inside `op[lang](<<P>>) { ... }`) | `body_delim`, `sub_lang_kind` | tolerant ranges; tree-sitter `set_included_ranges` | partially; ast-grep lang block | full per `_1_ast-grep-extension.md` |
| A4 | capture decls | lower | `decl_captures() -> [CaptureName]` | build decl-scope per pipeline tree | implicit via `$X` parse | explicit op surface |
| A5 | capture uses | lower | `use_captures() -> [PathExpr]` | undeclared-ref diag | implicit | explicit |
| A6 | coord requirements | lower | `needs: { file?, byte_range?, parsed_tree? }` | plan validation | ad-hoc | explicit |
| A7 | emit schema | lower (Phase 2) | `schema() -> RowSchema` | sqlite DDL, scanner-hash | stub `register_expr_schema` | `QueryStore` effect + schema kind |
| A8 | parse-time diag | A2 failure | `impl Diagnostic for OpParseErr` | LSP publishDiagnostics + CLI stderr | op-owned per Inv 1 | same |
| A9 | lower-time diag | A4/A5/A6 failure | `impl Diagnostic` for algebra errors | same | ad-hoc | centralized in `parse/dag.rs` |

---

## Phase B — LSP (interactive)

Fires on hover / completion / signature help / code action. All
respond from the same artifacts Phase A produced, so tolerant-parse is
the only extra cost.

| # | touchpoint | when fires | what op provides | framework consumes for | v2 today | v3 |
|---|---|---|---|---|---|---|
| B1 | completion at op slot | user types op name | framework scans registry | complete `sem` / `ast_grep` | `analysis::completions_at` | same |
| B2 | completion inside args | cursor inside `sem(|)` | `completions_in_args(partial, cursor)` | op-owned token space (entity kinds, pattern metavars) | partial-eval lane in `analysis.rs` | same but op trait slot |
| B3 | signature help | `(` or `,` | `signature() -> SigInfo` | tooltip | stub | explicit slot |
| B4 | hover on op name | hover over `sem` | `hover_op() -> Markdown` | doc popover | `Op::hover_op` slot | same |
| B5 | hover on capture decl | hover over `$CLASS` at decl site | `hover_capture(name) -> Markdown` | show "class entity, from sem" | `analysis.rs` dispatches via `CaptureKind` | dispatched to `CaptureKind::hover_md` |
| B6 | hover on capture ref `&.$X` | hover over `&.$CLASS` downstream | delegates to decl op via path resolution | show "refers to $CLASS from sem(...)" | `cursor_ref` op + `DocSession` dispatcher | same, cleaner |
| B7 | hover on coord field | hover over `&.fs` / `&.repo` / `&.rev` | `hover_field(field)` | generic field hover | in `DocSession` | op-owned if op introduces new fields |
| B8 | hover on match output | hover over captured span after run | `hover_match(capture, hit)` with evidence | show matched content, file link, table of emitted rows | RunReport path | same |
| B9 | code actions | hover + range | `code_actions(range) -> [CodeAction]` | preview mutation, rename cap, extract pattern | Phase 2 | slot |
| B10 | LSP tolerant parse | on every keystroke | same A1–A3 pipeline in tolerant mode | partial tree for completions | `host_parse(tolerant=true)` | same |
| B11 | live diag publish | after tolerant parse | reuse A8/A9 output | publishDiagnostics | `DocSession::on_source_change` | same |

---

## Phase C — Runtime (pipe body)

The cursor-transform streaming loop. This is the hot path; every
surface here must honor the content contract and the cancellation
spine.

| # | touchpoint | when fires | what op provides | framework consumes for | v2 today | v3 |
|---|---|---|---|---|---|---|
| C1 | pipe transform | runner schedules | `pipe(ctx, stream) -> stream` | cursor flow | `Op::pipe` on `Arc<[Cursor]>` | same; ops call `ctx.put` only |
| C2 | content contract | inside pipe, byte-reading | PATH A slot → PATH B `cursor.content[byte_range]` → PATH C reader | avoids re-reading upstream work | Inv 3 | `ReadBytes` effect obeys order |
| C3 | cancellation check | inside pipe loops | honor `ctx.cancel` | abort on reparse | `TaskGuard` / `CancellationToken` | same |
| C4 | per-batch upstream call | inside pipe | batch `reader.read(ReadBatch)` or `ctx.put(Effect)` | kill N+1 | LAWS-OF-MIN collapse (in-flight) | `Batcher<E>` owns it |
| C5 | runtime diag emit | mid-pipe failure | emit `Diagnostic` on `OpCtx.diags` | surface to LSP + CLI | `RunEvent::Diag` | same |
| C6 | capture stamping | per emitted cursor | attach `Capture` / `CaptureKind` | downstream `&.$X` + hover | inline enum today | `CaptureKind` trait in v3 |
| C7 | evidence enrichment | per emitted cursor | `OpEvidence` annotation | traceability in store rows + hover | present | same |
| C8 | scan-pointer stamping | per emitted cursor | command-side vs content-side sigil + `Tri` verified | assumption checker, xref keying | `memory/project_scan_pointer_runtime.md` direction | same |
| C9 | slot write | per emitted cursor | `SlotKey<T>` typed insert | downstream op reads via key | `Cursor.slots` | same |
| C10 | pipeline tree tagging | framework does | op provides nothing | `SprfPath` per cursor | `Pipeline::run` tags leaves | same |
| C11 | file-size cap | per file | framework checks `runtime.max_file_bytes` | emit `FileTooLarge` diag | landed `70be3e5` | same |

---

## Phase D — Side effects

Deferred queue, drained post-pipeline. Approval gate in the middle.

| # | touchpoint | when fires | what op provides | framework consumes for | v2 today | v3 |
|---|---|---|---|---|---|---|
| D1 | queue mutation | during pipe | `Arc<dyn MutationEffect>` on mpsc | deferred flush | Phase 2 scaffolding | `WriteEdit` effect kind |
| D2 | approval policy | post-pipe, pre-apply | `approval_policy()` → Auto / Cli / Lsp | `MutationHandler` routing | 3 impls | one impl + enum field |
| D3 | approval prompt | handler pulls from mpsc | `RunEvent::MutationPrompt { preview }` | LSP code action or CLI y/n | `mutations.rs` | same effect batcher |
| D4 | preview render | on prompt | `render_preview(effect) -> Diff` | LSP hover over prompt, CLI stdout | slot | `TemplateRender` effect |
| D5 | apply | after approval | `apply(effect) -> ApplyResult` | edit buffer / fs / git | dispatches through Writer trait | `WriteEdit` batcher |
| D6 | mutation cache | pre-apply | scanner hash + effect hash | Skip / Stale / Emit | Phase 2 (G9) | same |
| D7 | store persist | post-apply | `QueryStore::Persist` rows | sqlite, FTS5 | Phase 2 | `QueryStore` effect |
| D8 | reparse safety | on source change | handler abort via `TaskGuard::drop` | cancel in-flight mutations before new wave | present | same |

---

## Cross-cutting: surface → stages map

Each user-visible surface draws from a fixed set of rows above. When
something looks wrong in a surface, this is where to start.

| surface | sources |
|---|---|
| LSP publishDiagnostics | A8, A9, C5, C11 |
| LSP hover | B4, B5, B6, B7, B8, D4 |
| LSP completion | B1, B2 |
| LSP code actions | B9 |
| CLI stderr | A8, A9, C5, C11 |
| CLI stdout per-cursor | C1 output stream rendered |
| CLI approval prompt | D2, D3 |
| store rows + FTS | A7, C7, C8, D7 |

---

## Minimum viable op

The smallest op fills only four rows. Every other row has a default.

| row | what a trivial op provides |
|---|---|
| A1 | `NAME = "myop"` |
| A2 | `parse(args)` returning the op struct (or `()` if argless) |
| C1 | `pipe(ctx, stream) -> stream` |
| C6 | one `CaptureKind` impl (or none if op does not emit captures) |

Everything else defaults to no-op: no completions, no hover beyond the
op-name tooltip (which is the docstring), no side effects, no schema,
no diagnostics beyond framework-generated "unexpected-token."

---

## Inv-1 audit: where v3 still breaks "ops own everything"

Any row where the framework would need to `match` on op kind is a
failure of Inv 1. These are the remaining weak points and their v3
fixes:

- **CaptureKind closed enum** (B5, C6) — today a fixed set. v3 turns
  it into a trait so ops register new capture payloads in one file.
- **Diagnostic rendering** (A8, A9, C5) — already trait-based per
  `_1_diagnostic.rs`. Fine.
- **Mutation approval routing** (D2) — enum-of-3 handlers today; v3
  collapses to one impl with an `ApprovalPolicy` enum field.
- **RunEvent enum** (D3) — v3 eliminates the enum entirely; every
  event becomes `ctx.put(Effect) -> E::Response` per
  `20260418.2.v3-design-and-numbers.md`.

---

## Sem-integration worked example

For `sem(class::$CLASS)` — semantic entity query over the sem tool at
`~/projects/ext/sem` — the rows an author fills:

- **A1–A6** — op surface, `args = SemPath`, `decl_capture = [$CLASS]`,
  `needs = { file: true }`, no byte_range pre-narrow, no parsed tree.
- **A7** — emit schema (if persisted):
  `(class_name, file, range_start, range_end, kind)`.
- **A8** — parse diags for bad sem path (`class::` without selector).
- **B1–B5** — completion for sem entity kinds (`class`, `fn`, `trait`,
  `mod`); hover on `$CLASS` = "class entity from sem"; hover on `sem`
  op = "semantic entity query via sem binary".
- **B8** — hover on captured class = name + range + refs count.
- **C1–C4** — pipe: batch cursors by file, `ctx.put(SemEntityQuery
  { files })`, emit one cursor per (input × entity).
- **C5** — runtime diag if sem binary missing / db stale.
- **C6** — `SemEntityCapture` as `CaptureKind` impl (v3) or enum
  variant (v2 today).
- **D** — none; sem is read-only.

One folder. Three files (op + batcher + grammar hook). Framework
untouched.

---

## Contrast: same op in v2 today

To land sem end-to-end without v3:

- grammar: register op name in `_8_parse.rs`
- op file: `ops/_11_sem.rs` owns parse, pipe, hover, diags
- reader batch: add `ReadBatch::SemEntities { files, kind }` variant
  and match arm in `GitBlobReader::read`
- capture: add `Captured::Sem(SemEntity)` enum variant; add hover arm
  in `analysis.rs`
- tests: extend `OpCtx::for_test` if any new ctx field is introduced

Edit surface today (post LAWS-OF-MIN): three files if capture stays
enum; two if reader absorbs via existing `ReadBatch` shape. The
central enums grow either way.

---

## Reading order

1. `v2/docs/_1_ast-grep-extension.md` — op-as-composition of ast-grep
   YAML rules; sets the mental model for sub-lang blocks (A3).
2. `chat_log/20260418.0.v3-effect-algebra-and-harmonization.md` — v3
   effect algebra locked in; the source of the Phase C shape.
3. `chat_log/20260418.2.v3-design-and-numbers.md` — LoC delta,
   per-effect batching policy, migration path.
4. `appendix/convergent-evolution-effect-dispatcher.md` — why four
   ecosystems converged on this surface.
5. `appendix/v3-vs-v2-reading-preview.md` — what a v3 op looks like
   to read vs v2 today.
