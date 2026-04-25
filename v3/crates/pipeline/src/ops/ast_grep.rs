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
    const BODY_GRAMMAR: &'static str = "ast-grep pattern using ${VAR} / $$${VAR}; lang via [rust|ts]";
    const DOC: &'static str = "\
**ast**[_lang_](_pattern_)

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

    fn pipe<'a>(&'a self, ctx: &'a RtCtx, c: Cursor) -> BoxFuture<'a, Vec<Cursor>> {
        Box::pin(async move {
            let Some(c) = crate::effects::ensure_content_loaded(ctx, c).await else {
                return Vec::new();
            };

            // Prefilter probe before issuing the parse effect — most
            // files skip parse on the kernel corpus. FINDINGS §2.3.
            {
                let active = c.active();
                let Ok(src_str) = std::str::from_utf8(active) else { return Vec::new(); };
                let fixed = self.pattern.fixed_string();
                if !fixed.is_empty() && !src_str.contains(&*fixed) {
                    return Vec::new();
                }
            }

            // Single-cursor path: parse via `ctx.put(AstParseEffect)` —
            // routes through `BoundedWorkSteal` so the per-cursor caller
            // gets the worker-pool dispatch story. The bulk-cursor path
            // (`pipe_batch`) skips this and runs parse inline on rayon
            // for `ScanBatch` parity.
            let parse_req = crate::effects::AstParseEffect {
                content:    c.content.clone(),
                byte_range: c.byte_range.clone(),
                lang:       self.lang,
            };
            let Some(grep) = ctx.put(parse_req).await else {
                return Vec::new();
            };
            scan_with_grep(self, &c, &grep)
        })
    }

    fn pipe_batch<'a>(
        &'a self,
        ctx: &'a RtCtx,
        cs:  std::sync::Arc<[Cursor]>,
    ) -> BoxFuture<'a, Vec<Vec<Cursor>>> {
        Box::pin(async move {
            // Chunked bulk-read + parse to bound peak RSS. Loading all
            // 36k file bodies before scanning would hold ~1 GB of
            // bytes alive at once. Sliding chunks of `CHUNK` keep
            // peak working set ≈ chunk × avg-file-size while letting
            // rayon par_iter saturate cores within each chunk. Output
            // groups are concatenated in input order; `Vec<Vec<_>>`
            // alignment is preserved.
            // Tunable via `SPREFA_AST_CHUNK` for measurement.
            let chunk_env: usize = std::env::var("SPREFA_AST_CHUNK")
                .ok().and_then(|s| s.parse().ok())
                .unwrap_or(2048);
            let chunk_n = chunk_env.max(1);
            let total = cs.len();
            let mut groups: Vec<Vec<Cursor>> = Vec::with_capacity(total);
            let mut start = 0;
            while start < total {
                let end = (start + chunk_n).min(total);
                let slice: &[Cursor] = &cs[start..end];

                // Bulk read for this chunk only.
                let loaded: Vec<Option<Cursor>> =
                    crate::effects::ensure_content_loaded_batch(ctx, slice).await;

                // Parse + match on rayon. `spawn_blocking` hands off to
                // tokio's blocking pool so the current tokio worker is
                // free; rayon's global pool drives the par_iter.
                let pattern      = self.pattern.clone();
                let slot_regexes = self.slot_regexes.clone();
                let multi_slots  = self.multi_slots.clone();
                let lang         = self.lang;
                // Hoist `fixed_string` out of the par_iter — recomputes
                // from the pattern tree each call. One allocation per
                // chunk instead of one per cursor.
                let fixed: Arc<str> = Arc::from(pattern.fixed_string().as_ref());
                let chunk_groups: Vec<Vec<Cursor>> =
                    tokio::task::spawn_blocking(move || {
                        use rayon::prelude::*;
                        loaded.into_par_iter().map(|opt_c| {
                            let Some(c) = opt_c else { return Vec::new(); };
                            scan_one_inline(&pattern, &fixed, &slot_regexes, &multi_slots, lang, c)
                        }).collect::<Vec<Vec<Cursor>>>()
                    }).await.unwrap_or_default();
                // `chunk_groups` drops here; bytes inside its Cursors'
                // Arc<[u8]> drop with the last reference, freeing the
                // chunk's working set before the next read.
                groups.extend(chunk_groups);
                start = end;
            }
            groups
        })
    }

    fn bound_captures(&self) -> &[Arc<str>] { &self.bound_caps }

    fn term_positions(&self) -> &[crate::TermPosition] { &self.term_positions }
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

/// Sync per-cursor scan for the rayon path: prefilter → parse → match.
/// No `ctx.put`. Runs inside `tokio::task::spawn_blocking` + rayon
/// `par_iter`. Mirrors the bench `ScanBatch` handler.
fn scan_one_inline(
    pattern:      &Arc<Pattern<SupportLang>>,
    fixed:        &str,
    slot_regexes: &Vec<(String, Regex)>,
    multi_slots:  &Vec<(String, Arc<str>)>,
    lang:         SupportLang,
    c:            Cursor,
) -> Vec<Cursor> {
    let active = c.active();
    let Ok(src_str) = std::str::from_utf8(active) else { return Vec::new(); };
    if !fixed.is_empty() && !src_str.contains(fixed) {
        return Vec::new();
    }
    let grep: AstGrep<StrDoc<SupportLang>> = lang.ast_grep(src_str);
    // Reuse the shared scan logic by constructing a thin synthetic op
    // view of (pattern, slot_regexes, multi_slots). Avoids duplicating
    // the match-and-emit code across the two paths.
    let view = AstGrepOpRef { pattern, slot_regexes, multi_slots };
    view.scan(&c, &grep)
}

/// Lightweight borrow-only view used inside the rayon worker so we can
/// re-use `scan_with_grep`-style logic without holding the full
/// `AstGrepOp`. Avoids a clone of the whole op into every par_iter slot.
struct AstGrepOpRef<'a> {
    pattern:      &'a Arc<Pattern<SupportLang>>,
    slot_regexes: &'a Vec<(String, Regex)>,
    multi_slots:  &'a Vec<(String, Arc<str>)>,
}

impl<'a> AstGrepOpRef<'a> {
    fn scan(&self, c: &Cursor, grep: &AstGrep<StrDoc<SupportLang>>) -> Vec<Cursor> {
        let base = c.byte_range.start;
        let mut out: Vec<Cursor> = Vec::new();
        for nm in grep.root().find_all(&**self.pattern) {
            let env = nm.get_env();
            let mut cap_values: HashMap<Arc<str>, (Arc<str>, Option<std::ops::Range<usize>>)> =
                HashMap::new();
            let mut slot_ok = true;
            for (slot_name, re) in self.slot_regexes.iter() {
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
            for (slot_name, cap_name) in self.multi_slots.iter() {
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

    /// Test ctx with `AstParseEffect` registered against a synchronous
    /// passthrough handler. Tests bypass the `BoundedWorkSteal` worker
    /// pool because parse work is small and synchronous in unit tests.
    fn test_ctx() -> RtCtx {
        use effect_runtime::RtCtxBuilder;
        use effect_runtime::batchers::Passthrough;
        use crate::effects::{ast_parse, AstParseEffect};
        RtCtxBuilder::new()
            .register::<AstParseEffect, _>(Passthrough::<AstParseEffect, _>::new(ast_parse))
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
        let ctx = test_ctx();
        let body = b"const x: number = 1;\nconst y: string = 'a';\n";
        let c = Cursor::new(Arc::from(body.as_slice()));
        let op = lower("ast[ts](const ${X}: ${T} = ${V})").unwrap();
        let out = op.pipe(&ctx, c).await;
        assert_eq!(out.len(), 2);
    }

    #[tokio::test]
    async fn pipe_multi_metavar_joins_args() {
        let ctx = test_ctx();
        let body = b"fn f(a: i32, b: i32) {}\n";
        let c = Cursor::new(Arc::from(body.as_slice()));
        let op = lower("ast[rust](fn ${N}($$${ARGS}) {})").unwrap();
        let out = op.pipe(&ctx, c).await;
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
