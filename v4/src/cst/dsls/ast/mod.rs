//! `ast` — ast-grep-core wrapper with sprf sub-token carveout.
//!
//! Borrowed-engine DSL: the body is a native ast-grep pattern (NOT
//! tree-sitter-backed at the surface, NOT hand-rolled). Compile builds
//! `Pattern::try_new`; match_into runs `AstGrep::new(target).root().find_all`.
//! Language is fixed at construction time via `AstDsl::new(SupportLang)`.
//!
//! Native `$NAME` and `$$$NAME` metavars pass through unchanged. The v2
//! sprf carveouts are restored: `${NAME?}` and `${$$$NAME}` rewrite their
//! surrounding identifier-run into a synthetic `$SPRFSLOT<N>` metavar plus a
//! named regex that recovers `NAME` from the slot's matched text at match
//! time. v2 lineage: `v2/src/ops/_9_ast_grep.rs::lower_sugar`.

use std::ops::ControlFlow;
use std::sync::{Arc, OnceLock};

use ast_grep_core::{source::StrDoc, AstGrep, Pattern};
use ast_grep_language::SupportLang;
use regex::Regex;

use crate::cst::diag::{Diag, DiagSink};
use crate::cst::dsl::{CaptureKind, CaptureRow, CaptureSink, Compiled, Dsl};

pub struct AstDsl {
    lang: SupportLang,
}

impl AstDsl {
    pub fn new(lang: SupportLang) -> Self {
        Self { lang }
    }
    pub fn lang(&self) -> SupportLang {
        self.lang
    }
}

impl Dsl for AstDsl {
    fn id(&self) -> &'static str {
        "ast"
    }

    fn compile(&self, body: &[u8], diags: &dyn DiagSink) -> Result<Box<dyn Compiled>, Diag> {
        let body_str = std::str::from_utf8(body)
            .map_err(|e| Diag::error("ast.utf8", format!("body not utf-8: {e}"), 0..body.len()))?;

        let (rewritten, slot_regexes, sprf_caps) = lower_sugar(body_str);
        let native_caps = scan_metavars(body_str);

        let pattern = Pattern::try_new(&rewritten, self.lang).map_err(|e| {
            Diag::error(
                "ast.pattern",
                format!("pattern compile failed: {e}"),
                0..body.len(),
            )
        })?;

        let mut metavars: Vec<Arc<str>> = Vec::new();
        for n in native_caps.into_iter().chain(sprf_caps.into_iter()) {
            if !metavars.iter().any(|s| s.as_ref() == n.as_ref()) {
                metavars.push(n);
            }
        }
        let _ = diags;

        Ok(Box::new(AstCompiled {
            lang: self.lang,
            pattern: Arc::new(pattern),
            metavars,
            slot_regexes,
        }))
    }
}

pub struct AstCompiled {
    lang: SupportLang,
    pattern: Arc<Pattern<SupportLang>>,
    /// Metavar names scanned at compile time. Drives match-time env lookup;
    /// the resulting capture names emerge through `match_into`. Includes the
    /// sprf capture names (`NAME` from `${NAME?}`) so binding consumers see
    /// them at the same name they were written with.
    metavars: Vec<Arc<str>>,
    /// Synthetic `$SPRFSLOT<N>` slots paired with a regex that recovers the
    /// user-named capture from the slot's matched text. Empty for patterns
    /// with no sprf carveout.
    slot_regexes: Vec<(String, Regex)>,
}

impl AstCompiled {
    pub fn lang(&self) -> SupportLang {
        self.lang
    }
    pub fn pattern(&self) -> &Pattern<SupportLang> {
        &self.pattern
    }
}

impl Compiled for AstCompiled {
    fn match_into(&self, target: &[u8], target_off: usize, sink: &mut dyn CaptureSink) {
        let target_str = match std::str::from_utf8(target) {
            Ok(s) => s,
            Err(_) => return,
        };
        let grep: AstGrep<StrDoc<SupportLang>> = AstGrep::new(target_str, self.lang);

        for nm in grep.root().find_all(&*self.pattern) {
            let env = nm.get_env();

            // Sub-token extraction from synthetic slots. Each slot regex
            // names captures matching the user's `${NAME?}` identifier; we
            // emit them as Literal rows under that name. If any slot regex
            // fails to match the slot text, skip this whole find_all hit
            // (matches v2 semantics: a slot mismatch invalidates the row).
            let mut slot_failed = false;
            let mut slot_rows: Vec<CaptureRow> = Vec::new();
            for (slot_name, re) in self.slot_regexes.iter() {
                let node = match env.get_match(slot_name) {
                    Some(n) => n,
                    None => {
                        slot_failed = true;
                        break;
                    }
                };
                let text = node.text();
                let caps = match re.captures(text.as_ref()) {
                    Some(c) => c,
                    None => {
                        slot_failed = true;
                        break;
                    }
                };
                for cap_name in re.capture_names().flatten() {
                    if let Some(v) = caps.name(cap_name) {
                        slot_rows.push(CaptureRow {
                            name: Arc::from(cap_name),
                            kind: CaptureKind::Literal {
                                value: Arc::from(v.as_str().as_bytes()),
                            },
                        });
                    }
                }
            }
            if slot_failed {
                continue;
            }
            for row in slot_rows {
                if let ControlFlow::Break(_) = sink.emit(row) {
                    return;
                }
            }

            for name in self.metavars.iter() {
                let n = name.as_ref();
                // Sprf names are already emitted via slot regex above; do
                // not also try to look them up in the ast-grep env (they
                // never bind there).
                if self
                    .slot_regexes
                    .iter()
                    .any(|(_, re)| re.capture_names().flatten().any(|c| c == n))
                {
                    continue;
                }
                if let Some(node) = env.get_match(n) {
                    let r = node.range();
                    let row = CaptureRow {
                        name: name.clone(),
                        kind: CaptureKind::Span {
                            byte_range: (target_off + r.start)..(target_off + r.end),
                        },
                    };
                    if let ControlFlow::Break(_) = sink.emit(row) {
                        return;
                    }
                    continue;
                }
                let multi = env.get_multiple_matches(n);
                if !multi.is_empty() {
                    let joined: String = multi
                        .iter()
                        .map(|node| node.text().to_string())
                        .collect::<Vec<_>>()
                        .join(" ");
                    let row = CaptureRow {
                        name: name.clone(),
                        kind: CaptureKind::Literal {
                            value: Arc::from(joined.into_bytes()),
                        },
                    };
                    if let ControlFlow::Break(_) = sink.emit(row) {
                        return;
                    }
                }
            }
        }
    }
}

/// Scan the pattern body for native ast-grep metavars: `$NAME` and `$$$NAME`.
/// Names are uppercase identifiers (`[A-Z_][A-Z0-9_]*`). `${...}` spans are
/// skipped (they are sprf carveouts, handled by `lower_sugar`). Order
/// preserved, duplicates collapsed.
fn scan_metavars(body: &str) -> Vec<Arc<str>> {
    static NAME_RE: OnceLock<Regex> = OnceLock::new();
    let name_re = NAME_RE.get_or_init(|| Regex::new(r"^[A-Z_][A-Z0-9_]*").expect("name regex"));
    let bytes = body.as_bytes();
    let mut seen: Vec<Arc<str>> = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'$' {
            // Skip sprf `${...}` spans entirely.
            if i + 1 < bytes.len() && bytes[i + 1] == b'{' {
                i += 2;
                while i < bytes.len() && bytes[i] != b'}' {
                    i += 1;
                }
                if i < bytes.len() {
                    i += 1;
                }
                continue;
            }
            let mut j = i + 1;
            if j + 1 < bytes.len() && bytes[j] == b'$' && bytes[j + 1] == b'$' {
                j += 2;
            }
            let rest = &body[j..];
            if let Some(m) = name_re.find(rest) {
                let n = m.as_str();
                if !seen.iter().any(|s| s.as_ref() == n) {
                    seen.push(Arc::from(n));
                }
                i = j + m.end();
                continue;
            }
            i = j;
        } else {
            i += 1;
        }
    }
    seen
}

/// Rewrite each `${NAME?}` / `${$$$NAME}` plus its surrounding identifier-like
/// token run into `$SPRFSLOT<N>`, emitting a named regex that recovers NAME
/// from the slot's matched text. Trailing `?` on the inner name is the sprf
/// marker and is stripped before use. Returns the rewritten pattern body,
/// the slot regexes (paired by synthetic slot name), and the user-facing
/// capture names that need to bind at match time. Ported from v2's
/// `_9_ast_grep.rs::lower_sugar`.
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
            while j < bytes.len() && bytes[j] != b'}' {
                j += 1;
            }
            if j >= bytes.len() {
                out.push('$');
                i += 1;
                continue;
            }
            let inner = &src[inner_start..j];
            let inner_trimmed = inner.trim();
            let (multi, raw_name) = if let Some(rest) = inner_trimmed.strip_prefix("$$$") {
                (true, rest.trim())
            } else {
                (false, inner_trimmed)
            };
            // Strip the trailing `?` sprf marker. `${NAME?}` and `${NAME}`
            // both lower; the `?` form is the canonical sprf surface.
            let name = raw_name.strip_suffix('?').unwrap_or(raw_name);
            if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                out.push_str(&src[i..j + 1]);
                i = j + 1;
                continue;
            }

            // Pull adjacent identifier-char run from the already-written
            // output as prefix.
            let mut prefix_bytes = 0usize;
            {
                let ob = out.as_bytes();
                let mut k = ob.len();
                while k > 0 {
                    let c = ob[k - 1];
                    if c.is_ascii_alphanumeric() || c == b'_' {
                        k -= 1;
                        prefix_bytes += 1;
                    } else {
                        break;
                    }
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

            // No surrounding tokens: bind the user name directly as a
            // native ast-grep metavar; no slot/regex needed.
            if prefix.is_empty() && suffix.is_empty() {
                out.push('$');
                out.push_str(name);
                let cap_arc: Arc<str> = Arc::from(name);
                if !captures.iter().any(|s| s.as_ref() == cap_arc.as_ref()) {
                    captures.push(cap_arc);
                }
                i = k;
                continue;
            }

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
    use crate::cst::diag::SilentSink;
    use crate::cst::dsl::VecCaptureSink;

    #[test]
    fn scans_single_and_multi_metavars() {
        let bound = scan_metavars("fn $NAME($$$ARGS) { $$$BODY }");
        let names: Vec<&str> = bound.iter().map(|a| &**a).collect();
        assert_eq!(names, vec!["NAME", "ARGS", "BODY"]);
    }

    #[test]
    fn dedups_repeated_metavars() {
        let bound = scan_metavars("$X + $X");
        let names: Vec<&str> = bound.iter().map(|a| &**a).collect();
        assert_eq!(names, vec!["X"]);
    }

    #[test]
    fn rust_pattern_compiles_and_emits_metavar_at_match_time() {
        let dsl = AstDsl::new(SupportLang::Rust);
        let c = dsl.compile(b"fn $NAME() {}", &SilentSink).unwrap();
        let mut sink = VecCaptureSink::new();
        c.match_into(b"fn alpha() {}", 0, &mut sink);
        let names: Vec<&str> = sink.rows.iter().map(|r| &*r.name).collect();
        assert!(names.contains(&"NAME"));
    }

    #[test]
    fn rust_pattern_matches_function_name() {
        let dsl = AstDsl::new(SupportLang::Rust);
        let c = dsl.compile(b"fn $NAME() {}", &SilentSink).unwrap();
        let target = b"fn alpha() {} fn beta() {}";
        let mut sink = VecCaptureSink::new();
        c.match_into(target, 100, &mut sink);
        let spans: Vec<&[u8]> = sink
            .rows
            .iter()
            .map(|r| match &r.kind {
                CaptureKind::Span { byte_range } => {
                    &target[(byte_range.start - 100)..(byte_range.end - 100)]
                }
                _ => panic!("expected Span"),
            })
            .collect();
        assert!(spans.iter().any(|b| *b == b"alpha"));
        assert!(spans.iter().any(|b| *b == b"beta"));
    }

    #[test]
    fn multi_metavar_emits_literal_capture() {
        let dsl = AstDsl::new(SupportLang::Rust);
        let c = dsl.compile(b"fn $NAME($$$ARGS) {}", &SilentSink).unwrap();
        let mut sink = VecCaptureSink::new();
        c.match_into(b"fn f(x: i32, y: i32) {}", 0, &mut sink);
        let args_row = sink
            .rows
            .iter()
            .find(|r| &*r.name == "ARGS")
            .expect("ARGS capture");
        match &args_row.kind {
            CaptureKind::Literal { value } => {
                let s = std::str::from_utf8(value).unwrap();
                assert!(s.contains("x") && s.contains("y"));
            }
            _ => panic!("expected Literal for $$$ARGS"),
        }
    }

    #[test]
    fn non_utf8_body_returns_fatal_diag() {
        let dsl = AstDsl::new(SupportLang::Rust);
        let bad: &[u8] = &[0xff, 0xfe, 0xfd];
        let err = match dsl.compile(bad, &SilentSink) {
            Err(d) => d,
            Ok(_) => panic!("expected utf8 failure"),
        };
        assert_eq!(err.code, "ast.utf8");
    }
}
