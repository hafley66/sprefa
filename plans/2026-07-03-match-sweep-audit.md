# match() sweep — audit of shipped .dl programs

Date: 2026-07-03. Source: full-context read of every `match(` site in
examples/*.dl, std/*.dl, .dl/*.dl (76 real sites; 6 grep hits excluded:
4 `enum_match(` identifier substrings, 2 doc-comment mentions).

## Why

House rule (long-standing, now enforced by this sweep): `match` is brute-force
regex over text. On languages the engine parses (Rust/TS/TSX/Kotlin — and now
go/py/bash/hcl/starlark/jsonnet/gotmpl/dockerfile via ast) the structured rels
must carry the fact instead: ast/sg patterns, type_entity/call_def, call_site/
call_edge, comment(), doc_comment, json/jsonp. match survives only for text
with no parse handle. Two extra reasons beyond hygiene: (1) the dominant abuse
is the `match(p, rev, /./, line)` interest-decl idiom — a bare 2/4-arg `scan`
already populates `_file` and triggers extraction (the index_over idiom,
documented in examples/lsp-def-target.dl), so the regex pass is pure waste;
(2) examples ship as the language's teaching corpus — every match site trains
users toward regex.

## Verdict counts

- REPLACEABLE: 53 (sweep now)
- GRAY: 15 (blocked on a named engine gap — leave, gaps listed below)
- LEGIT: 8 (genuinely unparsed text — leave)

## REPLACEABLE (53) — the sweep

### Bucket A: interest-decl / scan-forcer (~25 sites, mechanical)
`seen(p) <- scan(...), match(p, rev, /./, line)` (or equivalent): drop the
match atom, use plain 2/4-arg scan (index_over idiom).
Files: inspect_pairs.dl:3, anim-deck.dl:10, debug_type_link.dl:3,
type_coincidence.dl:22, type_profile.dl:20, lint-imports.dl:15,
type_lgg_query.dl:13, graph_score.dl:34, debug_type.dl:3, anim-self.dl:14,
typegraph-anim.dl:10, field_matrix.dl:6, module-history.dl:16+20 (keep 4-arg
scan, rev already bound), typeports.dl:10, refactor-discovery.dl:11 (drop the
`fn \w+` filter too), gen-type-table.dl:21, typegraph.dl:25,
interface-soup.dl:21, debug_lgg.dl:3, gen-doc-index.dl:792.

### Bucket B: decls → type_entity / call_def
- anim-deck.dl:75 `pub struct/enum NAME` → type_entity kind IN (struct,enum)
- call-seams.dl:17 fn decls → type_entity kind IN (function,method)
- string-fns.dl:18, dup-collapse.dl:21, callgraph.dl:15, std/callgraph.dl:27
  `fn NAME` → type_entity kind=function / call_def
- recompute-guard.dl:26 fn decls, :34 `fn embed_graph` → type_entity/call_def
- openapi-lsp.dl:24 `fn NAME(` → type_entity/call_def

### Bucket C: call sites → sg / call_site / call_edge
- rails.dl:37+49, rails-call-kind.dl:26, .dl/rails.dl:34 `.conn()`/`.unwrap()`
  → sg(:rust, "$X.conn()") etc. (.dl/rails.dl already does ast_yaml for unwrap)
- call-seams.dl:21 `self.method(` → sg(:rust, "self.$C()")
- openapi-lsp.dl:19 bare callee → call_site
- recompute-guard.dl:31+41 named call sites → call_site
- rtkq-op-recovery.dl:28 hook calls → call_site
- callgraph.dl:19, std/callgraph.dl:31 co-occurrence heuristic → call_edge
  (files predate the resolved graph; this is an upgrade, note it in the header)
- openapi.dl:16 registerHook("op") → sg(:ts, "registerHook($OP)")
- route-norm.dl:42+46+50 route/fetch/axios literals → sg(:ts/:py patterns)
- op-table.dl:22 parser dispatch arm → sg(:rust, if-eq-return pattern)
- cli-doc.dl:16 `#[arg(long` → ast(:rust, "(attribute_item) @cap") or sg
- anim-self.dl:68 `export function NAME` (.mjs) → sg(:js, function pattern)

### Bucket D: comment markers → comment()
- autodoc-plans.dl:20 `<!-- @tag msg -->` → comment()
- anim-self.dl:70 `//` comment text → comment()
- recompute-guard.dl:44 `@recompute unguarded` waiver → comment()
- gen-reference.dl:77+115 `# @op` marker + summary line → comment()

## GRAY (15) — engine gaps, leave sites as-is

| Gap | Sites | What's missing |
|---|---|---|
| JSONL dispatch | banned-word-guard.dl:16+20+24, latest-turn-guardrail.dl:41+49 | json/jsonp treats .jsonl as single-doc Fmt::Json; needs line-delimited dispatch |
| Token-text rel | rails.dl:25, .dl/rails.dl:15 | banned-word rail wants per-token text across idents+comments+strings; node/child carry spans, no text column |
| rev on type_entity/call_def | time.dl:11+15 | WORK-vs-HEAD fn-set diff needs a rev column (type_edge_rev precedent) |
| Field/param identifier names | cli-doc.dl:22 | type_sig tracks referenced types, not the field's own name |
| Line-enumeration rel | lint-docs.dl:84 | doc-block complement wants a line spine; node/child are byte-span |
| .dl op-call-site rel | gen-reference.dl:63 | dl_diag is diagnostics-only; no rel exposes op call heads in .dl source |
| impl-mention span | .dl/triage.dl:22+25 | type_entity lacks `impl TargetType` mention spans (also :25 is a dead duplicate of :22 — flagged, separate issue) |
| Go import rel | phantom-deps.dl:42 | no module_import equivalent for Go; line-shape guard has no structural analogue |

## LEGIT (8) — keep match

go.mod pins (version-skew.dl:21, pin-skew.dl:23+28, phantom-deps.dl:27+32 —
bespoke manifest format, the sanctioned seam per the pin-skew arc), markdown
tables (cli-doc.dl:39, op-table.dl:34 — doc_node covers headings+fences only),
ad hoc generated txt (latest-turn-guardrail.dl:75).

## Task list

- [x] S1 sweep Bucket A (mechanical scan-forcer drops; also fixes the prose
      comments that taught the idiom) — Haiku partition, 18 files
- [x] S2 sweep Buckets B/C/D (structured-rel swaps) — landed by the first
      (killed) sweep agent in its final minutes + two Sonnet partitions that
      audited every site against this plan and fixed 2 deviations
      (callgraph def rules moved to call_def; recompute-guard's def-subtraction
      deleted as structurally unnecessary under call_site)
- [x] S3 full suite green post-sweep: it 458/0/4, lib 202/0/1 (2026-07-03).
      Commit-review leftovers: README autogen-zone diff (generator-written,
      keep) + stray examples/typeports.d2 (agent verify-run artifact, drop or
      keep at commit)
- [ ] (separate arcs, not this sweep) GRAY gaps in priority order: JSONL
      dispatch, rev on type_entity/call_def, token-text rel; then revisit the
      13 blocked sites
