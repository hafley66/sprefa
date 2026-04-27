//! `ast[lang](pattern)` — AST pattern matching via ast-grep-core.
//!
//! Ported from v2/src/ops/_9_ast_grep.rs. Rayon prefetch + reader cache
//! deferred (sprefa-4m7.2.22 spike) — v0 does per-cursor synchronous
//! parse + find_all on the cursor's active bytes.
//!
//! Examples:
//!   ast[rust](fn ${NAME?}($$${ARGS?}) { $$${BODY?} })
//!   ast[typescript](let ${X?}: ${TY?}Error = ${V?})
//!
//! Metavars (strict — bare `$NAME` / `$$$NAME` rejected at lower-time):
//!   - `${VAR}`        — single-node sprefa metavar. Synthetic
//!     `$SPRFSLOTN` collapses the surrounding identifier-char token run,
//!     a named regex pulls `VAR` from that slot's matched text. Lets
//!     you capture sub-token spans without breaking ast-grep's pattern
//!     grammar.
//!   - `$$${VAR}`      — multi-node metavar. The `$$$` ast-grep prefix
//!     sits OUTSIDE the host's `${...}` carveout (the host parses the
//!     leading `$` chars as opaque slot punctuation, then the `${VAR}`
//!     as a normal carveout). Only legal when the carveout sits free of
//!     identifier-char neighbours; sub-token multi is rejected.
//!     Rewrites to native `$$$SPRFSLOTN`; matched node texts are joined
//!     and bound as a synthesized capture.
//!
//! Strict-mode rationale: ast-grep pattern-by-example abuts identifier
//! characters, so bare `$N` ambiguates against legitimate identifiers
//! and against the `${}` carveout. One uniform metavar grammar across
//! str / json / ast keeps hover, completion, and term_positions on a
//! single path.
//!
//! Bracket-arg `[lang]` selects the ast-grep grammar: `rust` (`rs`) and
//! `typescript` (`ts`) are wired today.

use std::collections::HashMap;
use std::sync::Arc;

use ast_grep_core::{source::StrDoc, AstGrep, Pattern};
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
    /// Slots whose `$$$SPRFSLOTN` resolves via `get_multiple_matches`;
    /// matched node texts join into a single Synthesized capture under
    /// the user-facing name.
    pub multi_slots:    Vec<(String, Arc<str>)>,
    pub bound_caps:     Vec<Arc<str>>,
    pub _src:           Arc<str>,
    /// `${VAR}` / `${$$$VAR}` token spans, absolute into the host
    /// source. Surfaced via `Op::term_positions()` for LSP capture
    /// hover inside the opaque ast pattern body.
    pub term_positions: Arc<[crate::TermPosition]>,
}

impl OpCtor for AstGrepOp {
    const NAME: &'static str = "ast";
    const BODY_GRAMMAR: &'static str = "ast-grep pattern using `${VAR}` / `$$${VAR}`; lang via `[rust|ts]`";
    const DOC: &'static str = "\
`ast[lang](pattern)`

Match an ast-grep pattern against the cursor's active bytes. `lang` is
`rust` (`rs`) or `typescript` (`ts`).

Strict metavar grammar (bare `$NAME` / `$$$NAME` rejected):

- `${VAR}` — single-node capture. The surrounding identifier-char token
  run collapses to a synthetic `$SPRFSLOTN` and a named regex pulls
  `VAR` out of that slot's matched text. Use mid-token: `${TY}Error`
  matches `MyError` and binds `TY=My`.
- `$$${VAR}` — multi-node capture. The `$$$` prefix sits outside the
  carveout. Only legal with no identifier-char neighbours. Matched node
  texts join (space-separated) into one Synthesized capture under `VAR`.
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

    fn pipe<'a>(
        &'a self,
        ctx: &'a RtCtx,
        batch: Arc<[Cursor]>,
    ) -> BoxFuture<'a, Arc<[Cursor]>> {
        Box::pin(crate::_1_op::per_cursor(batch, move |c| async move {
            let Some(c) = crate::effects::ensure_content_loaded(ctx, c).await else {
                return Vec::new();
            };
            {
                let active = c.active();
                let Ok(src_str) = std::str::from_utf8(active) else { return Vec::new(); };
                let fixed = self.pattern.fixed_string();
                if !fixed.is_empty() && !src_str.contains(&*fixed) {
                    return Vec::new();
                }
            }
            let parse_req = crate::effects::AstParseEffect {
                content:    c.content.clone(),
                byte_range: c.byte_range.clone(),
                lang:       self.lang,
            };
            let Some(grep) = ctx.put(parse_req).await else {
                return Vec::new();
            };
            scan_with_grep(self, &c, &grep)
        }))
    }

    fn bound_captures(&self) -> &[Arc<str>] { &self.bound_caps }

    fn term_positions(&self) -> &[crate::TermPosition] { &self.term_positions }

    fn cache_key(&self, h: &mut blake3::Hasher) -> bool {
        h.update(self.name().as_bytes());
        h.update(&[0u8]);
        h.update(self.lang.to_string().as_bytes());
        h.update(&[0u8]);
        // `_src` is the raw pattern body the op was compiled from; pair it
        // with lang for identity.
        h.update(self._src.as_bytes());
        true
    }
}

/// Per-cursor scan with a pre-parsed grep tree. Shared by `pipe` (which
/// gets `grep` from `ctx.put(AstParseEffect)`) and the rayon path inside
/// `pipe_batch` (via `scan_one_inline`).
fn scan_with_grep(
    op:   &AstGrepOp,
    c:    &Cursor,
    grep: &AstGrep<StrDoc<SupportLang>>,
) -> Vec<Cursor> {
    let base = c.byte_range.start;
    let mut out: Vec<Cursor> = Vec::new();
    for nm in grep.root().find_all(&*op.pattern) {
        let env = nm.get_env();
        let mut cap_values: HashMap<Arc<str>, (Arc<str>, Option<std::ops::Range<usize>>)> =
            HashMap::new();

        let mut slot_ok = true;
        for (slot_name, re) in op.slot_regexes.iter() {
            let Some(node) = env.get_match(slot_name) else { slot_ok = false; break; };
            let text = node.text().to_string();
            let Some(caps) = re.captures(&text) else { slot_ok = false; break; };
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

        for (slot_name, cap_name) in op.multi_slots.iter() {
            let nodes = env.get_multiple_matches(slot_name);
            let joined: String = nodes.iter()
                .map(|n| n.text().to_string())
                .collect::<Vec<_>>()
                .join(" ");
            cap_values.insert(cap_name.clone(), (Arc::from(joined.as_str()), None));
        }

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

    reject_bare_metavars(raw, paren.byte_range())?;

    let (rewritten, slot_res, multi_slots, sugar_caps, sugar_positions) =
        lower_sugar(raw, paren.byte_range())?;

    let pattern = Pattern::try_new(&rewritten, lang).map_err(|e| {
        vec![PatternDiagnostic {
            code: "ast/bad-pattern",
            message: format!("ast pattern `{raw}` invalid: {e}"),
            byte_range: paren.byte_range(),
        }]
    })?;

    let mut bound: Vec<Arc<str>> = Vec::new();
    for n in sugar_caps.into_iter() {
        if !bound.iter().any(|s| s.as_ref() == n.as_ref()) { bound.push(n); }
    }

    // Sugar positions are bytes into the raw paren body. Lift to
    // absolute (paren start + 1 + body offset).
    let body_origin = pr.start + 1;
    let term_positions: Vec<crate::TermPosition> = sugar_positions
        .into_iter()
        .map(|(name, r)| crate::TermPosition {
            name,
            range: (body_origin + r.start)..(body_origin + r.end),
        })
        .collect();

    Ok(AstGrepOp {
        lang,
        pattern:        Arc::new(pattern),
        slot_regexes:   slot_res,
        multi_slots,
        bound_caps:     bound,
        _src:           Arc::from(raw),
        term_positions: Arc::from(term_positions.into_boxed_slice()),
    })
}

/// Reject bare `$NAME` / `$$$NAME` outside a `${...}` carveout. Strict
/// surface mandates `${NAME}` / `${$$$NAME}` so the same metavar grammar
/// works across str / json / ast.
fn reject_bare_metavars(
    pat:        &str,
    paren_span: std::ops::Range<usize>,
) -> Result<(), Vec<PatternDiagnostic>> {
    let b = pat.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'$' { i += 1; continue; }
        // `$$${...}` — multi-node carveout with outer-prefix. Skip past
        // matching `}`.
        if i + 3 < b.len() && b[i + 1] == b'$' && b[i + 2] == b'$' && b[i + 3] == b'{' {
            let mut j = i + 4;
            while j < b.len() && b[j] != b'}' { j += 1; }
            i = if j < b.len() { j + 1 } else { b.len() };
            continue;
        }
        // `${...}` — single-node carveout. Skip past matching `}`.
        if i + 1 < b.len() && b[i + 1] == b'{' {
            let mut j = i + 2;
            while j < b.len() && b[j] != b'}' { j += 1; }
            i = if j < b.len() { j + 1 } else { b.len() };
            continue;
        }
        // Bare `$` or `$$$` followed by an identifier char → illegal.
        let mut j = i + 1;
        let multi = j + 1 < b.len() && b[j] == b'$' && b[j + 1] == b'$';
        if multi { j += 2; }
        let name_start = j;
        while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_') { j += 1; }
        if j > name_start {
            let name = &pat[name_start..j];
            let canonical = if multi {
                format!("$$${{{name}}}")
            } else {
                format!("${{{name}}}")
            };
            return Err(vec![PatternDiagnostic {
                code: "ast/bare-metavar-illegal",
                message: format!(
                    "bare metavar `${}{name}` illegal; use `{canonical}` (strict ast surface)",
                    if multi { "$$$" } else { "" }
                ),
                byte_range: paren_span,
            }]);
        }
        i = j;
    }
    Ok(())
}

fn parse_lang(s: &str) -> Option<SupportLang> {
    match s {
        "rust" | "rs"       => Some(SupportLang::Rust),
        "typescript" | "ts" => Some(SupportLang::TypeScript),
        "c"                 => Some(SupportLang::C),
        _                   => None,
    }
}

/// Rewrite each `${VAR}` / `$$${VAR}` carveout. Single-form collapses
/// the surrounding identifier-char token run into `$SPRFSLOTN` plus a
/// named regex for sub-token extraction. Multi-form (`$$${VAR}`) carries
/// the `$$$` ast-grep prefix outside the carveout; only legal with no
/// identifier-char neighbours and rewrites to native `$$$SPRFSLOTN`;
/// bound text comes from `get_multiple_matches` joined with spaces.
fn lower_sugar(
    src:        &str,
    paren_span: std::ops::Range<usize>,
) -> Result<
    (
        String,
        Vec<(String, Regex)>,
        Vec<(String, Arc<str>)>,
        Vec<Arc<str>>,
        Vec<(Arc<str>, std::ops::Range<usize>)>,
    ),
    Vec<PatternDiagnostic>,
> {
    let bytes = src.as_bytes();
    let mut out = String::new();
    let mut regexes: Vec<(String, Regex)> = Vec::new();
    let mut multi_slots: Vec<(String, Arc<str>)> = Vec::new();
    let mut captures: Vec<Arc<str>> = Vec::new();
    let mut positions: Vec<(Arc<str>, std::ops::Range<usize>)> = Vec::new();
    let mut slot_idx = 0usize;
    let mut i = 0usize;

    while i < bytes.len() {
        // Outer-prefix multi-form `$$${...}`: ast-grep's `$$$` lives
        // OUTSIDE the host carveout. Detect first so single-form match
        // doesn't shadow it.
        let multi = bytes[i] == b'$'
            && i + 3 < bytes.len()
            && bytes[i + 1] == b'$'
            && bytes[i + 2] == b'$'
            && bytes[i + 3] == b'{';
        let single = !multi
            && bytes[i] == b'$'
            && i + 1 < bytes.len()
            && bytes[i + 1] == b'{';
        if multi || single {
            // `token_start` covers the full carveout including any outer
            // `$$$` prefix — used for term_positions span.
            let token_start = i;
            let inner_start = if multi { i + 4 } else { i + 2 };
            let mut j = inner_start;
            while j < bytes.len() && bytes[j] != b'}' { j += 1; }
            if j >= bytes.len() {
                return Err(vec![PatternDiagnostic {
                    code: "ast/bad-pattern",
                    message: format!("unterminated `${{` in pattern `{src}`"),
                    byte_range: paren_span,
                }]);
            }
            let inner = &src[inner_start..j];
            let inner_trimmed = inner.trim();
            let name_with_suffix = inner_trimmed;
            // `${NAME?}` Unbound suffix mirrors host grammar; same name
            // for v0 walker.
            let name = name_with_suffix.strip_suffix('?').unwrap_or(name_with_suffix);
            if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                return Err(vec![PatternDiagnostic {
                    code: "ast/bad-metavar",
                    message: format!("invalid metavar `${{{}}}` in ast pattern", inner_trimmed),
                    byte_range: paren_span,
                }]);
            }

            // Adjacent identifier-char neighbours.
            let prefix_bytes = {
                let ob = out.as_bytes();
                let mut k = ob.len();
                let mut count = 0usize;
                while k > 0 {
                    let c = ob[k - 1];
                    if c.is_ascii_alphanumeric() || c == b'_' { k -= 1; count += 1; } else { break; }
                }
                count
            };
            let after_brace = j + 1;
            let mut k = after_brace;
            while k < bytes.len() && (bytes[k].is_ascii_alphanumeric() || bytes[k] == b'_') {
                k += 1;
            }
            let suffix_bytes = k - after_brace;

            let slot_name = format!("SPRFSLOT{}", slot_idx);
            slot_idx += 1;

            if multi {
                if prefix_bytes != 0 || suffix_bytes != 0 {
                    return Err(vec![PatternDiagnostic {
                        code: "ast/multi-sub-token-illegal",
                        message: format!(
                            "multi metavar `$$${{{}}}` cannot sit adjacent to identifier characters \
                             (sub-token multi has no defined meaning); place it free of neighbours",
                            name
                        ),
                        byte_range: paren_span,
                    }]);
                }
                out.push_str("$$$");
                out.push_str(&slot_name);
                multi_slots.push((slot_name, Arc::from(name)));
            } else {
                let prefix: String = out[out.len() - prefix_bytes..].to_string();
                out.truncate(out.len() - prefix_bytes);
                let suffix: String = src[after_brace..k].to_string();
                out.push('$');
                out.push_str(&slot_name);
                let re_src = format!(
                    "^{}(?P<{}>\\S+){}$",
                    regex::escape(&prefix),
                    name,
                    regex::escape(&suffix),
                );
                if let Ok(re) = Regex::new(&re_src) {
                    regexes.push((slot_name, re));
                }
            }
            let cap_arc: Arc<str> = Arc::from(name);
            if !captures.iter().any(|s| s.as_ref() == cap_arc.as_ref()) {
                captures.push(cap_arc.clone());
            }
            // Token span = the whole carveout including outer `$$$`
            // prefix when present, body-relative. Caller lifts to
            // absolute when constructing TermPositions.
            positions.push((cap_arc, token_start..(j + 1)));
            i = k;
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }

    Ok((out, regexes, multi_slots, captures, positions))
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

    fn dummy_span() -> std::ops::Range<usize> { 0..0 }

    /// Synthetic-overhead pause applied to every `AstParseEffect`
    /// dispatch in the ast-grep operator tests. Establishes a known
    /// baseline so future perf comparisons can subtract a fixed
    /// per-dispatch latency. Adjust here, not per-test.
    const SYNTHETIC_PAUSE: std::time::Duration =
        std::time::Duration::from_millis(16);

    /// Async batcher that sleeps `SYNTHETIC_PAUSE` before delegating to
    /// the sync `ast_parse`. Drop-in replacement for `Passthrough` when
    /// a known synthetic overhead is wanted.
    struct PausedAstParse;
    impl effect_runtime::Batcher<crate::effects::AstParseEffect> for PausedAstParse {
        fn run(
            &self,
            req: crate::effects::AstParseEffect,
            cancel: effect_runtime::CancellationToken,
        ) -> effect_runtime::BoxFuture<
            'static,
            <crate::effects::AstParseEffect as effect_runtime::EffectKind>::Response,
        > {
            Box::pin(async move {
                tokio::select! {
                    _ = tokio::time::sleep(SYNTHETIC_PAUSE) => crate::effects::ast_parse(req),
                    _ = cancel.cancelled() => None,
                }
            })
        }
    }

    /// Test ctx with `AstParseEffect` registered against the
    /// `PausedAstParse` batcher. Tests bypass the `BoundedWorkSteal`
    /// worker pool because parse work is small in unit tests; the
    /// 16 ms synthetic pause stands in as a measurable baseline.
    fn test_ctx() -> RtCtx {
        use effect_runtime::RtCtxBuilder;
        use crate::effects::AstParseEffect;
        RtCtxBuilder::new()
            .register::<AstParseEffect, _>(PausedAstParse)
            .build()
    }

    #[test]
    fn lower_sugar_synthesizes_slot_and_regex() {
        let (out, res, multi, caps, _) = lower_sugar("${TY}Error", dummy_span()).unwrap();
        assert_eq!(out, "$SPRFSLOT0");
        assert_eq!(res.len(), 1);
        assert!(multi.is_empty());
        let names: Vec<&str> = caps.iter().map(|s| s.as_ref()).collect();
        assert_eq!(names, vec!["TY"]);
    }

    #[test]
    fn lower_sugar_passthrough_when_isolated() {
        // Isolated `${VAR}` becomes $SPRFSLOTN; the regex
        // `^(?P<VAR>\S+)$` identity-captures.
        let (out, res, _, _, _) = lower_sugar("fn ${NAME}() {}", dummy_span()).unwrap();
        assert_eq!(out, "fn $SPRFSLOT0() {}");
        assert_eq!(res.len(), 1);
    }

    #[test]
    fn lower_sugar_multi_emits_native_triple() {
        let (out, res, multi, caps, _) =
            lower_sugar("fn f($$${ARGS})", dummy_span()).unwrap();
        assert_eq!(out, "fn f($$$SPRFSLOT0)");
        assert!(res.is_empty());
        assert_eq!(multi.len(), 1);
        assert_eq!(&*multi[0].1, "ARGS");
        let names: Vec<&str> = caps.iter().map(|s| s.as_ref()).collect();
        assert_eq!(names, vec!["ARGS"]);
    }

    #[test]
    fn lower_sugar_rejects_multi_with_neighbours() {
        let err = lower_sugar("foo$$${X}bar", dummy_span()).unwrap_err();
        assert_eq!(err[0].code, "ast/multi-sub-token-illegal");
    }

    #[test]
    fn lower_basic_rust_pattern() {
        let op = lower("ast[rust](fn ${NAME}() {})").unwrap();
        let names: Vec<&str> = op.bound_captures().iter().map(|s| s.as_ref()).collect();
        assert!(names.contains(&"NAME"));
    }

    #[test]
    fn lower_rejects_bare_metavar() {
        let err = lower("ast[rust](fn $NAME() {})").unwrap_err();
        assert_eq!(err[0].code, "ast/bare-metavar-illegal");
    }

    #[test]
    fn lower_rejects_bare_multi() {
        let err = lower("ast[rust](fn f($$$ARGS) {})").unwrap_err();
        assert_eq!(err[0].code, "ast/bare-metavar-illegal");
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
        let ctx = test_ctx();
        let body = b"fn alpha() {}\nfn beta() {}\n";
        let c = Cursor::new(Arc::from(body.as_slice()));
        let op = lower("ast[rust](fn ${N}() {})").unwrap();
        let out = op.pipe(&ctx, Arc::from(vec![c])).await;
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
        let ctx = test_ctx();
        let body = b"const x: number = 1;\nconst y: string = 'a';\n";
        let c = Cursor::new(Arc::from(body.as_slice()));
        let op = lower("ast[ts](const ${X}: ${T} = ${V})").unwrap();
        let out = op.pipe(&ctx, Arc::from(vec![c])).await;
        assert_eq!(out.len(), 2);
    }

    #[tokio::test]
    async fn pipe_multi_metavar_joins_args() {
        let ctx = test_ctx();
        let body = b"fn f(a: i32, b: i32) {}\n";
        let c = Cursor::new(Arc::from(body.as_slice()));
        let op = lower("ast[rust](fn ${N}($$${ARGS}) {})").unwrap();
        let out = op.pipe(&ctx, Arc::from(vec![c])).await;
        assert_eq!(out.len(), 1);
        let args = out[0].captures.iter().find(|c| &*c.name == "ARGS").unwrap();
        let text = match &args.kind {
            crate::_0_cursor::CaptureKind::Synthesized { value } => value.as_ref(),
            _ => panic!("expected synthesized text for multi capture"),
        };
        assert!(text.contains("a") && text.contains("b"), "joined args text: {text}");
    }

    #[tokio::test]
    async fn pipe_sub_token_sugar_extracts_prefix() {
        // `${TY}Error` over `MyError` and `OtherError` should bind TY to
        // `My` and `Other` respectively.
        let ctx = test_ctx();
        let body = b"type a = MyError;\ntype b = OtherError;\n";
        let c = Cursor::new(Arc::from(body.as_slice()));
        let op = lower("ast[ts](type ${A} = ${TY}Error)").unwrap();
        let out = op.pipe(&ctx, Arc::from(vec![c])).await;
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

    // --- synthetic-pause baseline tests ---
    //
    // These pin the load-bearing properties around `SYNTHETIC_PAUSE`:
    //
    //   A. Floor — a real dispatch pays the pause (catches accidental
    //      `Passthrough` regression).
    //   B. Cancellability — a `cx.cancel_all()` mid-pause resolves the
    //      dispatch in « one pause (catches `select!` or cancel-token
    //      propagation regressions).
    //   C. Parallelism — N concurrent dispatches finish in « N pauses
    //      (catches accidental serialization in the batcher pool).
    //
    // Bounds are slack-tolerant: the lower edge is "pause not paid" and
    // the upper edge is "cancel/parallelism is broken". CI variance
    // sits comfortably inside the gap.

    fn timing_op() -> AstGrepOp {
        lower("ast[rust](fn ${N}() {})").unwrap()
    }

    fn timing_cursor() -> Cursor {
        let body: &[u8] = b"fn alpha() {}\n";
        Cursor::new(Arc::from(body))
    }

    #[tokio::test]
    async fn synthetic_pause_is_paid_per_dispatch() {
        let ctx = test_ctx();
        let op = timing_op();
        let c = timing_cursor();

        let t0 = std::time::Instant::now();
        let _ = op.pipe(&ctx, Arc::from(vec![c])).await;
        let elapsed = t0.elapsed();

        // Lower edge: 15ms (just under 16) absorbs sleep wake-up jitter
        // without letting a `Passthrough` regression sneak through.
        assert!(
            elapsed >= std::time::Duration::from_millis(15),
            "synthetic pause was not paid: {elapsed:?}",
        );
        // Upper edge: a single dispatch should not pay more than the
        // pause + a fixed slack. If we ever introduce serialization
        // (eg dropping `buffer_unordered` from `pipe_flat_map` default,
        // or putting a global Mutex in front of AstParseEffect dispatch)
        // a single dispatch will spike. Slack covers macOS sleep wakeup
        // jitter and CI-runner startup tax.
        let upper = SYNTHETIC_PAUSE + std::time::Duration::from_millis(48);
        assert!(
            elapsed < upper,
            "single dispatch took {elapsed:?}, expected < {upper:?}; \
             likely a runtime serialization regression",
        );
    }

    #[tokio::test]
    async fn synthetic_pause_is_cancellable_mid_flight() {
        let ctx = test_ctx();
        let op = timing_op();
        let c = timing_cursor();

        let canceller = {
            let ctx = ctx.clone();
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(2)).await;
                ctx.cancel_all();
            })
        };

        let t0 = std::time::Instant::now();
        let out = op.pipe(&ctx, Arc::from(vec![c])).await;
        let elapsed = t0.elapsed();
        canceller.await.unwrap();

        // Cancel-mid-pause must resolve well before the full pause.
        // Half the pause is the conservative ceiling; in practice it
        // resolves in 2-3 ms.
        assert!(
            elapsed < SYNTHETIC_PAUSE / 2,
            "cancel did not preempt pause: {elapsed:?}",
        );
        // Cancelled parse returns None → ast-grep op emits no cursors.
        assert!(out.is_empty(), "cancel should yield zero matches");
    }

    #[tokio::test]
    async fn synthetic_pauses_parallelize_across_dispatches() {
        let ctx = test_ctx();
        let n: usize = 8;

        let t0 = std::time::Instant::now();
        let mut set = tokio::task::JoinSet::new();
        for _ in 0..n {
            let ctx = ctx.clone();
            let op = timing_op();
            let c = timing_cursor();
            set.spawn(async move { op.pipe(&ctx, Arc::from(vec![c])).await });
        }
        while let Some(j) = set.join_next().await {
            let out = j.unwrap();
            assert_eq!(out.len(), 1);
        }
        let elapsed = t0.elapsed();

        // Floor: pause was actually paid (>= one full pause).
        assert!(
            elapsed >= std::time::Duration::from_millis(15),
            "even one pause was not paid: {elapsed:?}",
        );
        // Ceiling: real parallelism — wall time below the half-serial
        // cost. Generous slack handles single-thread test runners.
        let serial_half = SYNTHETIC_PAUSE * (n as u32 / 2);
        assert!(
            elapsed < serial_half,
            "{n} dispatches serialized to {elapsed:?}, expected < {serial_half:?}",
        );
    }
}
