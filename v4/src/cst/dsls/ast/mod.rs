//! `ast` — vanilla ast-grep-core wrapper.
//!
//! Borrowed-engine DSL: the body is a native ast-grep pattern (NOT
//! tree-sitter-backed at the surface, NOT hand-rolled). Compile builds
//! `Pattern::try_new`; match_into runs `AstGrep::new(target).root().find_all`.
//! Language is fixed at construction time via `AstDsl::new(SupportLang)`.
//!
//! Sprf-blind. The v3 carveouts (`${VAR}` carveout sugar, `$SPRFSLOTN` slot
//! synthesis, `$$${VAR}` multi-slot rewriting, bracket-arg `[lang]` parsing,
//! `term_positions` for hover) are dropped. Native `$NAME` / `$$$NAME` only.

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
    pub fn new(lang: SupportLang) -> Self { Self { lang } }
    pub fn lang(&self) -> SupportLang { self.lang }
}

impl Dsl for AstDsl {
    fn id(&self) -> &'static str { "ast" }

    fn compile(
        &self,
        body:  &[u8],
        diags: &dyn DiagSink,
    ) -> Result<Box<dyn Compiled>, Diag> {
        let body_str = std::str::from_utf8(body).map_err(|e| {
            Diag::error("ast.utf8", format!("body not utf-8: {e}"), 0..body.len())
        })?;

        let pattern = Pattern::try_new(body_str, self.lang).map_err(|e| {
            Diag::error("ast.pattern", format!("pattern compile failed: {e}"), 0..body.len())
        })?;

        let metavars = scan_metavars(body_str);
        let _ = diags;

        Ok(Box::new(AstCompiled {
            lang: self.lang,
            pattern: Arc::new(pattern),
            metavars,
        }))
    }
}

pub struct AstCompiled {
    lang:    SupportLang,
    pattern: Arc<Pattern<SupportLang>>,
    /// Metavar names scanned at compile time. Drives match-time env lookup;
    /// the resulting capture names emerge through `match_into`.
    metavars: Vec<Arc<str>>,
}

impl AstCompiled {
    pub fn lang(&self)    -> SupportLang        { self.lang }
    pub fn pattern(&self) -> &Pattern<SupportLang> { &self.pattern }
}

impl Compiled for AstCompiled {
    fn match_into(&self, target: &[u8], target_off: usize, sink: &mut dyn CaptureSink) {
        let target_str = match std::str::from_utf8(target) {
            Ok(s)  => s,
            Err(_) => return,
        };
        let grep: AstGrep<StrDoc<SupportLang>> = AstGrep::new(target_str, self.lang);

        for nm in grep.root().find_all(&*self.pattern) {
            let env = nm.get_env();

            for name in self.metavars.iter() {
                let n = name.as_ref();
                if let Some(node) = env.get_match(n) {
                    let r = node.range();
                    let row = CaptureRow {
                        name: name.clone(),
                        kind: CaptureKind::Span {
                            byte_range: (target_off + r.start)..(target_off + r.end),
                        },
                    };
                    if let ControlFlow::Break(_) = sink.emit(row) { return; }
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
                        kind: CaptureKind::Literal { value: Arc::from(joined.into_bytes()) },
                    };
                    if let ControlFlow::Break(_) = sink.emit(row) { return; }
                }
            }
        }
    }
}

/// Scan the pattern body for native ast-grep metavars: `$NAME` and `$$$NAME`.
/// Names are uppercase identifiers (`[A-Z_][A-Z0-9_]*`). Order preserved,
/// duplicates collapsed.
fn scan_metavars(body: &str) -> Vec<Arc<str>> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"\$\$?\$?([A-Z_][A-Z0-9_]*)").expect("metavar regex")
    });
    let mut seen: Vec<Arc<str>> = Vec::new();
    for caps in re.captures_iter(body) {
        if let Some(m) = caps.get(1) {
            let n = m.as_str();
            if !seen.iter().any(|s| s.as_ref() == n) {
                seen.push(Arc::from(n));
            }
        }
    }
    seen
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
            Ok(_)  => panic!("expected utf8 failure"),
        };
        assert_eq!(err.code, "ast.utf8");
    }
}
