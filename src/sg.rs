use anyhow::{anyhow, bail, Result};
use ast_grep_config::{DeserializeEnv, SerializableRuleCore};
use ast_grep_core::{AstGrep, Pattern};
use ast_grep_language::{LanguageExt, SupportLang};

// LANG-JUNCTION(sg-grammars): one table row = `sg` + `ast_yaml` op support for a grammar (canonical name, aliases, ast-grep SupportLang); the skill language matrix test asserts this table
/// The ast-grep grammar table: `(canonical name, [extra aliases], SupportLang)`.
/// Single source of truth so `sg_lang` (the resolver) and `sg_langs` (the
/// canonical list the skill language matrix must match) can never drift. Adding
/// a grammar here without updating the skill matrix fails the matrix-honesty
/// test. Shared by the `sg` AND `ast_yaml` ops (both call `sg_lang`).
const SG_LANG_TABLE: &[(&str, &[&str], SupportLang)] = &[
    ("rust",       &["rs"],                SupportLang::Rust),
    ("typescript", &["ts"],                SupportLang::TypeScript),
    ("tsx",        &[],                    SupportLang::Tsx),
    ("javascript", &["js", "jsx"],         SupportLang::JavaScript),
    ("python",     &["py"],                SupportLang::Python),
    ("go",         &["golang"],            SupportLang::Go),
    ("json",       &[],                    SupportLang::Json),
    ("c",          &[],                    SupportLang::C),
    ("cpp",        &["cc", "cxx", "c++"],  SupportLang::Cpp),
    ("kotlin",     &["kt"],                SupportLang::Kotlin),
    // The rest of ast-grep-language's SupportLang set (every grammar the crate
    // ships) — the embedded-language seam wants css/html for styled-components and
    // markdown fences, and the polyglot corpus wants the compiled-language grammars.
    ("css",        &[],                    SupportLang::Css),
    ("html",       &[],                    SupportLang::Html),
    ("bash",       &["sh"],                SupportLang::Bash),
    ("csharp",     &["cs"],                SupportLang::CSharp),
    ("java",       &[],                    SupportLang::Java),
    ("scala",      &[],                    SupportLang::Scala),
    ("swift",      &[],                    SupportLang::Swift),
    ("ruby",       &["rb"],                SupportLang::Ruby),
    ("php",        &[],                    SupportLang::Php),
    ("lua",        &[],                    SupportLang::Lua),
    ("elixir",     &["ex"],                SupportLang::Elixir),
    ("haskell",    &["hs"],                SupportLang::Haskell),
    ("yaml",       &["yml"],               SupportLang::Yaml),
];

fn sg_lang(lang: &str) -> Result<SupportLang> {
    for (canon, aliases, sl) in SG_LANG_TABLE {
        if lang == *canon || aliases.contains(&lang) { return Ok(*sl); }
    }
    bail!("no ast-grep grammar for :{lang}")
}

/// Canonical language names the `sg` / `ast_yaml` ops accept (one per grammar).
/// The skill's per-op language matrix is checked set-equal against this in
/// `tests/it/lang_matrix.rs`, so a stale matrix fails CI.
pub fn sg_langs() -> Vec<&'static str> {
    SG_LANG_TABLE.iter().map(|(canon, ..)| *canon).collect()
}

fn metavar_names(pattern: &str) -> Vec<String> {
    let re = regex::Regex::new(r"\$+([A-Z_][A-Z0-9_]*)").unwrap();
    let mut seen = Vec::new();
    for c in re.captures_iter(pattern) {
        let n = c[1].to_string();
        if !seen.contains(&n) { seen.push(n); }
    }
    seen
}

/// One ast-grep match: 1-based start/end lines, 0-based byte columns
/// (tree-sitter's convention, == char/UTF-16 for ASCII), the whole-match node's
/// absolute byte range `[mlo, mhi)` in `content`, and the metavar captures. Each
/// capture is `(name, text, lo, hi)` where `[lo, hi)` is the metavar node's byte
/// range. `mlo`/`mhi` cover the ENTIRE match (literal text included), so a
/// `gen(:replace, …)` keyed off the match `id` rewrites the whole pattern — not
/// just the captures' bounding box.
pub type SgHit = (i64, i64, i64, i64, usize, usize, Vec<(String, String, usize, usize)>);

/// Match an ast-grep pattern (target-language syntax with `$META` vars) over
/// file content. One `SgHit` per match.
pub fn run_sg(content: &str, lang: &str, pattern_str: &str) -> Result<Vec<SgHit>> {
    let l = sg_lang(lang)?;
    let grep = AstGrep::new(content, l);
    let pattern = Pattern::new(pattern_str, l);
    let names = metavar_names(pattern_str);
    let mut out = Vec::new();
    for m in grep.root().find_all(&pattern) {
        let env = m.get_env();
        let mut caps = Vec::new();
        for n in &names {
            if let Some(node) = env.get_match(n) {
                let r = node.range();
                caps.push((n.clone(), node.text().to_string(), r.start, r.end));
            }
        }
        let mr = m.range();
        let (sl, sc) = m.start_pos().byte_point();
        let (el, ec) = m.end_pos().byte_point();
        out.push((sl as i64 + 1, sc as i64, el as i64 + 1, ec as i64, mr.start, mr.end, caps));
    }
    Ok(out)
}

/// Match an ast-grep `RuleCore` (the relational YAML rule language: `inside`,
/// `has`, `precedes`, `follows`, `any`, `all`, `not`, `kind`, `regex`,
/// `pattern`) over file content. `pattern`-only rules are a strict subset, so
/// this supersets `run_sg` — the difference is `inside:`/`has:` let a rule
/// express "a Db call nested inside a loop", which a bare pattern cannot.
///
/// Returns the same shape as `run_sg` so the engine arm is identical: one row
/// per match with `(line, col, end_line, end_col, captures)`, and each capture
/// is `(name, text, lo, hi)` where `[lo, hi)` is the metavar's byte range.
///
/// The YAML body is either a bare rule body (`pattern: "..."`, `inside: {...}`)
/// or wrapped under a top-level `rule:`. A bare body is the common case and
/// matches ast-grep's own RuleCore serde model directly; the `rule:`-wrapped
/// form is accepted for parity with v3/v4 `ast_yaml` programs.
pub fn run_ast_yaml(content: &str, lang: &str, yaml: &str) -> Result<Vec<SgHit>>
{
    let l = sg_lang(lang)?;
    let value: serde_yaml::Value = serde_yaml::from_str(yaml)
        .map_err(|e| anyhow!("ast_yaml YAML invalid: {e}"))?;
    // ast-grep 0.38's SerializableRuleCore requires a top-level `rule:` field.
    // The common case is a bare rule body (pattern/inside/has/any/...); wrap
    // it under `rule:`. An already-wrapped `{rule: {...}}` passes through.
    // (Full RuleConfig fields — constraints/utils — are accepted by serde if
    // present but the engine only runs the relational `rule` body.)
    let core_value = match &value {
        serde_yaml::Value::Mapping(m)
            if m.contains_key(serde_yaml::Value::String("rule".into())) => value,
        _ => {
            let mut wrapper = serde_yaml::Mapping::new();
            wrapper.insert(serde_yaml::Value::String("rule".into()), value);
            serde_yaml::Value::Mapping(wrapper)
        }
    };
    let core: SerializableRuleCore = serde_yaml::from_value(core_value)
        .map_err(|e| anyhow!("ast_yaml RuleCore deserialise failed: {e}"))?;
    let rule_core = core.get_matcher(DeserializeEnv::new(l))
        .map_err(|e| anyhow!("ast_yaml rule build failed: {e}"))?;

    let names = metavar_names(yaml);
    // lang.ast_grep builds AstGrep<StrDoc<L>> with the full parent-pointed tree
    // relational rules (inside/has/follows/precedes) need; AstGrep::new(content,
    // lang) yields AstGrep<L> which matches patterns fine but returns 0 for
    // inside:/has: (ancestor/descendant lookups come up empty).
    let grep = l.ast_grep(content);
    let mut out = Vec::new();
    for m in grep.root().find_all(&rule_core) {
        let env = m.get_env();
        let mut caps = Vec::new();
        for n in &names {
            if let Some(node) = env.get_match(n) {
                let r = node.range();
                caps.push((n.clone(), node.text().to_string(), r.start, r.end));
            }
        }
        let mr = m.range();
        let (sl, sc) = m.start_pos().byte_point();
        let (el, ec) = m.end_pos().byte_point();
        out.push((sl as i64 + 1, sc as i64, el as i64 + 1, ec as i64, mr.start, mr.end, caps));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every canonical name in `sg_langs()` resolves through `sg_lang` (the list
    /// and the resolver read one table, so this catches a typo'd canonical name).
    #[test]
    fn sg_langs_all_resolve() {
        for name in sg_langs() {
            assert!(sg_lang(name).is_ok(), "sg_langs lists `{name}` but sg_lang rejects it");
        }
        assert_eq!(sg_langs().len(), SG_LANG_TABLE.len());
    }

    /// Every newly-exposed grammar parses a trivial source and a bare metavar
    /// pattern matches over it — proof the SupportLang wiring is live, not just
    /// name-resolvable. One representative pattern per grammar.
    #[test]
    fn new_grammars_match_a_trivial_pattern() {
        let cases: &[(&str, &str, &str)] = &[
            // (lang, source, pattern) — pattern must produce >=1 hit. Patterns are
            // grammar-tuned: css declarations need the trailing `;` to parse as a
            // declaration node, php's `$x` is the whole variable node (no `$$$`).
            ("css",     ".card { position: fixed; }", "$PROP: $VAL;"),
            ("html",    "<div class=\"x\">hi</div>",  "<div class=\"x\">hi</div>"),
            ("bash",    "echo hello",                 "echo $ARG"),
            ("csharp",  "class C { }",                "class $NAME { }"),
            ("java",    "class C { }",                "class $NAME { }"),
            ("scala",   "object O { }",               "object $NAME { }"),
            ("swift",   "let x = 1",                   "let $NAME = $VAL"),
            ("ruby",    "def f; end",                 "def $NAME; end"),
            ("php",     "<?php $x = 1;",              "$VAR = $V"),
            ("lua",     "local x = 1",                 "local $NAME = $VAL"),
            ("elixir",  "x = 1",                       "$NAME = $VAL"),
            ("haskell", "main = putStrLn x",           "main = $BODY"),
            ("yaml",    "key: value",                  "$K: $V"),
        ];
        for (lang, src, pat) in cases {
            let hits = run_sg(src, lang, pat)
                .unwrap_or_else(|e| panic!("run_sg({lang}) errored: {e}"));
            assert!(!hits.is_empty(), "pattern `{pat}` matched nothing in {lang} source `{src}`");
        }
    }

    /// The css pattern the styled-components example relies on: over a bare
    /// declaration block (the body of a `styled.div` template — no selector),
    /// `$PROP: $VAL;` picks out each declaration and captures property + value.
    #[test]
    fn css_declaration_capture() {
        let css_body = "position: fixed;\ncolor: red;";
        let hits = run_sg(css_body, "css", "$PROP: $VAL;").unwrap();
        assert_eq!(hits.len(), 2, "expected two declarations");
        let position = hits.iter().find(|h| {
            h.6.iter().any(|(n, t, ..)| n == "PROP" && t == "position")
        }).expect("a position declaration");
        let val = position.6.iter().find(|(n, ..)| n == "VAL").map(|(_, t, ..)| t.as_str());
        assert_eq!(val, Some("fixed"));
    }
}
