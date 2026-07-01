use anyhow::{anyhow, bail, Result};
use ast_grep_config::{DeserializeEnv, SerializableRuleCore};
use ast_grep_core::{AstGrep, Pattern};
use ast_grep_language::{LanguageExt, SupportLang};

fn sg_lang(lang: &str) -> Result<SupportLang> {
    Ok(match lang {
        "rust" | "rs" => SupportLang::Rust,
        "ts" | "typescript" => SupportLang::TypeScript,
        "tsx" => SupportLang::Tsx,
        "js" | "javascript" => SupportLang::JavaScript,
        "py" | "python" => SupportLang::Python,
        "go" => SupportLang::Go,
        "json" => SupportLang::Json,
        "c" => SupportLang::C,
        "cpp" | "cc" | "cxx" => SupportLang::Cpp,
        "kotlin" | "kt" => SupportLang::Kotlin,
        other => bail!("no ast-grep grammar for :{other}"),
    })
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
