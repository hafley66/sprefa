//! `ast[lang](pattern)` — AST pattern matching via ast-grep-core.
//!
//! Ported from v2/src/ops/_9_ast_grep.rs. Rayon prefetch + reader cache
//! deferred (sprefa-4m7.2.22 spike) — v0 does per-cursor synchronous
//! parse + find_all on the cursor's active bytes.
//!
//! Examples:
//!   ast[rust](fn $NAME($$$ARGS) { $$$BODY })
//!   ast[typescript](let $X: ${TY}Error = $V)
//!
//! Metavars:
//!   - `$VAR` / `$$$VAR` — native ast-grep metavars, pass through
//!   - `${VAR}` — sprefa sugar. The surrounding identifier-char token run
//!     collapses to a synthetic `$SPRFSLOTN` metavar, and a named regex
//!     extracts `VAR` from that slot's matched text. Lets you capture
//!     sub-token spans without breaking ast-grep's pattern grammar.
//!
//! Bracket-arg `[lang]` selects the ast-grep grammar: `rust` (`rs`) and
//! `typescript` (`ts`) are wired today.

use std::collections::HashMap;
use std::sync::Arc;

use ast_grep_core::{source::StrDoc, AstGrep, Language, Pattern};
use ast_grep_language::SupportLang;
use effect_runtime::{BoxFuture, RtCtx};
use regex::Regex;
use tree_sitter::Node;

use crate::_0_cursor::{Capture, Cursor};
use crate::_1_op::Op;
use crate::op_ctor::OpCtor;
use crate::pattern_op::PatternDiagnostic;
use crate::value::Value;

#[derive(Debug)]
pub struct AstGrepOp {
    pub lang:           SupportLang,
    pub pattern:        Arc<Pattern<SupportLang>>,
    pub slot_regexes:   Vec<(String, Regex)>,
    pub bound_caps:     Vec<Arc<str>>,
    pub _src:           Arc<str>,
}

impl OpCtor for AstGrepOp {
    const NAME: &'static str = "ast";
    const BODY_GRAMMAR: &'static str = "ast-grep pattern + ${VAR} sugar; lang via [rust|ts]";
    const DOC: &'static str = "\
**ast**[_lang_](_pattern_)

Match an ast-grep pattern against the cursor's active bytes. `lang` is
`rust` (`rs`) or `typescript` (`ts`). Native ast-grep metavars `$VAR`
and `$$$VAR` work unchanged. `${VAR}` is sprefa sugar: the surrounding
identifier-char token run collapses to a synthetic `$SPRFSLOTN` and a
named regex pulls `VAR` out of that slot's matched text — useful when
the capture sits mid-token (e.g. `${TY}Error` matches `MyError` and
binds `TY=My`).
";

    fn from_values(_values: Vec<Value>) -> Result<Self, Vec<PatternDiagnostic>> {
        // Ast must come through `from_paren_node` — body is a raw DSL
        // and language comes from the bracket slot. A defensive empty
        // op compiles but matches nothing.
        Err(vec![PatternDiagnostic {
            code: "ast/missing-lang",
            message: "ast requires a bracket-arg form: ast[rust](pattern) or ast[ts](pattern)".into(),
            byte_range: 0..0,
        }])
    }

    fn from_paren_node(
        inv_node: Node<'_>,
        src:      &[u8],
    ) -> Option<Result<Self, Vec<PatternDiagnostic>>> {
        Some(lower_ast_paren(inv_node, src))
    }
}

impl Op for AstGrepOp {
    fn name(&self) -> &'static str { "ast" }

    fn pipe<'a>(&'a self, ctx: &'a RtCtx, c: Cursor) -> BoxFuture<'a, Vec<Cursor>> {
        Box::pin(async move {
            let Some(c) = crate::effects::ensure_content_loaded(ctx, c).await else {
                return Vec::new();
            };
            let active = c.active();
            let base = c.byte_range.start;

            let Ok(src_str) = std::str::from_utf8(active) else { return Vec::new(); };

            let grep: AstGrep<StrDoc<SupportLang>> = self.lang.ast_grep(src_str);

            let mut out: Vec<Cursor> = Vec::new();
            for nm in grep.root().find_all(&*self.pattern) {
                let env = nm.get_env();
                let mut cap_values: HashMap<Arc<str>, (Arc<str>, Option<std::ops::Range<usize>>)> =
                    HashMap::new();

                // Native bound captures from the env.
                for name in self.bound_caps.iter() {
                    if let Some(node) = env.get_match(name.as_ref()) {
                        let r = node.range();
                        let abs = (base + r.start)..(base + r.end);
                        cap_values.insert(
                            name.clone(),
                            (Arc::from(node.text().as_ref()), Some(abs)),
                        );
                    } else {
                        let nodes = env.get_multiple_matches(name.as_ref());
                        if !nodes.is_empty() {
                            let joined: String = nodes.iter()
                                .map(|n| n.text().to_string())
                                .collect::<Vec<_>>()
                                .join(" ");
                            cap_values.insert(name.clone(), (Arc::from(joined.as_str()), None));
                        }
                    }
                }

                // Slot-regex sub-token extraction — pull each ${VAR}'s
                // capture from the corresponding $SPRFSLOTN node.
                let mut slot_ok = true;
                for (slot_name, re) in self.slot_regexes.iter() {
                    let Some(node) = env.get_match(slot_name) else {
                        slot_ok = false;
                        break;
                    };
                    let text = node.text().to_string();
                    let Some(caps) = re.captures(&text) else {
                        slot_ok = false;
                        break;
                    };
                    let node_r = node.range();
                    for cap_name in re.capture_names().flatten() {
                        if let Some(m) = caps.name(cap_name) {
                            let abs =
                                (base + node_r.start + m.start())..(base + node_r.start + m.end());
                            cap_values.insert(
                                Arc::from(cap_name),
                                (Arc::from(m.as_str()), Some(abs)),
                            );
                        }
                    }
                }
                if !slot_ok { continue; }

                let r = nm.range();
                let abs_match = (base + r.start)..(base + r.end);
                let mut next = c.clone();
                next.byte_range = abs_match;
                for (name, (text, span)) in cap_values {
                    let cap = match span {
                        Some(s) => Capture::span_backed(name, s),
                        None    => Capture::synthesized(name, text),
                    };
                    next.captures.push(cap);
                }
                out.push(next);
            }
            out
        })
    }

    fn bound_captures(&self) -> &[Arc<str>] { &self.bound_caps }
}

// ---------------------------------------------------------------------------
// Lowering
// ---------------------------------------------------------------------------

fn lower_ast_paren(
    inv_node: Node<'_>,
    src:      &[u8],
) -> Result<AstGrepOp, Vec<PatternDiagnostic>> {
    let bracket = inv_node.child_by_field_name("bracket").ok_or_else(|| {
        vec![PatternDiagnostic {
            code: "ast/missing-lang",
            message: "ast requires a bracket-arg, e.g. ast[rust](fn $N() {})".into(),
            byte_range: inv_node.byte_range(),
        }]
    })?;
    let br = bracket.byte_range();
    let lang_bytes = if br.end < br.start + 2 { &[][..] } else { &src[br.start + 1..br.end - 1] };
    let lang_name = std::str::from_utf8(lang_bytes)
        .map_err(|_| vec![PatternDiagnostic {
            code: "ast/missing-lang",
            message: "ast bracket arg is not valid UTF-8".into(),
            byte_range: bracket.byte_range(),
        }])?
        .trim();
    let lang = parse_lang(lang_name).ok_or_else(|| {
        vec![PatternDiagnostic {
            code: "ast/unknown-lang",
            message: format!(
                "ast language `{lang_name}` unknown; supported: rust, rs, typescript, ts"
            ),
            byte_range: bracket.byte_range(),
        }]
    })?;

    let paren = inv_node.child_by_field_name("paren").ok_or_else(|| {
        vec![PatternDiagnostic {
            code: "ast/missing-pattern",
            message: "ast requires a paren body, e.g. ast[rust](fn $N() {})".into(),
            byte_range: inv_node.byte_range(),
        }]
    })?;
    let pr = paren.byte_range();
    let raw_bytes = if pr.end < pr.start + 2 { &[][..] } else { &src[pr.start + 1..pr.end - 1] };
    let raw = std::str::from_utf8(raw_bytes)
        .map_err(|_| vec![PatternDiagnostic {
            code: "ast/bad-pattern",
            message: "ast pattern is not valid UTF-8".into(),
            byte_range: paren.byte_range(),
        }])?
        .trim();
    if raw.is_empty() {
        return Err(vec![PatternDiagnostic {
            code: "ast/bad-pattern",
            message: "ast pattern body is empty".into(),
            byte_range: paren.byte_range(),
        }]);
    }

    let (rewritten, slot_res, sugar_caps) = lower_sugar(raw);
    let native_caps = scan_metavars(raw);

    let pattern = Pattern::try_new(&rewritten, lang).map_err(|e| {
        vec![PatternDiagnostic {
            code: "ast/bad-pattern",
            message: format!("ast pattern `{raw}` invalid: {e}"),
            byte_range: paren.byte_range(),
        }]
    })?;

    let mut bound: Vec<Arc<str>> = Vec::new();
    for n in native_caps.into_iter().chain(sugar_caps.into_iter()) {
        if !bound.iter().any(|s| s.as_ref() == n.as_ref()) { bound.push(n); }
    }

    Ok(AstGrepOp {
        lang,
        pattern:      Arc::new(pattern),
        slot_regexes: slot_res,
        bound_caps:   bound,
        _src:         Arc::from(raw),
    })
}

fn parse_lang(s: &str) -> Option<SupportLang> {
    match s {
        "rust" | "rs"       => Some(SupportLang::Rust),
        "typescript" | "ts" => Some(SupportLang::TypeScript),
        _                   => None,
    }
}

/// Scan a pattern for native ast-grep metavars (`$NAME` / `$$$NAME`),
/// skipping `${...}` sugar tokens. First-seen order, no duplicates.
fn scan_metavars(pat: &str) -> Vec<Arc<str>> {
    let b = pat.as_bytes();
    let mut out: Vec<Arc<str>> = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'$' {
            if i + 1 < b.len() && b[i + 1] == b'{' {
                i += 2;
                while i < b.len() && b[i] != b'}' { i += 1; }
                if i < b.len() { i += 1; }
                continue;
            }
            let mut j = i + 1;
            if j + 1 < b.len() && b[j] == b'$' && b[j + 1] == b'$' { j += 2; }
            let start = j;
            while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_') { j += 1; }
            if j > start {
                let name = &pat[start..j];
                let a: Arc<str> = Arc::from(name);
                if !out.iter().any(|s| s.as_ref() == a.as_ref()) { out.push(a); }
            }
            i = j;
        } else {
            i += 1;
        }
    }
    out
}

/// Rewrite each `${VAR}` / `${$$$VAR}` plus its surrounding identifier
/// token run into `$SPRFSLOTN`, emitting a named regex that recovers
/// VAR from the slot's matched text.
fn lower_sugar(src: &str) -> (String, Vec<(String, Regex)>, Vec<Arc<str>>) {
    let bytes = src.as_bytes();
    let mut out = String::new();
    let mut regexes: Vec<(String, Regex)> = Vec::new();
    let mut captures: Vec<Arc<str>> = Vec::new();
    let mut slot_idx = 0usize;
    let mut i = 0usize;

    while i < bytes.len() {
        if bytes[i] == b'$' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
            let inner_start = i + 2;
            let mut j = inner_start;
            while j < bytes.len() && bytes[j] != b'}' { j += 1; }
            if j >= bytes.len() {
                out.push('$');
                i += 1;
                continue;
            }
            let inner = &src[inner_start..j];
            let inner_trimmed = inner.trim();
            let (multi, name) = if let Some(rest) = inner_trimmed.strip_prefix("$$$") {
                (true, rest.trim())
            } else {
                (false, inner_trimmed)
            };
            if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                out.push_str(&src[i..j + 1]);
                i = j + 1;
                continue;
            }

            // Pull adjacent identifier-char run from already-written output as prefix.
            let mut prefix_bytes = 0usize;
            {
                let ob = out.as_bytes();
                let mut k = ob.len();
                while k > 0 {
                    let c = ob[k - 1];
                    if c.is_ascii_alphanumeric() || c == b'_' {
                        k -= 1;
                        prefix_bytes += 1;
                    } else { break; }
                }
            }
            let prefix: String = out[out.len() - prefix_bytes..].to_string();
            out.truncate(out.len() - prefix_bytes);

            let after_brace = j + 1;
            let mut k = after_brace;
            while k < bytes.len() && (bytes[k].is_ascii_alphanumeric() || bytes[k] == b'_') {
                k += 1;
            }
            let suffix: String = src[after_brace..k].to_string();

            let slot_name = format!("SPRFSLOT{}", slot_idx);
            slot_idx += 1;
            out.push('$');
            out.push_str(&slot_name);

            let inner_re = if multi { ".+" } else { "\\S+" };
            let re_src = format!(
                "^{}(?P<{}>{}){}$",
                regex::escape(&prefix),
                name,
                inner_re,
                regex::escape(&suffix),
            );
            if let Ok(re) = Regex::new(&re_src) {
                regexes.push((slot_name, re));
            }
            let cap_arc: Arc<str> = Arc::from(name);
            if !captures.iter().any(|s| s.as_ref() == cap_arc.as_ref()) {
                captures.push(cap_arc);
            }
            i = k;
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }

    (out, regexes, captures)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn host_lang() -> tree_sitter::Language {
        tree_sitter_sprefa::LANGUAGE.into()
    }

    fn lower(src: &str) -> Result<AstGrepOp, Vec<PatternDiagnostic>> {
        let mut parser = Parser::new();
        parser.set_language(&host_lang()).unwrap();
        let tree = parser.parse(src, None).unwrap();
        let root = tree.root_node();
        let inv = find_op_invocation(root).expect("no op_invocation in source");
        lower_ast_paren(inv, src.as_bytes())
    }

    fn find_op_invocation(n: Node<'_>) -> Option<Node<'_>> {
        if n.kind() == "op_invocation" { return Some(n); }
        let mut walk = n.walk();
        for ch in n.named_children(&mut walk) {
            if let Some(v) = find_op_invocation(ch) { return Some(v); }
        }
        None
    }

    #[test]
    fn scan_metavars_basic() {
        let v = scan_metavars("fn $NAME($$$ARGS) { $$$BODY }");
        let names: Vec<&str> = v.iter().map(|s| s.as_ref()).collect();
        assert_eq!(names, vec!["NAME", "ARGS", "BODY"]);
    }

    #[test]
    fn scan_metavars_skips_brace_form() {
        let v = scan_metavars("let $X: ${TY}Error = $V");
        let names: Vec<&str> = v.iter().map(|s| s.as_ref()).collect();
        // ${TY} is sugar, not native; X and V are native.
        assert_eq!(names, vec!["X", "V"]);
    }

    #[test]
    fn lower_sugar_synthesizes_slot_and_regex() {
        let (out, res, caps) = lower_sugar("${TY}Error");
        assert_eq!(out, "$SPRFSLOT0");
        assert_eq!(res.len(), 1);
        let names: Vec<&str> = caps.iter().map(|s| s.as_ref()).collect();
        assert_eq!(names, vec!["TY"]);
    }

    #[test]
    fn lower_sugar_passthrough_when_isolated() {
        // Even isolated ${VAR} becomes $SPRFSLOT — that's how v2 works,
        // and the regex `^(?P<VAR>\S+)$` then identity-captures.
        let (out, res, _) = lower_sugar("fn ${NAME}() {}");
        assert_eq!(out, "fn $SPRFSLOT0() {}");
        assert_eq!(res.len(), 1);
    }

    #[test]
    fn lower_basic_rust_pattern() {
        let op = lower("ast[rust](fn $NAME() {})").unwrap();
        let names: Vec<&str> = op.bound_captures().iter().map(|s| s.as_ref()).collect();
        assert!(names.contains(&"NAME"));
    }

    #[test]
    fn lower_rejects_missing_lang() {
        let err = lower("ast(fn $X() {})").unwrap_err();
        assert_eq!(err[0].code, "ast/missing-lang");
    }

    #[test]
    fn lower_rejects_unknown_lang() {
        let err = lower("ast[cobol](fn $X() {})").unwrap_err();
        assert_eq!(err[0].code, "ast/unknown-lang");
    }

    #[test]
    fn lower_rejects_empty_pattern() {
        let err = lower("ast[rust]()").unwrap_err();
        assert_eq!(err[0].code, "ast/bad-pattern");
    }

    #[tokio::test]
    async fn pipe_matches_rust_function() {
        let ctx = RtCtx::default();
        let body = b"fn alpha() {}\nfn beta() {}\n";
        let c = Cursor::new(Arc::from(body.as_slice()));
        let op = lower("ast[rust](fn $N() {})").unwrap();
        let out = op.pipe(&ctx, c).await;
        assert_eq!(out.len(), 2);
        let names: Vec<String> = out
            .iter()
            .map(|c| {
                let n = c.captures.iter().find(|c| &*c.name == "N").unwrap();
                String::from_utf8_lossy(n.bytes(&c.content)).into_owned()
            })
            .collect();
        assert_eq!(names, vec!["alpha".to_string(), "beta".to_string()]);
    }

    #[tokio::test]
    async fn pipe_matches_typescript() {
        let ctx = RtCtx::default();
        let body = b"const x: number = 1;\nconst y: string = 'a';\n";
        let c = Cursor::new(Arc::from(body.as_slice()));
        let op = lower("ast[ts](const $X: $T = $V)").unwrap();
        let out = op.pipe(&ctx, c).await;
        assert_eq!(out.len(), 2);
    }

    #[tokio::test]
    async fn pipe_sub_token_sugar_extracts_prefix() {
        // `${TY}Error` over `MyError` and `OtherError` should bind TY to
        // `My` and `Other` respectively.
        let ctx = RtCtx::default();
        let body = b"type a = MyError;\ntype b = OtherError;\n";
        let c = Cursor::new(Arc::from(body.as_slice()));
        let op = lower("ast[ts](type $A = ${TY}Error)").unwrap();
        let out = op.pipe(&ctx, c).await;
        assert_eq!(out.len(), 2);
        let mut tys: Vec<String> = out
            .iter()
            .map(|c| {
                let t = c.captures.iter().find(|c| &*c.name == "TY").unwrap();
                String::from_utf8_lossy(t.bytes(&c.content)).into_owned()
            })
            .collect();
        tys.sort();
        assert_eq!(tys, vec!["My".to_string(), "Other".to_string()]);
    }
}
