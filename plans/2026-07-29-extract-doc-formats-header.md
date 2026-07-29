# sprefa-extract doc formats header (user directive 2026-07-29): raw html/xml/md/json/yaml/toml as extraction possibilities

User word: "we need raw html/xml/md/json/yaml/toml as part of extraction
possibilities, so i would like that planned up for adding to sprefa-extract."

## Current state (receipts)
- extract CLI: "python/c/... (any ast-grep grammar) -> cst only"; full
  families (cst/type/call/df) exist for ts/rust/go/kotlin/prolog only.
- The comment-node lab (plans/2026-07-29-comment-node-verdict.md) named
  markdown as the ONE extractor hole in comment parity (v5 walk_md_comments;
  no md grammar in the cst family). This arc closes SLOT-EXTRACTOR-WAIVER's
  whole scope.
- v5 precedent: json op (term-extract), comment op walks md; html/xml/yaml/
  toml had no dedicated ops.

## Step 0 — buy research (standing law, before any code)
Which of the six ship in our ast-grep dependency's grammar registry vs need
a direct tree-sitter grammar dep: tree-sitter-html, tree-sitter-xml,
tree-sitter-markdown (NOTE: block/inline SPLIT grammars), tree-sitter-json,
tree-sitter-yaml, tree-sitter-toml. Version pins, maintenance state, parse
failure behavior on dirty real-world files (html especially). Written
candidate table first.

## Family plan per format
- ALL six: `cst` family (nodes + spans), which alone unlocks ts_query/
  sg_pattern hosts over these files.
- json/yaml/toml: a `doc` family — one row per leaf:
  (key_path, value_text, value_kind, span) with key_path in one canonical
  spelling (json-pointer-style; slot below). Programs then join config keys
  as ordinary rels with no destructure gymnastics; yaml anchors/aliases
  resolved at emit with a named refusal for cycles; toml tables flattened
  into the same key_path spelling.
- html/xml: `doc` rows as (element_path, attr_name, attr_value, text, span);
  entities/DTD out of scope, named refusal.
- md: `cst` + comment rows (closes the markdown comment hole) + `doc` rows
  for headings/sections/fences/links (heading path = the key_path analogue).

## Laws riding
- Extractor stays policy-free: marker conventions, key significance, and
  suppression semantics stay in-language (std/suppress.dl law).
- Spans stay half-open bytes; line/col derivation stays at the seam.
- Every new format lands with: fixture corpus files, snapshot tests, AND a
  CLI-level golden test (the bin-vs-lib parity lesson from the resolve arc).
- One record shape per family across formats — a yaml doc row and a toml
  doc row differ only in path spelling, never in columns.

## Named slots
- SLOT-KEYPATH-SPELLING: json-pointer (/a/0/b) vs dotted (a.0.b) vs the
  jq-style ($.a[0].b) — one spelling for json/yaml/toml/md-heading paths.
- SLOT-MD-GRAMMAR: block-only vs block+inline tree-sitter-markdown; inline
  doubles cost and most rails need block level only.
- SLOT-HTML-DIRT: parse-error policy on real-world html (tree-sitter-html
  is lenient; decide whether ERROR nodes emit rows or a named finding).
- SLOT-DOC-VALUE-TYPES: value_kind vocabulary (string/int/float/bool/null
  in the SOURCE document vs our column model that has no bool/null — the
  doc family REPORTS the source kind as text; the language's own bool/null
  stance is untouched).

## Shape of the arc
Research lane (step 0, sonnet) -> family implementation lane (rust,
fixtures + goldens, codex-shaped) -> one dogfood program per format over
our own tree (justfile yaml? package.json? INDEX.md headings) graded like
the comment receipts. Not dispatched; awaiting user word on sequencing
vs phase 5 / simplify wave.
