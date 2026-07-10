---
name: project_doc_comment_spine
description: doc_comment/doc_tag rels — AST-located doc-comment extraction (Tier 1/2 doc gen) across Rust/Kotlin/TS
metadata: 
  node_type: memory
  type: project
  originSessionId: 56424a34-a5a4-4ee1-b2de-df146b4ee420
---

Doc-comment extraction landed (main, local/uncommitted as of 2026-06-29). Two
builtin rels ride the `TypeFacts` extractor (one parse, populated in
`refresh_type_rels` in engine.rs):

- `doc_comment(repo, sym, line, text)` — Tier 1: cleaned doc block per
  `type_entity` sym (joins on sym).
- `doc_tag(repo, sym, tag, arg, text)` — Tier 2: structured split. tag =
  param/returns/deprecated/throws/section; arg = the `@param` name.

Per-language AST locators in typegraph.rs (NOT the `comment` op — that's
region-based and can't see Python docstrings or distinguish `///` from `//`):
- **Rust** = syn `#[doc]` attrs (handles a `#[derive]` between doc and decl,
  which a line-scan-above approach would miss).
- **Kotlin** = tree-sitter `prev_sibling` KDoc (`*comment*` opening `/**`).
- **TS** = oxc byte-association (oxc keeps comments out of the AST): each
  `/** */` → nearest entity anchor at/after its end with a whitespace-only gap.
  Top-level decls + class methods + var-fn arrows; decorated classes skipped.

Tags: shared `parse_jsdoc_tags` (`@tag [{type}] [name] desc`, type dropped) for
JSDoc/KDoc; `parse_rust_sections` (`# Heading` → section) for rustdoc (rustdoc
has no @-tags).

Wiring mirrors [[project_v5_dl_engine]] type rels: `DOC_TEXT_RELS` const,
`doc_text_rel_decls`, `doc_text_rels_used` (OR'd into both `refresh_type_rels`
triggers so a doc-only program works), reserved-name guard, `builtin_rel_docs`
catalog rows (rel_catalog completeness test enforces a summary). Tests:
`tests/it/doc_comment.rs`. Example: `examples/doc-coverage.dl` (undocumented-API
rail via `!doc_comment`).

Scope limit: only the three TypeLang langs. Python (docstrings = first body
string literal, not a comment) and Go (godoc adjacency) have NO TypeLang
extractor yet — SCIP-tier only — so they get no doc extraction until someone
adds their `TypeLang::extract`. The cross-language doc-convention survey
(attachment axis: leading-comment / body-string / attribute; structure axis:
@-tags / reST-fields / prose) is in the chat_log. Deliberately NOT a CI gate
(Chris: "no don't fail ci for docs thats lame"). This is the on-ramp to the
TypeSpec-like type-language-as-DSL idea Chris floated.
