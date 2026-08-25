# Extract as an ast-grep extension over soopy

Auditor copy. Every claim carries `path:line`. Design decided 2026-08-25, not
re-litigated. No Rust written in this lane; signatures live here only.

<!-- todo(decision): arc C FactMatcher reads ~/.agent/dl6.db read-only; the one-server-one-db law means the db must already be the live store, not a copy -->

## Context

The v6 extraction leaf (`v6/sprefa-extract`) owns three grammars it parses with
raw tree-sitter today: dl6 (`src/lang/dl6/_0_source.rs:24-29`), prolog
(`src/lang/prolog/_0_source.rs:23-28`), and markdown block+inline
(`src/lang/markdown/_0_source.rs:20-24, 86`). Every other grammar routes through
ast-grep (`SupportLang`), so `extract --ast-pattern` and YAML ast-grep rules
work on rust/ts/tsx/js/go but refuse `.dl6`/`.pl`/`.md` with
`ParseError::NoGrammar` (`lang/astgrep.rs:64`, `SupportLang::from_path` returns
None for those extensions, ast-grep-language `lib.rs:441-443`).

The move verb (`src/0_move.rs`) hand-builds specifier edits from the prolog
family (`0_move.rs:288-331`), stages them as `Act::Replace`
(`0_move.rs:71-84`) with a bare `(start, end, String)` triple, and carries them
through soopy's staged-mutation boundary (`0_move.rs:550-589`). This plan makes
dl6/prolog/md first-class ast-grep languages, drains ast-grep edits into soopy
`TextEdit`s, and re-expresses the move as one YAML rule plus a fact matcher
against the live `~/.agent/dl6.db`.

Three arcs, ordered:

- A. `impl Language + LanguageExt` for `Dl6`, `Prolog`, `Markdown` (block and
  inline). After it, `--ast-pattern` and YAML rules run on those files with zero
  further code.
- B. `From<Edit<String>> for soopy::TextEdit` + a `Doc` impl whose `do_edit`
  appends to a pending `SourceAction::Replace` carrying `expected: ContentId`
  instead of mutating a string, so the ast-grep replace drain lands in a
  `StageRequest`. Delete the `Vec<(usize, usize, String)>` triple in
  `Act::Replace` (`0_move.rs:72`).
- C. `FactMatcher { rel, column, value }` reading rows from `~/.agent/dl6.db`
  composed with ast-grep `ops::All/Any/Not`; `extract move` re-expressed as one
  YAML rule (specifier rewrite) + a fact matcher, byte-identical on
  `tests/1_move.rs` fixtures and on the real tree.

The layers and their owners (decided, do not re-open):

| layer | owner | seam |
|---|---|---|
| grammar | tree-sitter; extract owns dl6/prolog/markdown, ast-grep-language the rest | `ast_grep_core::Language` + `LanguageExt` |
| match, pattern syntax, YAML rules | ast-grep | `Matcher`, `ast_grep_config::RuleCore` |
| facts, which files, which nodes | extract + dl6 over the one SQLite db | `FactMatcher` implementing `Matcher` |
| edit generation | ast-grep `Replacer` | `ast_grep_core::source::Edit` -> `soopy::TextEdit` |
| staging, expected-hash guard, atomic commit, replay | soopy | `StageRequest`, `SourceAction` |

## Arc A: the three languages as `Language + LanguageExt`

### Decision: wrap `SupportLang` in an extract-side enum

Chosen: an extract-side enum `ExtractLang` implementing `Language` and
`LanguageExt` by per-variant delegation, with the three grammars as fresh
variants. Rejected: three standalone `Language` impls (three parallel
`SgRoot` aliases, three copies of `query_patterns`/`query_ast_rule`/the
`CstProjector` type parameter).

Why the wrapper fits the generic: `AstGrep<D> = Root<D>` (ast-grep-core
`lib.rs:36`), `Root<D: Doc>` (`node.rs:50`), and the concrete document is
`StrDoc<L: LanguageExt>` (tree_sitter `mod.rs:46`). The whole extract ast-grep
surface is written against `StrDoc<SupportLang>`: `SgRoot` (astgrep.rs:27), the
`CstProjector` walk (astgrep.rs:177), and `query_patterns`/`query_ast_rule`
(astgrep.rs:58, `1_ast_rule.rs:327`). Swapping the inner `L` from `SupportLang`
to `ExtractLang` is one type substitution across those sites; the root type
stays a single alias. `SupportLang` itself is already a delegating enum
(ast-grep-language `lib.rs:431-458`), so `ExtractLang` mirrors that exact
shape, one variant deeper.

`ExtractLang` needs `from_path` that returns `None` only when no known grammar
matches, and an `expando_char`/`pre_process_pattern` that is correct per
variant.

### Type signatures (Arc A)

```rust
// v6/sprefa-extract/src/lang/extract_lang.rs (new module, re-exported by lang/mod.rs)
/// `AstGrep<StrDoc<L>>` is `Root<D: Doc>` (core lib.rs:36, node.rs:50); `StrDoc<L>`
/// needs `L: LanguageExt` (tree_sitter/mod.rs:46). One enum keeps one `SgRoot` alias.
#[derive(Clone)]
pub enum ExtractLang {
    Sg(SupportLang),
    Dl6,
    Prolog,
    Markdown,
    MarkdownInline,
}

impl ExtractLang {
    /// Match a path to a grammar. The five ast-grep-languages delegate to
    /// SupportLang::from_path; the rest ride the extractor's own extension arms
    /// (dl6/_0_source.rs:395, prolog/_0_source.rs:867-873, markdown/_0_source.rs:112).
    pub fn from_path(path: &str) -> Option<Self>;
}

impl ast_grep_core::Language for ExtractLang {
    // Language: Clone + 'static (language.rs:11). All five required methods.
    fn meta_var_char(&self) -> char { '$' }
    fn expando_char(&self) -> char;
    fn pre_process_pattern<'q>(&self, query: &'q str) -> Cow<'q, str>;
    fn extract_meta_var(&self, source: &str) -> Option<MetaVariable>; // default ok
    fn kind_to_id(&self, kind: &str) -> u16;
    fn field_to_id(&self, field: &str) -> Option<u16>;
    fn build_pattern(&self, builder: &PatternBuilder) -> Result<Pattern, PatternError>;
    fn from_path<P: AsRef<Path>>(_path: P) -> Option<Self>; // via ExtractLang::from_path
}

impl ast_grep_core::tree_sitter::LanguageExt for ExtractLang {
    fn get_ts_language(&self) -> TSLanguage;
    // injectable_languages / extract_injections keep the trait defaults
    // (tree_sitter/mod.rs:276-291).
}
```

Body (pseudo-code, per `impl_lang_expando!`, ast-grep-language `lib.rs:102-133`):

```rust
// expando_char: the tree-sitter grammars for the three reject '$' as an
// identifier lead (dl6 variable = [A-Z][A-Za-z0-9_]* grammar.js:129; prolog
// unquoted_atom = [a-z].../variable = [A-Z_]... grammar.js:204,207). Use 'z'
// following Html (html.rs:13-15). Sg(_) delegates to SupportLang (lib.rs:434-435).
match self {
    ExtractLang::Sg(sg) => sg.expando_char(),
    _ => 'z',
}

// pre_process_pattern: '$' + [A-Z_]/$$$ -> expando. Copy of the private fn
// ast-grep-language lib.rs:79-98; the crate does not export it.
fn pre_process_pattern(&self, q: &'q str) -> Cow<'q, str> {
    let sigil = self.expando_char();
    rewrite_dollar(q, sigil) // $X -> zX, $$$ -> zzz
}

// kind_to_id / field_to_id / build_pattern: delegate get_ts_language like the
// Tsx test impl (language.rs:58-73) and SupportLang (lib.rs:431-437).
fn kind_to_id(&self, kind: &str) -> u16 {
    self.get_ts_language().id_for_node_kind(kind, true)
}
fn build_pattern(&self, builder: &PatternBuilder) -> Result<Pattern, PatternError> {
    builder.build(|src| StrDoc::try_new(src, self.clone()))
}
```

The three concrete `LanguageExt::get_ts_language` arms:

```rust
Dl6           => tree_sitter::Language::new(tree_sitter_dl6::LANGUAGE).into(),
Prolog        => tree_sitter::Language::new(tree_sitter_prolog::LANGUAGE).into(),
Markdown      => tree_sitter::Language::new(tree_sitter_md::LANGUAGE).into(),
MarkdownInline=> tree_sitter::Language::new(tree_sitter_md::INLINE_LANGUAGE).into(),
// Each Language derives into TSLanguage via `From<Language>` (tree_sitter
// runtime); the same LANGUAGE constants the raw extractors already parse
// (dl6/_0_source.rs:26, prolog/_0_source.rs:25, markdown/_0_source.rs:86,120).
```

### Why `$` cannot be the dl6 metavar, with citation

`parse_dl_dcg.pl:1688-1690` defines `dollar_var` -> `[0'$], ident(Name),
{ hole_var(Name, Var) }`: in dl6, `$Name` is a hole/meta-variable in the
language itself. ast-grep's default metavar char is also `$` (language.rs:21-22).
If the Dl6 impl kept `$` as `expando_char`, an ast-grep pattern `$X` and a dl6
source hole `$X` would be textually identical and indistinguishable. Overriding
`expando_char` to `'z'` (so `$X` becomes `zX` at pattern-build time, and `zX`
is the expando metavar recognized at match time) separates the two namespaces.

### Instance lifetimes (Arc A)

- `ExtractLang` values are `Copy`-cheap (the `Sg(SupportLang)` variant is
  `Copy`-derived; the three unit variants are `Copy`). They live for the
  lifetime of one parse call and are moved into the `StrDoc` via
  `StrDoc::try_new` (tree_sitter/mod.rs:53-57).
- `SgRoot = AstGrep<StrDoc<ExtractLang>>` owns the source `String` + the
  tree-sitter `Tree` (astgrep.rs:23-27, tree_sitter/mod.rs:46-50); one per
  file, `Send`, dropped after projection/query.
- No long-lived state in Arc A; the language value is a zero-or-`Copy` token.

### Storage layout (Arc A)

None new. The three grammars already exist as tree-sitter `LANGUAGE` constants
in the dependency set (`Cargo.toml:75-94`). Read sequence: path -> `ExtractLang`
-> grammar -> parse. Uniqueness: one grammar per path, first match in the
`Source` roster order (lang/mod.rs:60-71).

### Files touched (Arc A)

- `v6/sprefa-extract/src/lang/extract_lang.rs` (new: `ExtractLang`,
  `Language`+`LanguageExt` impls, the `rewrite_dollar` copy).
- `v6/sprefa-extract/src/lang/mod.rs` (declare the module; add `ExtractLang` to
  the re-exports).
- `v6/sprefa-extract/src/lang/astgrep.rs` (swap `SupportLang` ->
  `ExtractLang` in `SgRoot:27`, `query_patterns:64`, `CstProjector:177`,
  `AstGrepParser` impl `:136-161`).
- `v6/sprefa-extract/src/lang/1_ast_rule.rs` (swap `SupportLang` ->
  `ExtractLang` at `:314, 327, 365, 420, 432`; `ConfigWire.language` becomes
  `ExtractLang`).

### Files forbidden (Arc A)

- `src/lang/prolog/_0_source.rs`, `src/lang/dl6/_0_source.rs`,
  `src/lang/markdown/_0_source.rs` projection logic (the `Source`/`Resolve`
  family code stays raw tree-sitter; only the new `Language` impls touch the
  grammars).
- `src/0_move.rs` (Arc B owns it).
- The vendored ast-grep crates (never edit `~/.cargo/registry/src/...`).

### Tests to add (Arc A)

- `dl6_ast_pattern_matches_rule_head` (tests/3_ast_pattern_cli.rs style, or a
  new `31_extract_lang.rs`): `extract --ast-pattern` over a `.dl6` fixture
  returns the expected capture fact. Breaks if `expando_char`/grammar wiring is
  wrong. Receipt: `query_patterns` returns a capture whose span matches the
  fixture oracle byte-for-byte.
- `prolog_ast_pattern_matches_body_goal`: same over a `.pl` fixture.
- `markdown_ast_pattern_matches_heading`: same over a `.md` fixture (block +
  inline).
- `extract_lang_from_path_routes_grammar`: asserts `ExtractLang::from_path`
  routes `.dl6`/`.pl`/`.md` and delegates the five `SupportLang` extensions.

### Gate command (Arc A)

```sh
cargo test --features cli            # per extract AGENTS.md gate
```

### Risk table (Arc A)

| risk | citation that raises it | mitigation |
|---|---|---|
| `$` collision between ast-grep metavar and dl6 hole | parse_dl_dcg.pl:1688-1690 vs language.rs:21-22 | `expando_char='z'`, citation in code |
| tree-sitter-dl6 grammar may not parse a `.dl6` file that contains `$X` holes (the grammar has no `$` rule; only the prolog compiler does) | tree-sitter-dl6/grammar.js:129 (variable = `[A-Z]...`, no `$`) vs parse_dl_dcg.pl:1688 | gate a real `.dl6` fixture through `--ast-pattern`; if holes do not parse, flag for human, do not paper over |
| markdown is text-heavy; expando `z` inside inline text can collide with real lowercase-`z`+uppercase runs | markdown/_0_source.rs:83-103, html.rs:13-15 | test real `.md` headings; accept if matches are correct on fixtures |
| prolog `variable` is `[A-Z_]...`, so `zX` (expando) parses as an atom, not a variable | tree-sitter-prolog/grammar.js:204,207 | verify pattern still matches; atom vs variable is fine for a single-node metavar |
| `pre_process_pattern` is private in ast-grep-language | ast-grep-language lib.rs:79-98 | vendor a small copy into extract_lang.rs, pinned by a unit test |

## Arc B: `From<Edit<String>>` + the pending-action `Doc`

### Build-vs-buy: ast-grep-config `fix:` / `transform:`

Does the fix pipeline already produce `Edit`s we can drain, or only strings?
Answer: it produces `Edit`s. `Fixer` implements `Replacer<D>`
(ast-grep-config `fixer.rs:172-179`), and `NodeMatch::make_edit`
(node_match.rs:55-67) produces `Edit<D>` whose `inserted_text` is the
transformed replacement bytes. The transform values (the `transform:` map,
`rule_core.rs:53-55`) resolve into the fix template at
`generate_replacement` (fixer.rs:177-179 -> replacer/template.rs:41). This is
already exercised in-tree: `1_ast_rule.rs:401-412` calls
`matched.make_edit(matcher, fixer)` and reads `edit.inserted_text`.

Buy verdict: no bespoke fix engine. Arc B's `From<Edit<String>>` is the only
new adapter; it maps an existing `Edit` to a soopy `TextEdit` and the fix
pipeline is untouched. Citations: fixer.rs:172-179, node_match.rs:55-67,
1_ast_rule.rs:401-412.

### Decision: delete `Act`, store `Vec<soopy::SourceAction>` in `Plan`

Chosen: `Plan.stages` becomes `Vec<Vec<soopy::SourceAction>>`; `Act`
(`0_move.rs:71-94`) and the per-stage `action()` builder (`0_move.rs:591-640`)
are deleted. `stage_and_commit` (`0_move.rs:550-589`) then only wraps the
already-built actions in a `StageRequest` (`0_move.rs:562-566`). Rejected:
keeping `Act` with a `Replace` arm holding `SourceAction` (two names for the
same thing, and `stage_and_commit` still walks `Act` to rebuild).

Rationale: soopy accepts ONE operation per source file (`_7d_mutation_plan.rs`
`insert_non_replace`, cited `0_move.rs:69-70`), so the stage grouping that `Act`
existed to express survives as `Vec<Vec<SourceAction>>`. Building each
`SourceAction` once in `Plan::build` (which already reads bytes for the
expected-hash, `0_move.rs:605-609`) removes the per-stage rebuild.

### Type signatures (Arc B)

```rust
// v6/sprefa-extract/src/lang/astgrep.rs (or a new src/drain.rs)

/// ast-grep `Edit<String>` is `Edit<u8>`: Underlying = u8 (source.rs:20-24,
/// 151-160). Its position/deleted_length/inserted_text map 1:1 onto a soopy
/// byte TextEdit (soopy _7b_source_actions.rs:136-140).
impl From<ast_grep_core::source::Edit<String>> for soopy::TextEdit {
    fn from(edit: ast_grep_core::source::Edit<String>) -> Self;
}

/// A Doc whose do_edit never mutates a string: it appends a soopy TextEdit to a
/// pending SourceAction::Replace (soopy _7b_source_actions.rs:187-191) carrying
/// the file's ContentId as the optimistic `expected` precondition. The matcher
/// walk stays on the immutable parsed tree, so multiple edits collect without
/// re-parse.
#[derive(Clone)]
pub struct PendingReplaceDoc<L: LanguageExt> {
    src: String,          // frozen source, never mutated by do_edit
    lang: L,
    tree: tree_sitter::Tree,
    source: soopy::ActionSource,
    expected: soopy::ContentId,
    edits: Vec<soopy::TextEdit>,
    producer: soopy::ActionProducer,
}

impl<L: LanguageExt> Doc for PendingReplaceDoc<L> {
    // Doc: Clone + 'static; Source: Content; Node<'r>: SgNode<'r>
    // (source.rs:127-136, 28).
    type Source = String;
    type Lang = L;
    type Node<'r> = ast_grep_core::tree_sitter::Node<'r>;
    fn get_lang(&self) -> &L;
    fn get_source(&self) -> &String;
    fn do_edit(&mut self, edit: &Edit<String>) -> Result<(), String>;
    fn root_node(&self) -> Self::Node<'_>;
    fn get_node_text<'a>(&'a self, node: &Self::Node<'a>) -> Cow<'a, str>;
}
```

Body (pseudo-code):

```rust
impl From<Edit<String>> for soopy::TextEdit {
    fn from(edit: Edit<String>) -> Self {
        // TextEdit.range is ActionSpan { source, start, end } (_7b:67-71).
        soopy::TextEdit {
            range: soopy::ActionSpan {
                source: <filled by caller from the pending source>,
                start: edit.position as u64,
                end: (edit.position + edit.deleted_length) as u64,
            },
            replacement: edit.inserted_text,          // Vec<u8> (source.rs:23, 152)
            producer: <caller producer>,              // ActionProducer (_7b:95-100)
        }
    }
}

impl<L: LanguageExt> Doc for PendingReplaceDoc<L> {
    fn do_edit(&mut self, edit: &Edit<String>) -> Result<(), String> {
        // append a TextEdit, do NOT touch self.src (contrast StrDoc::do_edit,
        // tree_sitter/mod.rs:79-84 which mutates + reparses).
        self.edits.push(edit.into());   // expected/source filled from self
        Ok(())
    }
    fn root_node(&self) -> Self::Node<'_> { self.tree.root_node() }
    fn get_node_text<'a>(&'a self, node: &Self::Node<'a>) -> Cow<'a, str> {
        Cow::Borrowed(node.utf8_text(self.src.as_bytes()).unwrap_or(""))
    }
}
```

The drain (the generic analogue of the StrDoc-only `replace_all`,
tree_sitter/mod.rs:439-450):

```rust
/// Collect every edit for one file: same Visitor inner pattern StrDoc uses
/// (tree_sitter/mod.rs:445-449) but generic over D: Doc (node.rs:323-336 +
/// node_match.rs:55-67). Caller passes the pending doc so do_edit is never
/// driven; the edits are returned and the caller folds them into the
/// SourceAction::Replace.
pub fn drain_edits<D, M, R>(
    root: &ast_grep_core::Node<D>,
    matcher: &M,
    replacer: &R,
) -> Vec<ast_grep_core::source::Edit<D::Source>>
where
    D: Doc, M: Matcher, R: Replacer<D>,
{ /* root.find_all(matcher).map(|m| m.make_edit(matcher, replacer)).collect() */ }

/// Fold edits into one Replace action and stage.
pub fn stage_edits(
    source: soopy::ActionSource,
    expected: soopy::ContentId,
    edits: Vec<soopy::TextEdit>,
    root_id: soopy::SourceRootId,
) -> soopy::StageRequest; // SourceAction::Replace { source, expected, edits } (_7b:187-191)
```

The ContentId `expected` comes from `content_id_of` / `ContentId::blake3`
(types.rs:54-56).

### Instance lifetimes (Arc B)

- `PendingReplaceDoc` is created per file, owns the frozen `src: String`, the
  `tree`, the `ActionSource`, the `ContentId`, and the growing `edits` vec. It
  is dropped after the drain; the `StageRequest` owns the completed
  `SourceAction::Replace`.
- The `StageRequest` lives per run (one per file's replace, or grouped per
  stage in the move path). It is consumed by `stage_mutations`
  (stage_store.rs:164-203) and sealed into a `StagedSourceTransaction`.
- The tree lifetime: `Node<'r>` borrows the `PendingReplaceDoc`; `drain_edits`
  runs on the thread owning the doc (same constraint as `SgRoot`,
  astgrep.rs:24-26).

### Storage layout (Arc B)

- Input: file bytes on disk (read once, `0_move.rs:146-148`).
- Read sequence: bytes -> `String` -> `AstGrep`/`PendingReplaceDoc` -> matcher
  walk -> `Vec<Edit>` -> `Vec<soopy::TextEdit>` -> `SourceAction::Replace`.
- Write sequence: `StageRequest` -> `stage_mutations` (stage_store.rs:164-203)
  -> `StagedSourceTransaction` -> `CommitEngine::commit` (commit.rs:254-259).
- Uniqueness: one `Replace` action per file (the soopy one-op-per-file rule,
  `_7d_mutation_plan.rs` + `0_move.rs:69-70`); edits sorted by start, deduped
  (mirror `0_move.rs:213-218`).

### Files touched (Arc B)

- `v6/sprefa-extract/src/lang/1_ast_rule.rs`: reuse `stage_request_batch` /
  `stage_request` (`:106-175`) or extend them; the `From<Edit<String>>` adapter
  feeds `AstRuleMutationProposal.replacement` (`:96-101`) unchanged.
- `v6/sprefa-extract/src/0_move.rs`: delete `Act` (`:71-94`), change
  `Plan.stages` to `Vec<Vec<soopy::SourceAction>>` (`:96-101`), delete
  `action()` (`:591-640`), keep `stage_and_commit`'s StageRequest wrap
  (`:562-566`).
- New `src/drain.rs` (or in astgrep.rs): `From<Edit<String>>`,
  `drain_edits`, `stage_edits`.

### Files forbidden (Arc B)

- The soopy crate (`../../../hafley-rs/crates/soopy`, path dep
  `Cargo.toml:117`): `SourceAction`/`TextEdit`/`StageRequest` shapes are already
  sufficient; no soopy change.
- `src/lang/1_ast_rule.rs` semantics (fix/transform behavior unchanged).

### Tests to add (Arc B)

- `edit_to_text_edit_maps_spans_and_bytes`: `From<Edit<String>>` preserves
  position/deleted_length/inserted_text. Breaks if the byte offsets shift.
  Receipt: a `NodeMatch::replace_by` (`node_match.rs:42-52`) over a known
  source yields an `Edit` whose conversion matches the expected
  `ActionSpan.start/end` and `replacement`.
- `pending_doc_do_edit_appends_without_mutating`: drive `Root::replace`
  (`node.rs:76-89` -> `edit` `:71-74`) with `PendingReplaceDoc`; assert
  `do_edit` appended and `self.src` is unchanged. Breaks if do_edit reparses.
- `stage_edits_builds_replace_action_with_expected_content_id`:
  `stage_edits` returns a `StageRequest` whose `SourceAction::Replace` carries
  `expected = content_id_of(bytes)` (`types.rs:54-56`) and the folded edits.
- `1_move.rs` stays green after the `Act` deletion (byte-identical previews).

### Gate command (Arc B)

```sh
cargo test --features cli 1_move
```

### Risk table (Arc B)

| risk | citation that raises it | mitigation |
|---|---|---|
| `replace_all` is StrDoc-only, not generic; a custom Doc has no `replace_all` | tree_sitter/mod.rs:439-450 (impl on `Node<StrDoc<L>>`) | `drain_edits` replicates the inner Visitor pattern generically (node.rs:323-336, node_match.rs:55-67) |
| `do_edit` must return `Result<(), String>`; appending never errors | source.rs:133 | return `Ok(())`; errors surface at stage time |
| byte spans: ast-grep ranges are byte offsets, soopy ranges are byte offsets | source.rs:20-24, _7b:61-65 | direct `as u64` cast, no re-encode |
| one-op-per-file: a file with both Replace and Move must split stages | `_7d_mutation_plan.rs`, `0_move.rs:69-70` | keep `Vec<Vec<SourceAction>>` stage grouping |
| `expected` ContentId must match on-disk bytes or staging rejects as stale | `_7d_mutation_plan.rs:80-81` | compute from the same bytes read for parsing |

## Arc C: `FactMatcher` + the move as one YAML rule

### Decision: `FactMatcher` is a `Matcher` reading the live dl6.db read-only

Chosen: `FactMatcher { rel, column, value }` implements `ast_grep_core::Matcher`
(matcher.rs:27-48). Its `match_node_with_env` (matcher.rs:31-35) returns the
node when the node's text equals `value` and `value` is present in relation
`rel` column `column` of `~/.agent/dl6.db`. Composition uses the ast-grep
`ops` combinators directly: `ops::All` (ops.rs:45), `ops::Any` (ops.rs:107),
`ops::Not` (ops.rs:197). The db handle is one read-only connection per run, per
the one-server-one-db law (the db is the live store, not a copy).

The store layout (`~/.agent/dl6.db`) is the sprefa relational store: a `__str`
dictionary (`__id` surrogate, `content` unique) and one surrogate-keyed table
per relation (columns are `__id` references into `__str`). `FactMatcher` issues
`SELECT 1 FROM <rel> WHERE <column> = (SELECT __id FROM __str WHERE content = ?)`
per node text, or a single preload of the (rel, column) value set into memory
for a whole run.

### Type signatures (Arc C)

```rust
// v6/sprefa-extract/src/lang/fact.rs (new)

/// One node matches when its text appears in relation `rel`, column `column`,
/// equal to `value`. Implements Matcher (matcher.rs:27-48); composes with
/// ops::All/Any/Not (ops.rs:45,107,197).
#[derive(Clone)]
pub struct FactMatcher {
    rel: String,
    column: String,
    value: String,
}

impl FactMatcher {
    /// Preload the value set for (rel, column) once per run, then match by
    /// set membership. Lifetime: one db read per (rel, column) per run.
    pub fn load(conn: &rusqlite::Connection, rel: &str, column: &str) -> Result<Vec<String>>;
    pub fn new(rel: String, column: String, value: String) -> Self;
}

impl ast_grep_core::Matcher for FactMatcher {
    fn match_node_with_env<'tree, D: Doc>(
        &self,
        node: ast_grep_core::Node<'tree, D>,
        _env: &mut Cow<MetaVarEnv<'tree, D>>,
    ) -> Option<ast_grep_core::Node<'tree, D>>;
    fn potential_kinds(&self) -> Option<BitSet> { None }
}
```

Body (pseudo-code):

```rust
impl Matcher for FactMatcher {
    fn match_node_with_env(...) -> Option<Node<'tree, D>> {
        // node.text() equals self.value and self.value is in the loaded set?
        if node.text() == self.value && self.present { Some(node) } else { None }
    }
}
```

The `extract move` re-expression (specifier rewrite as one YAML rule + a fact
matcher), designed against the existing hand-built specifier edits
(`0_move.rs:288-331`):

```rust
// one YAML rule that rewrites a specifier atom whose text equals the moved
// file's old name to the new name; the fact matcher gates it to the exact
// (rel, column) set that names the moved file.
let rule_yaml = format!(
    "id: move-spec\nrule:\n  pattern: $SPEC\nfix: '{}'\n", new_name
);
// FactMatcher { rel: "file_edge" /* or the specifier relation */,
//               column: "name", value: old_name }
```

### Instance lifetimes (Arc C)

- The db connection lives one per run (`rusqlite::Connection`), read-only,
  `Send`; shared across files within a run (one server one db).
- `FactMatcher` values are `Clone` and cheap; the preloaded value set is a
  `Vec<String>` per (rel, column), built once and borrowed by each match.
- The `SgRoot`/`PendingReplaceDoc` per file as in Arcs A/B.

### Storage layout (Arc C)

- Input: `~/.agent/dl6.db` (dict-encoded relations) + the corpus files.
- Read sequence: db -> value set per (rel, column); corpus -> parse -> node ->
  membership check -> edit.
- Write sequence: same soopy stage path as Arc B.
- Uniqueness: a node matches at most one (rel, column, value) tuple; edits
  deduped by (start, end) per file (mirror `0_move.rs:215-218`).

### Files touched (Arc C)

- New `v6/sprefa-extract/src/lang/fact.rs`: `FactMatcher` + `load`.
- `v6/sprefa-extract/Cargo.toml`: add `rusqlite` (or the existing dl6 store
  crate if it exposes a reader) for the read-only db access.
- `v6/sprefa-extract/src/0_move.rs`: replace the specifier hand-build
  (`:288-331`) with the YAML rule + fact matcher drain from Arc B.

### Files forbidden (Arc C)

- `scripts/rehome-passes.sh` (exists on held branch
  `refactor/prolog-rehome`, PR #467; do not touch or run).
- `src/lang/prolog/_0_source.rs` family logic (the specifier semantics stay
  there).

### Tests to add (Arc C)

- `fact_matcher_matches_relation_column_value`: `FactMatcher` matches a node
  whose text is in the loaded set and rejects one that is not. Breaks if the
  dict join is wrong. Receipt: `match_node_with_env` returns the node for a
  present value.
- `move_as_fact_matcher_byte_identical`: the re-expressed move against the
  `tests/1_move.rs` fixtures (`1_move.rs:120-182`) produces byte-identical
  output and previews. Breaks if the YAML rule/fact matcher misses an importer.
  Receipt: `cargo test --features cli 1_move` and a diff of the applied tree
  against the committed fixture strings.
- `move_matches_real_tree`: run against the real corpus tree (NOT
  rehome-passes.sh) and assert the edit set equals the current `extract move`
  output.

### Gate command (Arc C)

```sh
cargo test --features cli 1_move
# plus a diff of the re-expressed move output vs the current move output on a
# captured corpus snapshot
```

### Risk table (Arc C)

| risk | citation that raises it | mitigation |
|---|---|---|
| the specifier relation's shape in dl6.db differs from the hand-built `Specifier` rows | `0_move.rs:302-327` (reads `call.aux.specifiers`), types.rs:518-529 | confirm the (rel, column, value) names against the live db before writing the rule; gate on byte-identical 1_move fixtures |
| `potential_kinds` returns None, so matching is O(all nodes) | matcher.rs:39-41, 46-47 | acceptable at move corpus scale; add a kind filter if measured slow |
| reading a copy of the db would violate one-server-one-db | task law; store layout | open `~/.agent/dl6.db` read-only, never copy |

## Sequencing

Arc A -> Arc B -> Arc C. Arc A unblocks `--ast-pattern`/YAML rules on the three
grammars and is independently testable. Arc B is the edit drain both Arc A's
fix path and Arc C's move path feed. Arc C is the only arc with a human-held
gate (the rehome branch). Each arc ends with its gate command green and its
fixtures byte-identical.

## Verification

- Arc A: `cargo test --features cli`; `--ast-pattern` on `.dl6`/`.pl`/`.md`
  fixtures.
- Arc B: `cargo test --features cli 1_move`; the `From<Edit<String>>` and
  `PendingReplaceDoc` unit tests.
- Arc C: `cargo test --features cli 1_move`; byte-identical move on fixtures and
  a captured corpus snapshot; the rehome script untouched.

## Staffing

- Implements in `v6/sprefa-extract` on a worktree off this branch. Base SHA:
  `4f0b6ab9f24d6e4703f34ea075769c4746ed5f6e` (merged ff-only into
  `plan/extract-astgrep-soopy`).
- No soopy changes (path dep at `Cargo.toml:117`).
- Gate per arc as above; CI = build + `cargo test --features cli`. No Rust
  written in this plan lane; signatures live in this doc.
