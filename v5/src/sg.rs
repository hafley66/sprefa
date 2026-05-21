use anyhow::{bail, Result};
use ast_grep_core::{AstGrep, Pattern};
use ast_grep_language::SupportLang;

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

/// Match an ast-grep pattern (target-language syntax with `$META` vars) over
/// file content. Returns (line, captures) per match; captures bind each
/// metavar name to the matched node's text.
pub fn run_sg(content: &str, lang: &str, pattern_str: &str) -> Result<Vec<(i64, Vec<(String, String)>)>> {
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
                caps.push((n.clone(), node.text().to_string()));
            }
        }
        let line = m.start_pos().line() as i64 + 1;
        out.push((line, caps));
    }
    Ok(out)
}
