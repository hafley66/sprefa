# Structural parsing: widen coverage across markup, template, and data formats

## Context

sprefa v5 has three independent grammar registries that serve different ops:

- **`AST_LANG_TABLE`** (`src/engine/lang_tables.rs`) — 12 tree-sitter grammars. Powers `ast` op, `comment_node`, CST `node`/`child`.
- **`SG_LANG_TABLE`** (`src/sg.rs`) — 23 ast-grep grammars. Powers `sg`/`ast_yaml`.
- **oxc** (via `src/graph/typegraph.rs`) — TS/TSX/JS/JSX. Powers type/call/dataflow, `comment_node`, `template_parts`, `unresolved`.

Coverage gaps surfaced during the `std/arch.dl` dogfooding session and the user's sweep of format families:

| Format | exts | `comment_node` | `ast`/CST | `sg` | type/call | `jsonp`/`json` | `doc_node` |
|---|---|:---:|:---:|:---:|:---:|:---:|:---:|
| HTML | .html .htm | no | no | yes | no | no | no |
| YAML | .yaml .yml | no | no | yes | no | yes | no |
| TOML | .toml | no | no | no | no | yes | no |
| CSS | .css | no | no | yes | no | no | no |
| Markdown | .md | yes | no | no | no | no | headings + code only |
| .mts/.cts | TS ES modules | **no** (gap) | no | no | **no** (gap) | no | no |
| Handlebars | .hbs .handlebars | no | no | no | no | no | no |
| EJS | .ejs | no | no | no | no | no | no |
| Vue SFC | .vue | no | no | no | no | no | no |
| Svelte | .svelte | no | no | no | no | no | no |
| Astro | .astro | no | no | no | no | no | no |
| Jinja | .j2 .jinja | no | no | no | no | no | no |
| gotmpl | .tmpl .gotmpl .gohtml | **yes** | **yes** | no | no | no | no |
| jsonnet | .jsonnet .libsonnet | **yes** | **yes** | no | no | no | no |
| JS/JSX template interp | `${expr}` backticks | — | — | — | — | — | `template_parts` ✅ |

Source references: `src/engine/lang_tables.rs:4` (AST_LANG_TABLE), `src/sg.rs:12` (SG_LANG_TABLE), `src/cst.rs:208` (`lang_label_for_path`), `src/engine/extract/text.rs:18` (comment file set), `src/engine/extract/text.rs:103` (template file set), `src/graph/typegraph.rs:1774` (TypeLang matches), `src/ingest/mod.rs:63` (`MarkdownDoc::extract_docs`).

## Decisions

### 1. Markdown `doc_node` widening (highest value, lowest cost)

Extend `MarkdownDoc::extract_docs` in `src/ingest/mod.rs` to emit `doc_node` rows for tree-sitter-md block types already parsed but currently skipped in the `_ =>` descend arm:

- **`list` / `list_item`** — kind `"list_item"`, name = marker (`-`, `*`, `1.`), text = item content
- **`block_quote`** — kind `"blockquote"`, text = first paragraph quote text
- **`pipe_table`** — kind `"table_row"`, one row per `table_row`/`table_header`, name = first cell text
- **`thematic_break`** — kind `"thematic_break"`
- **`html_block`** — kind `"html_block"`, text = raw HTML

Project `DocNode.text` into the `doc_node` relation (add a 7th column `text`). Currently `text` exists on `DocNode` but is internal-only (used by `doc_ref` for code block scanning). Exposing it gives consumers access to list item content, blockquote text, table row text, etc.

For inline nodes (links, images), descend into `paragraph`/`heading` inline children and emit:
- **`inline_link`** `[text](url)` — kind `"link"`, name = url, text = link text
- **`image`** `![alt](url)` — kind `"image"`, name = url, text = alt text

Rejected: a separate `doc_inline` rel — over-normalized for the first pass; the `doc_node` shape `(kind, name, parent, text)` carries link data adequately (name=url, text=link text). A companion `doc_link(file, line, text, url)` can be added later if the structural join needs both fields typed.

### 2. Wire compiled-but-unwired grammars (free wins)

These tree-sitter crates are already in `Cargo.toml` and compiled into the binary but NOT in `AST_LANG_TABLE` / `lang_label_for_path`:

- **`tree-sitter-html`** — add to `AST_LANG_TABLE` as `("html", &["htm"], ctor)`. Add `"html" | "htm" => "html"` to `lang_label_for_path` (src/cst.rs:216). Unlocks `comment_node` + `ast` + CST for HTML.
- **`tree-sitter-yaml`** — add `("yaml", &["yml"], ctor)`. Add `"yaml" | "yml" => "yaml"`. Unlocks `comment_node` + `ast` for YAML. (Already in `datapath` for `jsonp`/`json` and in `sg` for pattern matching.)
- **`tree-sitter-toml-ng`** — add `("toml", &[], ctor)`. Add `"toml" => "toml"`. Unlocks `comment_node` + `ast` for TOML. (Already in `datapath`.)
- **`tree-sitter-json`** — add `("json", &[], ctor)`. Add `"json" => "json"`. Unlocks `comment_node` + `ast` for JSON. (Already in `datapath` and `sg`. JSON has no comments, but CST `node`/`child` and `ast` queries become available.)
- **`tree-sitter-css`** — NOT in Cargo.toml. Add the dependency + wire. Unlocks `comment_node` + `ast` + CST for CSS.

Rejected: adding these to `SG_LANG_TABLE` too — they're already there (html, yaml, json, css all in the ast-grep set). The gap is only in the tree-sitter `ast`/`comment_node`/CST path.

### 3. Fix `.mts`/`.cts` extraction gap

The module graph resolver (`src/graph/modgraph.rs:276`) recognizes `.mts`/`.cts` but the oxc extraction paths skip them. Add `.mts`/`.cts` to the extension checks in:

- `src/engine/extract/text.rs:18` — `comment_file_set` (`.ts`/`.tsx` check)
- `src/engine/extract/text.rs:103` — `template_file_set` (`.ts`/`.tsx`/`.js`/`.jsx`/`.mjs`/`.cjs`)
- `src/engine/extract/text.rs:167` — `unresolved_file_set` (same)
- `src/graph/typegraph.rs:1774` — `TsTypes::matches`

Rejected: a centralized `is_ts_file(path)` helper — cleaner but the four call sites are simple `ends_with` chains and the existing code style doesn't use helpers for these.

### 4. Add template-framework grammars (new deps)

None of these have tree-sitter grammars in the build. Adding each is the `sprf-add-language` checklist: add the crate to Cargo.toml, wire into `AST_LANG_TABLE` + `lang_label_for_path`, run the language matrix test.

Ordered by estimated value:

| Grammar | crate | exts | value |
|---|---|---|---|
| Vue SFC | `tree-sitter-vue` | .vue | high — SFC blocks (template/script/style) are structural |
| Svelte | `tree-sitter-svelte` | .svelte | high — component structure |
| Astro | `tree-sitter-astro` (community) | .astro | medium — island architecture |
| Handlebars | `tree-sitter-handlebars` (community) or `tree-sitter-glimmer` | .hbs .handlebars | medium — template structure |
| EJS | no tree-sitter grammar exists | .ejs | low — EJS is JS with `<% %>` delimiters; the JS portion is already parseable by oxc if the delimiters are stripped |
| Jinja | `tree-sitter-jinja` (community) | .j2 .jinja | medium — Python template structure |

<!-- todo(feature): confirm tree-sitter-vue/svelte/astro crates exist and compile before committing to the dep -->

### 5. Embedded-language interpolation (already partially covered)

JSX/TS template interpolation via backtick strings is already covered by `template_parts(file, line, node, idx, kind, text)`. The `sg` term form (`sg(:css, str, "pat")`) parses embedded CSS bodies (styled-components, emotion).

No new work here — documented in the `sg` op doc (`src/engine/decls.rs:172`).

## Sequencing

1. **Markdown `doc_node` widening** — extend `MarkdownDoc::extract_docs`, add `text` column to `doc_node`, update `doc_node` tests in `tests/it/doc_node.rs` and `tests/it/doc_ref.rs`.
2. **Wire compiled-but-unwired grammars** — add html/yaml/toml/json to `AST_LANG_TABLE` + `lang_label_for_path`. Add `tree-sitter-css` dep. Update language matrix test.
3. **Fix `.mts`/`.cts`** — four one-liner extension additions.
4. **Template grammars** — per-grammar, following `sprf-add-language`. Vue/Svelte first.
5. **Regen** — `dl examples/gen-plans-index.dl`, README splice, skill matrix update.

<!-- todo(decision): whether to add a `doc_inline` rel for link/image URL+text separation, or keep overloaded in `doc_node` -->

## Verification

- `cargo test --test it -- doc_node doc_ref` — existing markdown tests must still pass with the `text` column added.
- New tests for each `doc_node` kind: list_item, blockquote, table_row, link, thematic_break.
- `cargo test --test it -- comment_node` — verify HTML/YAML/TOML comments are extracted after wiring.
- `cargo test --test it -- lang_matrix` — language matrix test must include the new grammars.
- `cargo test --test it -- ast_grammars` — verify `ast` op works over HTML/YAML/TOML/JSON.
- New test for `.mts`/`.cts` extraction parity with `.ts`.
- `cargo test --test it` full suite passes.

<!-- todo(triage): CSS comment_node coverage depends on tree-sitter-css being published for tree-sitter 0.25; verify availability -->

## Staffing

- Base SHA: `2b10fbd6`
- Agent: general-purpose subagent per arc item, or direct edits for the small fixes
- Worktree: items 2-3 can share a worktree (grammar wiring); item 1 is a focused edit; item 4 is per-grammar
- Suite budget: full `cargo test --test it` (~70s on this machine)
