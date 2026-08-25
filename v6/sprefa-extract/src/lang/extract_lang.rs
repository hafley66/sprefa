//! `ExtractLang`: the one ast-grep `Language` the extractor speaks. `StrDoc<L>`
//! needs `L: LanguageExt` (core tree_sitter/mod.rs:46), so one enum keeps one
//! `SgRoot` alias: `Sg` delegates to `SupportLang` (ast-grep-language
//! lib.rs:431-458), the rest carry grammars this crate already links.
//! @comment-ok: module header, the shape every lang/*.rs opens with

use std::borrow::Cow;
use std::path::Path;
use std::str::FromStr;

use ast_grep_core::matcher::{Pattern, PatternBuilder, PatternError};
use ast_grep_core::tree_sitter::{LanguageExt, StrDoc, TSLanguage};
use ast_grep_core::Language;
use ast_grep_language::SupportLang;
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

/// `MarkdownInline` is never routed from a path (a `.md` routes to the block
/// grammar); a caller names it directly to reach the inline plane.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ExtractLang {
    Sg(SupportLang),
    Dl6,
    Prolog,
    Markdown,
    MarkdownInline,
}

impl ExtractLang {
    /// The arms mirror the `Source` roster's own tests (dl6/_0_source.rs:395,
    /// prolog/_0_source.rs:867-873, markdown/_0_source.rs:112).
    pub fn from_path(path: &str) -> Option<Self> {
        if path.ends_with(".dl6") {
            return Some(Self::Dl6);
        }
        if path.ends_with(".pl")
            || path.ends_with(".plt")
            || path.ends_with(".pro")
            || path.ends_with(".prolog")
            || path.ends_with(".datalog")
            || path.ends_with(".horn")
        {
            return Some(Self::Prolog);
        }
        if path.ends_with(".md") || path.ends_with(".markdown") {
            return Some(Self::Markdown);
        }
        SupportLang::from_path(path).map(Self::Sg)
    }

    /// The ast-grep YAML `language:` field's spelling; inverse of `parse_name`.
    pub fn name(&self) -> Cow<'static, str> {
        match self {
            Self::Sg(sg) => Cow::Owned(sg.to_string()),
            Self::Dl6 => Cow::Borrowed("dl6"),
            Self::Prolog => Cow::Borrowed("prolog"),
            Self::Markdown => Cow::Borrowed("markdown"),
            Self::MarkdownInline => Cow::Borrowed("markdown_inline"),
        }
    }

    /// `SupportLang::from_str` is case-insensitive over its alias table
    /// (ast-grep-language lib.rs:378-389), so either spelling of `Sg` resolves.
    pub fn parse_name(name: &str) -> Option<Self> {
        match name {
            "dl6" => Some(Self::Dl6),
            "prolog" => Some(Self::Prolog),
            "markdown" | "md" => Some(Self::Markdown),
            "markdown_inline" | "md_inline" => Some(Self::MarkdownInline),
            _ => SupportLang::from_str(name).ok().map(Self::Sg),
        }
    }
}

/// Verbatim copy of the private `pre_process_pattern` (ast-grep-language
/// lib.rs:88-97); the crate exports the macro that calls it, never the fn.
fn rewrite_dollar(expando: char, query: &str) -> Cow<'_, str> {
    let mut out = Vec::with_capacity(query.len());
    let mut dollar_count = 0;
    for char in query.chars() {
        if char == '$' {
            dollar_count += 1;
            continue;
        }
        let need_replace = matches!(char, 'A'..='Z' | '_') || dollar_count == 3;
        let sigil = if need_replace { expando } else { '$' };
        out.extend(std::iter::repeat_n(sigil, dollar_count));
        dollar_count = 0;
        out.push(char);
    }
    let sigil = if dollar_count == 3 { expando } else { '$' };
    out.extend(std::iter::repeat_n(sigil, dollar_count));
    Cow::Owned(out.into_iter().collect())
}

impl Language for ExtractLang {
    fn meta_var_char(&self) -> char {
        match self {
            Self::Sg(sg) => sg.meta_var_char(),
            _ => '$',
        }
    }

    /// dl6 and prolog have no lexer rule whose charset holds `µ` (dl6
    /// grammar.js:129-130: `variable` = `[A-Z]...`, `identifier` =
    /// `_*[a-z]...`), so `µT` parses to `(ERROR (UNEXPECTED 181))` under both,
    /// while `_T` is a plain `variable` in each. That is the C/C++/CSS sigil
    /// (ast-grep-language lib.rs:186-190), not the `µ` of lib.rs:196-211.
    /// Markdown keeps `µ` because `_` is emphasis syntax there.
    /// @comment-ok: the sigil per grammar is the one fact the code cannot show
    fn expando_char(&self) -> char {
        match self {
            Self::Sg(sg) => sg.expando_char(),
            Self::Dl6 | Self::Prolog => '_',
            Self::Markdown | Self::MarkdownInline => 'µ',
        }
    }

    fn pre_process_pattern<'q>(&self, query: &'q str) -> Cow<'q, str> {
        match self {
            Self::Sg(sg) => sg.pre_process_pattern(query),
            _ => rewrite_dollar(self.expando_char(), query),
        }
    }

    fn from_path<P: AsRef<Path>>(path: P) -> Option<Self> {
        ExtractLang::from_path(path.as_ref().to_str()?)
    }

    fn kind_to_id(&self, kind: &str) -> u16 {
        match self {
            Self::Sg(sg) => sg.kind_to_id(kind),
            _ => self.get_ts_language().id_for_node_kind(kind, true),
        }
    }

    fn field_to_id(&self, field: &str) -> Option<u16> {
        match self {
            Self::Sg(sg) => sg.field_to_id(field),
            _ => self
                .get_ts_language()
                .field_id_for_name(field)
                .map(|id| id.get()),
        }
    }

    /// The pattern doc carries `ExtractLang`, never the inner `SupportLang`:
    /// pattern and candidate have to share one `Doc` type.
    fn build_pattern(&self, builder: &PatternBuilder) -> Result<Pattern, PatternError> {
        builder.build(|src| StrDoc::try_new(src, *self))
    }
}

impl LanguageExt for ExtractLang {
    /// The same `LANGUAGE` constants the raw extractors parse with
    /// (dl6/_0_source.rs:26, prolog/_0_source.rs:25, markdown/_0_source.rs:86).
    fn get_ts_language(&self) -> TSLanguage {
        match self {
            Self::Sg(sg) => sg.get_ts_language(),
            Self::Dl6 => tree_sitter::Language::new(tree_sitter_dl6::LANGUAGE),
            Self::Prolog => tree_sitter::Language::new(tree_sitter_prolog::LANGUAGE),
            Self::Markdown => tree_sitter::Language::new(tree_sitter_md::LANGUAGE),
            Self::MarkdownInline => tree_sitter::Language::new(tree_sitter_md::INLINE_LANGUAGE),
        }
    }
}

impl std::fmt::Display for ExtractLang {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.name())
    }
}

impl Serialize for ExtractLang {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.name())
    }
}

impl<'de> Deserialize<'de> for ExtractLang {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let name = String::deserialize(deserializer)?;
        ExtractLang::parse_name(&name)
            .ok_or_else(|| de::Error::invalid_value(de::Unexpected::Str(&name), &"a known grammar"))
    }
}
