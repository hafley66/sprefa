# LSP trace-diag feasibility (2026-07-28)

**VERDICT: possible today, tier (a)+(b), zero new engine constructs.** A
`@trace` marker comment on a source line can be resolved into "what other
extraction rel has a row on this line" (tier a) plus "which rule body in the
loaded program reads that rel, read off the program's own source as text"
(tier b), and published as an info-severity `diag` row (hoverable squiggle)
or a multi-line `hover_note` (markdown on hover, no squiggle). Both sinks are
pre-existing v5 builtins; the whole pipeline is a plain `.dl` program, proven
live below. Tier (c) - full per-row derivation (which SPECIFIC body rows of a
DERIVED rule supported this fact) - does not exist; no per-fact support/
derivation table is queryable in the v5 schema today (see Q3).

Demo: `examples/trace-diag-demo/` (new dir, this arc). Run: `examples/trace-diag-demo/run-demo.sh` / `run-demo.sh --check`.

## Q1 - can an info-severity `diag_v5` row reach the editor as a hoverable info diagnostic, zero code changes?

**Yes**, and it is not even limited to the `--diag-db` bridge path - the
native `--lsp` path (no v6 db involved at all) already maps `"info"` to
`DiagnosticSeverity::INFORMATION`:

- Native `diag` rel path: `to_diag`, `src/lsp.rs:1494-1500`
 ```rust
 fn to_diag(d: DiagRow) -> Diagnostic {
 let severity = Some(match d.severity.as_str() {
 "error" => DiagnosticSeverity::ERROR,
 "info" => DiagnosticSeverity::INFORMATION,
 "hint" => DiagnosticSeverity::HINT,
 _ => DiagnosticSeverity::WARNING,
 });
 ```
- `--diag-db` (v6-compat) path: `diag_v5_severity`, `src/lsp.rs:585-592` - same
 four-way mapping, byte-identical shape, default `_ => WARNING` (not an
 error) for anything unrecognized.
- Severity is a plain `TEXT` column, no enum/CHECK constraint at the SQL
 layer: `src/engine/decls.rs:263-270` (`diag_rel_decls`, `Type::Text`) and
 `v6/dl/src/5_diag.ts:22-32` (`diagDecl`, positional `"severity"` string).

Nothing is hardcoded to error/warning only. Severity string values below
proved live in the demo run (`? diag` row, `severity` column = `info`;
`--check` on that row exits 0, not 1 - see the Q2 receipt).

## Q2 - can marker -> trace -> diag be a plain `.dl` program TODAY?

**Yes, entirely**, using existing rules and existing sinks - no engine
change, no v6 bridge required. Confirmed two ways:

1. `diag` is a normal-looking sink rel any rule can head:
 `src/engine/decls.rs:260-270` (`diag_rel_decls`, 9-col fixed schema,
 `path` is `Type::Text` so a synthetic origin is never file-checked away).
 `examples/checked-notes.dl:57-62` heads `diag(...)` directly from a
 derived rule (`broken_note`/`broken_link`), proving rules-write-diag is
 an established pattern, not new.
2. `hover_note` is a second, arguably better-fit sink for "info window on
 hovering": `src/engine/decls.rs:294-304` (`hover_note_rel_decls`, 6-col:
 path/line/col/end_line/end_col/md). It renders through
 `textDocument/hover` as literal markdown, merged with any entity hover:
 `src/lsp.rs:714-761` (`handle_hover`), joined by a `---` separator
 (`src/lsp.rs:743`). `examples/goto-flows.dl:183-192` already heads
 `hover_note` from a derived rule (flow membership), same shape this demo
 uses.

Demo receipt (`examples/trace-diag-demo/trace-diag.dl`, run via
`run-demo.sh`, one-shot `--no-daemon`, scratch db, daemon never started):

```
? trace_marker => path	marker_line	marker_text
examples/trace-diag-demo/src/billing.ts	11	@trace
 (1 rows)

? call_on_marker_line => source_path	marker_line	callee_name
examples/trace-diag-demo/src/billing.ts	11	computeTotal
 (1 rows)

? program_reader_line => program_path	hit_line
examples/trace-diag-demo/trace-diag.dl	46
 (1 rows)

? traced => source_path	marker_line	marker_text	callee_name	reader_lines_json
examples/trace-diag-demo/src/billing.ts	11	@trace	computeTotal	[46]
 (1 rows)

? diag => path	line	col	end_line	end_col	severity	code	msg	hint
examples/trace-diag-demo/src/billing.ts	11				info	trace	tier(a): line 11 calls `computeTotal` -- tier(b): call_site is read at trace-diag.dl line(s) [46]	
 (1 rows)

? hover_note => path	line	col	end_line	end_col	md
examples/trace-diag-demo/src/billing.ts	11	0	11	200	**trace @ @trace**
- tier(a): this line also produced a row in `call_on_marker_line` (callee = `computeTotal`)
- tier(b): `call_site` is read by a rule body at trace-diag.dl line(s) [46]
 (1 rows)
```

`--check` receipt (proves info severity never fails CI):

```
examples/trace-diag-demo/src/billing.ts:11: info[trace]: tier(a): line 11 calls `computeTotal` -- tier(b): call_site is read at trace-diag.dl line(s) [46]
exit=0
```

## Q3 - what trace CONTENT is reachable without new engine work?

Three tiers as specified, ranked, with which is reachable TODAY:

- **Tier (a) - "this line yields rows in rels X, Y" from extraction rels
 alone: REACHABLE TODAY**, proven in the demo. `comment_node` gives the
 marker's own row (`src/engine/decls.rs:553-560`, grammar-backed per
 language, `src/comment.rs:1-6`); `call_site` gives what else lands on the
 same 1-based line (`src/engine/decls.rs:603-606`, columns
 `repo,caller,callee,file,line`) with a plain equality join on `line` - no
 coordinate translation needed (unlike some other spine rels that carry
 byte offsets instead of a line number).

- **Tier (b) - "which rules read those rels" from static rule metadata:
 REACHABLE TODAY, but only by a generic technique, not a purpose-built
 catalog.** There is no rule-dependency introspection rel in v5 - searched
 for a rule/rel-dependency catalog (`rule_dep`, `dl_rule`, `dl_atom`,
 `rel_dep`, `body_rels`, `reads_rel`, `RuleMeta`) across `src/engine/*.rs`
 and `examples/*.dl`: no hits. The one self-introspection surface that
 exists is `dl_diag` (used by `examples/lint-dl-self.dl:1-37` to typecheck
 `.dl` files as data), which validates a `.dl` file's syntax/types, not its
 rule/rel dependency graph. What IS reachable today: since a `.dl` program
 is itself an ordinary file, the generic `match_line` source op
 (`src/engine/decls.rs:225`, line-regex over file content) can scan the
 loaded `.dl` program's own text for a rel name appearing as a body atom - 
 proven in the demo (`program_reader_line`, matching `\bcall_site\(` over
 `trace-diag.dl` itself, finding the real body-atom line). This is
 necessarily approximate (textual, not parsed - a rel name inside a prose
 comment would also match; the demo avoids that by never writing
 `call_site(` with a trailing paren in prose) and program-self-referential
 only (it does not resolve `use "..."`-imported `.dl` files' rule bodies).
 Caveat: `match_line`'s own doc says it is for FLAT TEXT "never structured
 source code" and that `match_ast` is "the correct tool for SOURCE CODE"
 (`src/engine/decls.rs:225`). This demo uses `match_line` on a `.dl` file
 anyway because the target substring fits on one line and there is no
 `match_ast :lang` grammar for `.dl` itself - it works here by not tripping
 the documented failure mode (a multi-line construct), not because the op
 is endorsed for this genre of input.

- **Tier (c) - full per-row derivation (which specific body rows produced
 this derived fact): NOT REACHABLE without new machinery.** Searched for a
 per-fact support/derivation table: `src/why.rs` and
 `src/invlog.rs` are DAEMON self-diagnosis (what was the daemon doing, who
 spawned it - process/tick level, not per-fact), matching the standing
 self-diagnosis law's own framing. `src/engine/derive.rs` is fixpoint
 execution machinery (watchdog for slow statements), not a support table.
 The one adjacent table, `_prov` (`src/engine/meta.rs:225`,
 `CREATE TABLE IF NOT EXISTS _prov (rel, repo, path, src)`), tracks which
 EXTRACTION OP populated a rel for a given path (for incremental
 re-extraction skip-state), not which body rows of a DERIVED rule supported
 a specific output row. `src/engine/deltaflow.rs` is an explicit
 test-only fixture (`//! This module deliberately... does not participate
 in production execution`, `src/engine/deltaflow.rs:1-5`) proving a
 Z-set/weight design for future count-IVM, not a shipped derivation log.
 Building tier (c) would mean either (i) a semi-naive join log per rule
 firing (real storage/perf cost - the standing plan's storage-diet arc is
 actively fighting db bloat) or (ii) re-running the rule body as a query
 seeded by the fact's own columns at trace time (cheaper, approximate when
 a rule has multiple ways to derive the same row - SQL fixpoint dedup means
 a row's actual firing history is not preserved after commit).

## Q4 - multi-line trace text and `relatedInformation`

- **Multi-line messages work.** `Diagnostic.message` is a plain `String`; no
 truncation logic exists in either publisher (`to_diag`, `src/lsp.rs:1511-
 1514`; `diag_v5_to_diagnostic`, `src/lsp.rs:603-606` - both just
 `format!("{}\nhint: {h}", msg)` when a `hint` is present, i.e. the
 publisher ITSELF already appends a literal `\n`). The `.dl` lexer's
 backtick string is raw and genuinely multiline (`` ` ``, terminated only
 by the next backtick, `src/lex.rs:110-123`, comment: "raw, multiline, only
 the closing backtick terminates"), proven in the demo's `hover_note.md`
 (three real newlines via `"..." + `\n` + "..."` concatenation - see the
 receipt above, rendered literally in the TSV `? hover_note` output).
 `hover_note` renders through `HoverContents::Markup(MarkupKind::Markdown)`
 (`src/lsp.rs:755-758`), so an editor renders the markdown (bullet lists,
 bold, etc.), not just literal linebreaks.
- **`relatedInformation` is NOT populated by either publisher.** Both
 `to_diag` (`src/lsp.rs:1516`) and `diag_v5_to_diagnostic`
 (`src/lsp.rs:608`) construct `Diagnostic { ..Default::default() }`, and
 neither sets `related_information` anywhere in `src/lsp.rs` (grepped for
 `related_information`/`RelatedInformation`/`DiagnosticRelatedInformation`:
 zero hits in `src/lsp.rs` or `src/engine/*.rs`). Adding it would need (i) a
 new `diag`-adjacent sink rel - e.g. `diag_related(code, path, line,
 related_path, related_line, related_msg)` - or extra columns on `diag`
 itself, plus a small `to_diag`/`diag_v5_to_diagnostic` change to populate
 `Diagnostic.related_information`. Small, additive, not attempted here per
 the report-only mandate (`src/lsp.rs` is existing source).

## Q5 - the v6/tsv2 angle (sketch, not built)

Once phase D lands (`.dl` DCG + hosts, per the v6 standing plan), the prolog
emitter would target the SAME `diag_v5` 9-col contract v5's `--diag-db` mode
already polls (`src/lsp.rs:395-409`, `DiagV5Row`; `v6/dl/src/5_diag.ts:19-41`,
`diagDecl`/`DIAG_V5_VIEW_SQL`) - the contract is a view over `rel_diag`, so a
tsv2-generated TypeScript program only needs to write rows shaped like
`diagDecl`'s 9 columns (or head a rule that projects into it) and the
identical v5 `dl --lsp --diag-db <db>` process picks them up on its existing
500ms poll (`diag_db_poll_loop`, `src/lsp.rs:457-517`), unchanged. A tsv2
trace-diag program would follow the exact same marker->trace->diag shape
this demo uses, with `comment_node`-equivalent and `call_site`-equivalent
facts coming from whatever spine rels the tsv2 target program declares
(`v6/dl/src/5_diag.ts:48-56`, `spineDecls`, is the v6-side analogue but is
off-limits to edit per this arc's scope - cited for the sketch only). Tier
(b)'s "read the program's own source as data" trick ports directly (tsv2
programs are still text files on disk); tier (c) is exactly as absent on the
v6 side - `v6/sprefa-store/js/src/engine/engine.ts:177-292` implements a
weight/support counter for count-IVM retraction ("weight = # of derivations
supporting a row... row dies only when weight reaches 0"), which is closer
to tier (c) than anything on the v5 side, but it is a LIVE counter for
retraction correctness, not a queryable per-row derivation log; and
`v6/tsv2/runtime/ticklog.ts:1-12` is a per-TICK delta log (which rows were
added/deleted THIS tick), not a per-ROW derivation trace - it would tell you
a row appeared at tick N, not which body rows produced it.

## Smallest demo path (exact commands)

```
cd ~/projects/sprefa # or this worktree
examples/trace-diag-demo/run-demo.sh # full receipt: every queried rel
examples/trace-diag-demo/run-demo.sh --check # CLI-rendered diagnostic, exit code
```

Equivalent manual form:
```
dl examples/trace-diag-demo/trace-diag.dl --no-daemon --db /tmp/scratch.sqlite
dl examples/trace-diag-demo/trace-diag.dl --no-daemon --db /tmp/scratch2.sqlite --check
```
Never `--lsp` in this arc (would need a real editor client to observe the
squiggle/hover; the `--check`/query receipts above are the code-path proof - 
`to_diag`/`diag_v5_to_diagnostic` are the same functions an `--lsp` session
calls).

## Ranked list: what would need building for the full vision

1. **Tier (c) support/derivation log** (biggest lift): either a per-rule-
 firing join log (real storage cost, fights the storage-diet arc) or a
 query-time "explain this row" that re-runs the rule body seeded by the
 row's own columns (cheaper, but approximate under multi-way derivation - 
 SQL fixpoint commit does not preserve WHICH firing produced a row after
 dedup).
2. **A real rule/rel dependency catalog** (tier (b) done properly instead of
 textual `match_line` over the program's own source): something the
 stratifier already computes internally (`src/engine/strata.rs:476`,
 `stratify`) but does not expose as a queryable rel. Exposing it would
 replace the demo's `match_line`-over-self hack with an exact "every rule
 whose body reads rel X" answer, immune to a rel name appearing in prose.
3. **`relatedInformation` support** in `to_diag`/`diag_v5_to_diagnostic`
 (Q4): small, additive, would let a trace diag point at a SECOND location
 (e.g. the def site of the traced callee) instead of packing everything
 into one message string.
4. **A `Key(text)`-shaped marker convention** if the user wants a spelling
 nicer than a bare regex match on comment text (e.g. a dedicated
 `trace_marker` builtin op instead of `comment_node` + `=~ /@trace/` by
 hand) - not required for feasibility, purely ergonomics; explicitly
 deferred by the "no new engine constructs unless a program PROVES a gap"
 standing discipline, since this demo proves comment_node already
 suffices.
5. **v6/tsv2-side `diag_v5`-contract emission** (Q5): unstarted, blocked on
 phase D; sketch only, no code.

---
Directed by: Chris Hafley
Implemented by: Claude (feasibility study agent)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>
