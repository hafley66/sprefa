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
    // Fusion tracker: when an identifier-token run contains both a
    // metavar `${X}` and a hole `${re(...)}` (e.g. `use${HOOK}${re(...)}`),
    // they share one ast-grep metavar slot. ast-grep cannot match two
    // adjacent metavars on the same identifier, so we collapse them
    // into one slot whose regex carries both pieces.
    //
    // `last_single`: bookkeeping for the most recent single-form
    // carveout (metavar or hole). When the next carveout sits at
    // exactly `src_end` (no separator between), we extend the same
    // slot regex instead of emitting a new `$SPRFSLOTn` token.
    struct LastSingle {
        slot_name: String,
        // Bytes already matched inside the slot regex `^…$`, written
        // between the leading anchor and trailing `$`.
        body:      String,
        src_end:   usize,
    }
    let mut last_single: Option<LastSingle> = None;

    // Helper: finalize the in-flight fused slot (if any) by writing
    // its compiled regex to `regexes`. The fusion gate at top-of-loop
    // is responsible for absorbing any trailing literal id-chars into
    // `prev.body` BEFORE invoking finalize.
    let finalize_last = |last: &mut Option<LastSingle>,
                         regexes: &mut Vec<(String, Regex)>|
     -> Result<(), Vec<PatternDiagnostic>> {
        if let Some(prev) = last.take() {
            let re_src = format!("(?s)^{}$", prev.body);
            let re = Regex::new(&re_src).map_err(|err| vec![PatternDiagnostic {
                code: "ast/bad-pattern",
                message: format!("internal slot regex `{re_src}` invalid: {err}"),
                byte_range: 0..0,
            }])?;
            regexes.push((prev.slot_name, re));
        }
        Ok(())
    };

    while i < bytes.len() {
        // If we're sitting right after a carveout slot, decide fusion
        // BEFORE generic literal-byte handling. A `${` here means the
        // next carveout (if single, non-multi) extends the same slot.
        // Anything else closes the slot and we absorb any trailing
        // identifier bytes into its regex body.
        if let Some(prev) = last_single.as_ref() {
            if i == prev.src_end {
                let next_is_single_carveout = bytes[i] == b'$'
                    && i + 1 < bytes.len()
                    && bytes[i + 1] == b'{'
                    && !(i + 3 < bytes.len()
                        && bytes[i + 1] == b'$'
                        && bytes[i + 2] == b'$'
                        && bytes[i + 3] == b'{');
                // (Multi `$$${` never fuses — it has its own `$$$`
                // outer prefix that ast-grep parses separately.)
                if !next_is_single_carveout {
                    // Trailing identifier bytes (if any) belong to the
                    // slot's regex body — absorb them into `body` then
                    // finalize.
                    let mut k = i;
                    while k < bytes.len()
                        && (bytes[k].is_ascii_alphanumeric() || bytes[k] == b'_')
                    {
                        k += 1;
                    }
                    if k > i {
                        if let Some(prev) = last_single.as_mut() {
                            prev.body.push_str(&regex::escape(&src[i..k]));
                        }
                    }
                    finalize_last(&mut last_single, &mut regexes)?;
                    i = k;
                    continue;
                }
            }
        }

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
            // Hole detection: `${re(...)}` / `${glob(...)}`.
            let lookahead = &src[inner_start..bytes.len().min(inner_start + 8)];
            let hole_kind = if lookahead.starts_with("re(") {
                Some("re")
            } else if lookahead.starts_with("glob(") {
                Some("glob")
            } else {
                None
            };

            // ----- HOLE branch -----
            if let Some(op_name) = hole_kind {
                if multi {
                    return Err(vec![PatternDiagnostic {
                        code: "ast/bad-pattern",
                        message: format!(
                            "multi-form `$$${{...}}` cannot wrap a hole op `{op_name}(...)`; \
                             holes are anonymous synthetic metavars"
                        ),
                        byte_range: paren_span,
                    }]);
                }
                let body_open = inner_start + op_name.len() + 1;
                let body_end = match find_matching_paren(bytes, body_open - 1) {
                    Some(end) => end,
                    None => return Err(vec![PatternDiagnostic {
                        code: "ast/malformed-hole",
                        message: format!(
                            "hole `${{{op_name}(...)}}` missing closing `)` in pattern `{src}`"
                        ),
                        byte_range: paren_span,
                    }]),
                };
                let close_brace = body_end + 1;
                if close_brace >= bytes.len() || bytes[close_brace] != b'}' {
                    return Err(vec![PatternDiagnostic {
                        code: "ast/malformed-hole",
                        message: format!(
                            "hole `${{{op_name}(...)}}` must close `)}}` in pattern `{src}`"
                        ),
                        byte_range: paren_span,
                    }]);
                }
                let body_str = &src[body_open..body_end];
                let hole_rx_src = compile_hole_regex(op_name, body_str)
                    .map_err(|msg| vec![PatternDiagnostic {
                        code: "ast/hole-compilation-failed",
                        message: format!("hole `${{{op_name}({body_str})}}`: {msg}"),
                        byte_range: paren_span.clone(),
                    }])?;
                let span_label: Arc<str> = Arc::from(format!("{op_name}(...)").as_str());
                let span_end = close_brace + 1;
                positions.push((span_label, token_start..span_end));

                // Fuse if adjacent to previous single carveout, else
                // open a new slot (and finalize any prior).
                if let Some(prev) = last_single.as_mut() {
                    if prev.src_end == token_start {
                        prev.body.push_str("(?:");
                        prev.body.push_str(&hole_rx_src);
                        prev.body.push(')');
                        prev.src_end = span_end;
                        i = span_end;
                        continue;
                    }
                }
                finalize_last(&mut last_single, &mut regexes)?;

                // No fusion: collect literal-prefix from `out` (since
                // there's no previous slot, those bytes are literal
                // src chars copied verbatim).
                let prefix_bytes_h = {
                    let ob = out.as_bytes();
                    let mut kk = ob.len();
                    let mut count = 0usize;
                    while kk > 0 {
                        let c = ob[kk - 1];
                        if c.is_ascii_alphanumeric() || c == b'_' { kk -= 1; count += 1; }
                        else { break; }
                    }
                    count
                };
                let prefix_h: String = out[out.len() - prefix_bytes_h..].to_string();
                out.truncate(out.len() - prefix_bytes_h);
                let slot_name = format!("SPRFHOLE{}", slot_idx);
                slot_idx += 1;
                out.push('$');
                out.push_str(&slot_name);
                let mut body = String::new();
                body.push_str(&regex::escape(&prefix_h));
                body.push_str("(?:");
                body.push_str(&hole_rx_src);
                body.push(')');
                last_single = Some(LastSingle {
                    slot_name,
                    body,
                    src_end: span_end,
                });
                i = span_end;
                continue;
            }

            // ----- METAVAR branch -----
            let inner = &src[inner_start..j];
            let inner_trimmed = inner.trim();
            let name_with_suffix = inner_trimmed;
            let name = name_with_suffix.strip_suffix('?').unwrap_or(name_with_suffix);
            if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                return Err(vec![PatternDiagnostic {
                    code: "ast/bad-metavar",
                    message: format!("invalid metavar `${{{}}}` in ast pattern", inner_trimmed),
                    byte_range: paren_span,
                }]);
            }

            let after_brace = j + 1;
            let mut k = after_brace;
            while k < bytes.len() && (bytes[k].is_ascii_alphanumeric() || bytes[k] == b'_') {
                k += 1;
            }
            let suffix_bytes = k - after_brace;

            if multi {
                // Multi can never fuse — finalize prior, fresh slot.
                finalize_last(&mut last_single, &mut regexes)?;
                let prefix_bytes = {
                    let ob = out.as_bytes();
                    let mut kk = ob.len();
                    let mut count = 0usize;
                    while kk > 0 {
                        let c = ob[kk - 1];
                        if c.is_ascii_alphanumeric() || c == b'_' { kk -= 1; count += 1; }
                        else { break; }
                    }
                    count
                };
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
                let slot_name = format!("SPRFSLOT{}", slot_idx);
                slot_idx += 1;
                out.push_str("$$$");
                out.push_str(&slot_name);
                multi_slots.push((slot_name, Arc::from(name)));
                let cap_arc: Arc<str> = Arc::from(name);
                if !captures.iter().any(|s| s.as_ref() == cap_arc.as_ref()) {
                    captures.push(cap_arc.clone());
                }
                positions.push((cap_arc, token_start..(j + 1)));
                i = k;
                continue;
            }

            // Single metavar: try fusion.
            let cap_arc: Arc<str> = Arc::from(name);
            let span_end = j + 1;
            if let Some(prev) = last_single.as_mut() {
                if prev.src_end == token_start {
                    prev.body.push_str("(?P<");
                    prev.body.push_str(name);
                    prev.body.push_str(">.+?)");
                    prev.src_end = span_end;
                    if !captures.iter().any(|s| s.as_ref() == cap_arc.as_ref()) {
                        captures.push(cap_arc.clone());
                    }
                    positions.push((cap_arc, token_start..span_end));
                    i = span_end;
                    continue;
                }
            }
            finalize_last(&mut last_single, &mut regexes)?;

            let prefix_bytes = {
                let ob = out.as_bytes();
                let mut kk = ob.len();
                let mut count = 0usize;
                while kk > 0 {
                    let c = ob[kk - 1];
                    if c.is_ascii_alphanumeric() || c == b'_' { kk -= 1; count += 1; } else { break; }
                }
                count
            };
            let prefix: String = out[out.len() - prefix_bytes..].to_string();
            out.truncate(out.len() - prefix_bytes);
            let slot_name = format!("SPRFSLOT{}", slot_idx);
            slot_idx += 1;
            out.push('$');
            out.push_str(&slot_name);

            // Body so far: literal prefix (escaped) + named term.
            // Trailing literal id chars get absorbed at the next loop
            // iteration's "fusion gate" (which calls finalize_last).
            let mut body = String::new();
            body.push_str(&regex::escape(&prefix));
            body.push_str("(?P<");
            body.push_str(name);
            body.push_str(">.+?)");
            last_single = Some(LastSingle {
                slot_name,
                body,
                src_end: span_end,
            });
            if !captures.iter().any(|s| s.as_ref() == cap_arc.as_ref()) {
                captures.push(cap_arc.clone());
            }
            positions.push((cap_arc, token_start..span_end));
            i = span_end;
        } else {
            // Literal byte; finalize any in-flight slot.
            finalize_last(&mut last_single, &mut regexes)?;
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    finalize_last(&mut last_single, &mut regexes)?;

    Ok((out, regexes, multi_slots, captures, positions))
}

/// Find the matching `)` for the `(` at `bytes[open]`. Returns the index
/// of the closing paren, or `None` if not found. Tracks paren depth only;
/// does not handle quoted parens (acceptable for re/glob hole bodies in
/// practice — re patterns escape `(` as `\(`).
fn find_matching_paren(bytes: &[u8], open: usize) -> Option<usize> {
    debug_assert_eq!(bytes[open], b'(');
    let mut depth = 1isize;
    let mut i = open + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => { i += 2; continue; }
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 { return Some(i); }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Compile a hole's embedded op body to a regex source string suitable
/// for splicing into a slot regex. Handles `re` and `glob` ops by parsing
/// the body with the op's tree-sitter grammar and reusing the op's
/// existing `compile_from_tree` path. Anchors `^…$` from glob's compiled
/// regex are stripped because the slot regex re-anchors against the full
/// matched node text.
fn compile_hole_regex(op_name: &str, body: &str) -> Result<String, String> {
    use tree_sitter::Parser;
    let lang = crate::op_languages::language_of(op_name)
        .ok_or_else(|| format!("op `{op_name}` has no registered grammar"))?;
    let mut parser = Parser::new();
    parser.set_language(&lang).map_err(|e| e.to_string())?;
    let tree = parser
        .parse(body, None)
        .ok_or_else(|| "tree-sitter parse failed".to_string())?;
    let bytes = body.as_bytes();
    let raw_regex = match op_name {
        "re" => {
            let op = crate::ops::re::ReOp::compile_from_tree(&tree, bytes)
                .map_err(|d| d.into_iter().map(|d| d.message).collect::<Vec<_>>().join("; "))?;
            op.regex.as_str().to_string()
        }
        "glob" => {
            let op = crate::ops::glob::GlobOp::compile_from_tree(&tree, bytes)
                .map_err(|d| d.into_iter().map(|d| d.message).collect::<Vec<_>>().join("; "))?;
            // Glob bakes `^...$` anchors. Strip them — the outer slot
            // regex re-anchors to the matched node text.
            let s = op.regex.as_str();
            let s = s.strip_prefix('^').unwrap_or(s);
            let s = s.strip_suffix('$').unwrap_or(s);
            s.to_string()
        }
        other => return Err(format!("op `{other}` not supported in ast holes (use `re` or `glob`)")),
    };
    Ok(raw_regex)
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

    // --- regex/glob hole sugar (sprefa-3q3) ---

    #[test]
    fn lower_sugar_re_hole_synthesizes_constraint() {
        // Anonymous re hole: rewritten pattern carries `$SPRFHOLE0`,
        // slot regex anchors against the matched node text using the
        // re op's compiled regex source.
        let (out, res, multi, caps, _) =
            lower_sugar("${re(Query|LazyQuery|Mutation)}", dummy_span()).unwrap();
        assert_eq!(out, "$SPRFHOLE0");
        assert!(multi.is_empty());
        assert!(caps.is_empty(), "anonymous holes add no user-facing captures");
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].0, "SPRFHOLE0");
        let rx = res[0].1.as_str();
        assert!(rx.contains("Query|LazyQuery|Mutation"), "slot regex source: {rx}");
        assert!(rx.starts_with("(?s)^"), "slot regex must anchor: {rx}");
        assert!(rx.ends_with("$"));
    }

    #[test]
    fn lower_sugar_glob_hole_strips_anchors() {
        // Glob bakes `^...$`; the slot regex re-anchors so the inner
        // hole regex must arrive without those anchors.
        let (out, res, _, _, _) = lower_sugar("${glob(*.rs)}", dummy_span()).unwrap();
        assert_eq!(out, "$SPRFHOLE0");
        assert_eq!(res.len(), 1);
        let rx = res[0].1.as_str();
        // Slot regex shape: `(?s)^(?:<glob_no_anchors>)$`. The glob's
        // own `^…$` anchors must be stripped; only the slot's outer
        // anchors remain. The inner group should be `(?:[^/]*\.rs)`
        // (no leading `^`, no trailing `$` before the closing paren).
        assert!(rx.contains("(?:[^/]*\\.rs)"), "expected stripped glob inside slot: {rx}");
        assert!(!rx.contains("(?:^"), "glob `^` anchor leaked into slot: {rx}");
        assert!(!rx.contains("$)"), "glob `$` anchor leaked into slot: {rx}");
    }

    #[test]
    fn lower_sugar_re_hole_with_named_group_exposes_capture_via_regex() {
        // Named groups inside a hole regex survive into the slot regex
        // and get picked up by `scan_with_grep` via `capture_names()`.
        let (out, res, _, caps, _) =
            lower_sugar("${re((?P<KIND>Query|Mutation))}", dummy_span()).unwrap();
        assert_eq!(out, "$SPRFHOLE0");
        // The synthesized hole adds no name to `caps` (anonymous), but
        // the slot regex itself exposes KIND.
        assert!(caps.is_empty());
        let names: Vec<&str> = res[0].1.capture_names().flatten().collect();
        assert!(names.contains(&"KIND"), "expected KIND in capture_names: {names:?}");
    }

    #[test]
    fn lower_sugar_re_hole_with_prefix_collapses() {
        // `use${re(...)}` — prefix `use` collapses into the slot regex.
        let (out, res, _, _, _) =
            lower_sugar("use${re(Query|Mutation)}", dummy_span()).unwrap();
        assert_eq!(out, "$SPRFHOLE0");
        let rx = res[0].1.as_str();
        assert!(rx.contains("use"), "prefix not spliced into slot regex: {rx}");
    }

    #[test]
    fn lower_sugar_re_hole_alongside_metavar() {
        // RTKQ shape: `use${HOOK?}${re(...)}` — metavar and hole share
        // the same identifier-token run, so they FUSE into one slot
        // whose regex carries both pieces.
        //
        //   `use(?P<HOOK>.+?)(?:Query|LazyQuery|Mutation)$`
        //
        // ast-grep can't match two adjacent metavars on a single
        // identifier; fusion is the load-bearing trick.
        let (out, res, _, caps, _) =
            lower_sugar("use${HOOK?}${re(Query|LazyQuery|Mutation)}()", dummy_span()).unwrap();
        assert_eq!(out, "$SPRFSLOT0()", "fused single slot rewrite: {out}");
        assert_eq!(res.len(), 1, "fusion collapses to one slot regex");
        let rx = res[0].1.as_str();
        assert!(rx.contains("(?P<HOOK>.+?)"), "metavar group missing: {rx}");
        assert!(rx.contains("Query|LazyQuery|Mutation"), "hole alt missing: {rx}");
        assert!(rx.starts_with("(?s)^use"), "literal prefix missing: {rx}");
        let names: Vec<&str> = caps.iter().map(|s| s.as_ref()).collect();
        assert_eq!(names, vec!["HOOK"], "only metavar HOOK is user-visible");
    }

    // --- sprefa-236: pin the user-facing shape end-to-end ---

    #[tokio::test]
    async fn pipe_fusion_use_opname_re_with_multi_args() {
        // The exact shape from the sprefa-236 ask:
        //   use${OP_NAME?}${re(Query|LazyQuery|Mutation)}($$$)
        //
        // Fuses metavar+re into one slot, then ast-grep's bare `$$$`
        // sweeps any number of args. Should bind OP_NAME to the prefix
        // before the re alternation, drop call sites whose name doesn't
        // end with one of the listed suffixes.
        let ctx = test_ctx();
        let body = b"\
const a = useFooQuery(1, 2);
const b = useBarMutation();
const c = useBaz(99);
const d = useThingLazyQuery(x, y, z);
";
        let c = Cursor::new(Arc::from(body.as_slice()));
        let op =
            lower("ast[ts](use${OP_NAME?}${re(Query|LazyQuery|Mutation)}($$$))").unwrap();
        let out = op.pipe(&ctx, Arc::from(vec![c])).await;
        let mut bound: Vec<String> = out.iter().map(|c| {
            let n = c.captures.iter().find(|c| &*c.name == "OP_NAME").unwrap();
            String::from_utf8_lossy(n.bytes(&c.content)).into_owned()
        }).collect();
        bound.sort();
        assert_eq!(bound, vec!["Bar".to_string(), "Foo".to_string(), "Thing".to_string()]);
    }

    // Known gap (sprefa-236 follow-up): `${X?}${glob(*Service)}` lowers
    // to `(?P<X>.+?)(?:[^/]*Service)$`. The glob's leading `*` becomes
    // greedy `[^/]*` next to the term's non-greedy `.+?`; the greedy
    // `[^/]*` eats the prefix the user expects X to bind. Resolution
    // would special-case fusion when the hole's regex begins with a
    // wildcard quantifier (collapse the `*` into the term). Filed for
    // follow-up; the re path (no leading wildcard, alternation literal)
    // works as expected today.

    #[tokio::test]
    async fn pipe_two_adjacent_terms_first_term_grabs_minimum() {
        // Two unconstrained metavars adjacent collapse into
        // `(?P<A>.+?)(?P<B>.+?)`. Both are non-greedy, so A grabs the
        // minimum (one char) and B grabs the rest. No constraint => no
        // sensible split, but the regex still resolves deterministically.
        // Pinned so a future change to fusion behavior surfaces here.
        let ctx = test_ctx();
        let body = b"type t = FooBar;\n";
        let c = Cursor::new(Arc::from(body.as_slice()));
        let op = lower("ast[ts](type t = ${A?}${B?})").unwrap();
        let out = op.pipe(&ctx, Arc::from(vec![c])).await;
        assert_eq!(out.len(), 1);
        let cap = |name: &str| -> String {
            let cc = &out[0];
            let m = cc.captures.iter().find(|c| &*c.name == name).unwrap();
            String::from_utf8_lossy(m.bytes(&cc.content)).into_owned()
        };
        assert_eq!(cap("A"), "F");
        assert_eq!(cap("B"), "ooBar");
    }

    #[test]
    fn lower_sugar_re_hole_unterminated_paren_diag() {
        // `${re(unclosed}` — the carveout terminates at `}` but the
        // hole's `(` never closes. Hole-specific diagnostic.
        let err = lower_sugar("${re(unclosed}", dummy_span()).unwrap_err();
        assert_eq!(err[0].code, "ast/malformed-hole");
    }

    #[test]
    fn lower_sugar_multi_hole_rejected() {
        // `$$${re(...)}` is malformed — multi wraps a metavar name, not
        // an inline op.
        let err = lower_sugar("$$${re(Foo)}", dummy_span()).unwrap_err();
        assert_eq!(err[0].code, "ast/bad-pattern");
    }

    #[tokio::test]
    async fn pipe_re_hole_filters_matches() {
        // RTKQ pattern: `use${HOOK?}${re(Query|LazyQuery|Mutation)}()`
        // should match useFooQuery and useBarMutation but skip useBaz
        // (no constraint suffix).
        let ctx = test_ctx();
        let body = b"\
const a = useFooQuery();
const b = useBarMutation();
const c = useBaz();
";
        let c = Cursor::new(Arc::from(body.as_slice()));
        let op = lower("ast[ts](use${HOOK}${re(Query|LazyQuery|Mutation)}())").unwrap();
        let out = op.pipe(&ctx, Arc::from(vec![c])).await;
        assert_eq!(
            out.len(), 2,
            "hole constraint should drop useBaz; got {} matches", out.len()
        );
        let mut hooks: Vec<String> = out.iter().map(|c| {
            let n = c.captures.iter().find(|c| &*c.name == "HOOK").unwrap();
            String::from_utf8_lossy(n.bytes(&c.content)).into_owned()
        }).collect();
        hooks.sort();
        assert_eq!(hooks, vec!["Bar".to_string(), "Foo".to_string()]);
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

    // --- regression coverage for `selfcheck.sprf` patterns ---
    //
    // These pin the patterns the dogfood `selfcheck.sprf` fixture
    // depends on. Each one is tested against actual Rust source that
    // grep agrees contains the construct. A regression here means
    // selfcheck silently goes back to "0 cursors emitted" in the LSP
    // hover panel and on `sprefa-run` stderr.
    //
    // 2026-04-27 finding: every shape below MATCHES at the
    // `lower → op.pipe(in-memory cursor)` level. The selfcheck.sprf
    // 0-match symptom is therefore not a pattern-compile gap — it's
    // upstream (cursor content / byte_range / fs+ast integration) and
    // is exercised by the smoke harness, not these unit tests.

    #[tokio::test]
    async fn selfcheck_method_call_unit() {
        // The one shape that works today; if this regresses, every
        // selfcheck `${V?}.unwrap()` lint goes silent.
        let ctx = test_ctx();
        let body = b"fn f() { let _ = x.unwrap(); let _ = y.unwrap(); }\n";
        let c = Cursor::new(Arc::from(body.as_slice()));
        let op = lower("ast[rust](${V}.unwrap())").unwrap();
        let out = op.pipe(&ctx, Arc::from(vec![c])).await;
        assert_eq!(out.len(), 2, "method-call shape must keep matching");
    }

    #[tokio::test]
    async fn selfcheck_macro_with_metavar_arg() {
        // `panic!(${MSG?})` over real `panic!("boom")`. Sample taken
        // from v3/crates/pipeline/src/ops/ast_grep.rs L694 (`panic!(...)`
        // in this very file).
        let ctx = test_ctx();
        let body = b"fn f() { panic!(\"boom\"); }\n";
        let c = Cursor::new(Arc::from(body.as_slice()));
        let op = lower("ast[rust](panic!(${MSG}))").unwrap();
        let out = op.pipe(&ctx, Arc::from(vec![c])).await;
        assert!(
            !out.is_empty(),
            "macro_invocation pattern matched 0 cursors against `panic!(\"boom\")`"
        );
    }

    #[tokio::test]
    async fn selfcheck_macro_metavar_binds_text_with_whitespace() {
        // The exact bug that made selfcheck.sprf show 0 panic hits
        // against the live tree: every real panic message has spaces,
        // and the slot regex used `\S+` which rejects whitespace.
        // Two non-trivial shapes both must bind MSG:
        //   panic!("expected Object")             — quoted string + space
        //   panic!(\n    "multi-line msg"\n)    — newline inside the call
        //
        // Out of scope (ast-grep 0.36 limitation, not the slot regex):
        //   panic!(format!(...))                  — nested macro
        //     in the token-tree arg slot. ast-grep does not reliably
        //     bind a metavar to a nested macro_invocation here. File
        //     under sprefa-4m7.3 if it bites.
        let ctx = test_ctx();
        let body = b"\
fn a() { panic!(\"expected Object\"); }
fn b() {
    panic!(
        \"multi-line msg\"
    );
}
";
        let c = Cursor::new(Arc::from(body.as_slice()));
        let op = lower("ast[rust](panic!(${MSG}))").unwrap();
        let out = op.pipe(&ctx, Arc::from(vec![c])).await;
        let bound: Vec<String> = out.iter().map(|c| {
            let m = c.captures.iter().find(|c| &*c.name == "MSG").unwrap();
            String::from_utf8_lossy(m.bytes(&c.content)).into_owned()
        }).collect();
        assert_eq!(
            out.len(), 2,
            "panic!(${{MSG}}) must bind MSG even when the message contains \
             whitespace or newlines (got {}: {:?})",
            out.len(), bound
        );
        assert!(bound.iter().any(|s| s.contains("expected Object")));
        assert!(bound.iter().any(|s| s.contains("multi-line msg")));
    }

    #[tokio::test]
    async fn selfcheck_bare_macro_no_args() {
        let ctx = test_ctx();
        let body = b"fn f() { todo!(); }\n";
        let c = Cursor::new(Arc::from(body.as_slice()));
        let op = lower("ast[rust](todo!())").unwrap();
        let out = op.pipe(&ctx, Arc::from(vec![c])).await;
        assert!(
            !out.is_empty(),
            "bare-macro pattern matched 0 cursors against `todo!()`"
        );
    }

    #[tokio::test]
    async fn selfcheck_path_call_zero_args() {
        let ctx = test_ctx();
        let body = b"fn f() { let _ = RtCtxBuilder::new(); }\n";
        let c = Cursor::new(Arc::from(body.as_slice()));
        let op = lower("ast[rust](RtCtxBuilder::new())").unwrap();
        let out = op.pipe(&ctx, Arc::from(vec![c])).await;
        assert!(
            !out.is_empty(),
            "path-call pattern matched 0 cursors against `RtCtxBuilder::new()`"
        );
    }

    #[tokio::test]
    async fn selfcheck_method_call_with_string_arg() {
        // `${V?}.expect("")` shape. ast-grep should bind V to the
        // receiver and match on the empty-string literal arg.
        let ctx = test_ctx();
        let body = b"fn f() { let _ = x.expect(\"\"); }\n";
        let c = Cursor::new(Arc::from(body.as_slice()));
        let op = lower("ast[rust](${V}.expect(\"\"))").unwrap();
        let out = op.pipe(&ctx, Arc::from(vec![c])).await;
        assert!(
            !out.is_empty(),
            "string-arg method pattern matched 0 cursors against `x.expect(\"\")`"
        );
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
