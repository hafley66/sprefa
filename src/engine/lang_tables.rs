use super::*;

type TsLangCtor = fn() -> tree_sitter::Language;
static AST_LANG_TABLE: &[(&str, &[&str], TsLangCtor)] = &[
    ("rust", &["rs"], || {
        tree_sitter::Language::new(tree_sitter_rust::LANGUAGE)
    }),
    ("c", &[], || {
        tree_sitter::Language::new(tree_sitter_c::LANGUAGE)
    }),
    ("kotlin", &["kt"], || {
        tree_sitter::Language::new(tree_sitter_kotlin_sg::LANGUAGE)
    }),
    ("python", &["py"], || {
        tree_sitter::Language::new(tree_sitter_python::LANGUAGE)
    }),
    ("bash", &["sh", "shell"], || {
        tree_sitter::Language::new(tree_sitter_bash::LANGUAGE)
    }),
    ("go", &["golang"], || {
        tree_sitter::Language::new(tree_sitter_go::LANGUAGE)
    }),
    ("hcl", &["terraform", "tf"], || {
        tree_sitter::Language::new(tree_sitter_hcl::LANGUAGE)
    }),
    ("starlark", &["bzl", "bazel"], || {
        tree_sitter::Language::new(tree_sitter_starlark::LANGUAGE)
    }),
    ("jsonnet", &[], || {
        tree_sitter::Language::new(tree_sitter_jsonnet::LANGUAGE)
    }),
    ("dl", &["dl"], || tree_sitter_dl::language().into()),
    ("gotmpl", &["gotemplate", "gohtml"], || {
        tree_sitter::Language::new(unsafe {
            tree_sitter_language::LanguageFn::from_raw(tree_sitter_gotmpl)
        })
    }),
    ("dockerfile", &["docker"], || {
        tree_sitter::Language::new(unsafe {
            tree_sitter_language::LanguageFn::from_raw(tree_sitter_dockerfile)
        })
    }),
    ("yaml", &["yml"], || { tree_sitter_yaml::LANGUAGE.into() }),
    ("toml", &[], || { tree_sitter_toml_ng::LANGUAGE.into() }),
    ("json", &[], || { tree_sitter_json::LANGUAGE.into() }),
    ("css", &[], || { tree_sitter::Language::new(tree_sitter_css::LANGUAGE) }),
    // todo(feature): add html once tree-sitter-html is a Cargo dep.
];

pub(crate) fn ts_lang(lang: &str) -> Result<tree_sitter::Language> {
    ts_lang_resolved(lang).map(|(_, language)| language)
}

/// Resolve a lang label (canonical or alias) to `(canonical name, grammar)`.
/// The canonical name is the per-file `AstTreeCache` key, so alias spellings
/// (`:rs` / `:rust`) share one parsed tree.
pub(crate) fn ts_lang_resolved(lang: &str) -> Result<(&'static str, tree_sitter::Language)> {
    for (canon, aliases, ctor) in AST_LANG_TABLE {
        if lang == *canon || aliases.contains(&lang) {
            return Ok((canon, ctor()));
        }
    }
    let compiled = AST_LANG_TABLE
        .iter()
        .map(|(c, ..)| *c)
        .collect::<Vec<_>>()
        .join(", ");
    bail!("no ast grammar for :{lang} (compiled in: {compiled})")
}

/// Canonical language names the `ast` op accepts (one per tree-sitter grammar).
/// The skill's per-op language matrix is checked set-equal against this in
/// `tests/it/lang_matrix.rs`, so a stale matrix fails CI.
pub fn ast_langs() -> Vec<&'static str> {
    AST_LANG_TABLE.iter().map(|(canon, ..)| *canon).collect()
}
