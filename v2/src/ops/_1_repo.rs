//! $repo — filter/bind on `cursor.repo`.
//!
//! Paren slot is either:
//!   - a glob pattern (e.g. `myorg/*`) → filter cursors whose repo matches
//!   - a capture (e.g. `$R`)           → bind `R` to `cursor.repo`, pass all through
//!
//! This op does not expand cursors; seeding comes from `$rule`.

use std::sync::Arc;

use futures_util::stream::StreamExt;
use futures_core::stream::BoxStream;

use crate::_0_types::{Capture, Cursor, ParseSite, Tri};
use crate::_1_diagnostic::{Diagnostic, Renderer};
use crate::_5_op::{
    hover_render_grouped, BraceMode, CompletionItem, GrammarRef, Op, OpCtx, OpInvocation,
    Operator, Pipeline, ProgramCtx, ScanPointer,
};
use crate::_8_parse::{classify_token, TokenClass};
use crate::_16_pattern::CompiledPattern;

// ---------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------

pub struct RepoFactory;

impl Operator for RepoFactory {
    fn name(&self) -> &'static str { "repo" }
    fn paren_grammar(&self) -> GrammarRef { GrammarRef(Arc::from("repo-arg")) }
    fn brace_mode(&self) -> BraceMode { BraceMode::DefaultFork }

    fn scan_pointers(&self) -> &'static [ScanPointer] {
        static P: &[ScanPointer] = &[
            ScanPointer { sigil: "repo",      read: |c| Some(c.repo.clone()) },
            ScanPointer { sigil: "repo_norm", read: |c| Some(c.repo.clone()) },
        ];
        P
    }

    fn completion_item(&self) -> CompletionItem {
        CompletionItem {
            label:  "repo".to_string(),
            detail: "repo(glob | $CAP)".to_string(),
            doc:    "# repo\n\nFilter cursors by repo slug glob, or bind the slug to a capture.".to_string(),
        }
    }

    fn wildcard_instance(&self, parse_site: Arc<ParseSite>) -> Option<Arc<dyn Op>> {
        let pat = CompiledPattern::compile("*").ok()?;
        Some(Arc::new(RepoOp {
            mode: RepoMode::Filter(Arc::new(pat)),
            parse_site,
        }))
    }

    fn parse(&self, inv: &OpInvocation, _pctx: &mut ProgramCtx)
        -> Result<Pipeline, Vec<Box<dyn Diagnostic>>>
    {
        let paren = inv.paren_src.as_ref().ok_or_else(|| {
            vec![Box::new(RepoDiag::MissingArg { site: (*inv.parse_site).clone() })
                as Box<dyn Diagnostic>]
        })?;
        let arg = paren.src.trim();
        let bad_arg = |site: &ParseSite| Box::new(RepoDiag::BadArg {
            site: site.clone(),
            got:  Arc::from(arg),
        }) as Box<dyn Diagnostic>;
        let mode = match classify_token(arg) {
            TokenClass::Capture(c)     => RepoMode::Bind(c.name),
            TokenClass::Literal        => {
                let pat = CompiledPattern::compile(arg)
                    .map_err(|_| vec![bad_arg(&inv.parse_site)])?;
                RepoMode::Filter(Arc::new(pat))
            }
            TokenClass::ScanPointer(_)
          | TokenClass::CrossRef(_)    => {
                return Err(vec![bad_arg(&inv.parse_site)]);
            }
        };
        Ok(Pipeline::Op(Arc::new(RepoOp {
            mode,
            parse_site: inv.parse_site.clone(),
        }).into()))
    }
}

// ---------------------------------------------------------------------------
// Op
// ---------------------------------------------------------------------------

pub struct RepoOp {
    mode:       RepoMode,
    parse_site: Arc<ParseSite>,
}

enum RepoMode {
    Filter(Arc<CompiledPattern>),   // shared matcher — glob/regex/segment
    Bind(Arc<str>),                 // capture name
}

impl Op for RepoOp {
    fn name(&self) -> &'static str { "repo" }
    fn step(&self) -> u16 { 0 }
    fn parse_site(&self) -> &Arc<ParseSite> { &self.parse_site }

    fn witness(&self, c: &Cursor) -> Option<Arc<str>> { Some(c.repo.clone()) }

    fn capture_name(&self) -> Option<Arc<str>> {
        match &self.mode { RepoMode::Bind(n) => Some(n.clone()), _ => None }
    }

    fn hover_self(&self) -> String {
        let src = match &self.mode {
            RepoMode::Filter(p) => p.src.as_ref().to_string(),
            RepoMode::Bind(n)   => format!("${}", n),
        };
        format!("# repo({})", src)
    }

    fn hover_capture(&self, cap: &str, cursors: &[Cursor]) -> Option<String> {
        let header = format!("**`${cap}`** repos:");
        let entries: Vec<(Option<String>, String, String)> = cursors.iter()
            .filter_map(|c| {
                c.captures.get(cap).map(|capture| (
                    c.fs.as_ref().map(|fp| fp.0.to_string_lossy().into_owned()),
                    c.rev.to_string(),
                    capture.value.to_string(),
                ))
            })
            .collect();
        hover_render_grouped(&header, &entries)
    }

    fn hover_match(&self, site: &crate::_0_types::ParseSite, cursors: &[Cursor]) -> Option<String> {
        let entries: Vec<(Option<String>, String, String)> = cursors.iter()
            .filter(|c| c.evidence.iter().any(|ev|
                ev.op_name == "repo" && ev.parse_site.as_ref() == site
            ))
            .map(|c| (
                c.fs.as_ref().map(|fp| fp.0.to_string_lossy().into_owned()),
                c.rev.to_string(),
                c.repo.to_string(),
            ))
            .collect();
        hover_render_grouped("matches:", &entries)
    }

    fn pipe(&self, input: BoxStream<'static, Arc<[Cursor]>>, _ctx: OpCtx)
        -> BoxStream<'static, Arc<[Cursor]>>
    {
        match &self.mode {
            RepoMode::Filter(pat) => {
                let pat = pat.clone();
                input.map(move |batch| {
                    let kept: Vec<Cursor> = batch.iter()
                        .filter(|c| pat.is_match(&c.repo))
                        .cloned()
                        .collect();
                    Arc::<[Cursor]>::from(kept.into_boxed_slice())
                }).boxed()
            }
            RepoMode::Bind(name) => {
                let name = name.clone();
                input.map(move |batch| {
                    let mapped: Vec<Cursor> = batch.iter().map(|c| {
                        let mut c = c.clone();
                        let v = c.repo.clone();
                        c.captures.insert(name.clone(), Capture::new(v).with_scan(Arc::from("repo"), Tri::Verified));
                        c
                    }).collect();
                    Arc::<[Cursor]>::from(mapped.into_boxed_slice())
                }).boxed()
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Diagnostics — owned here
// ---------------------------------------------------------------------------

#[derive(Debug)]
enum RepoDiag {
    MissingArg { site: ParseSite },
    BadArg     { site: ParseSite, got: Arc<str> },
}

impl Diagnostic for RepoDiag {
    fn code(&self) -> &str {
        match self {
            RepoDiag::MissingArg { .. } => "repo/missing-arg",
            RepoDiag::BadArg     { .. } => "repo/bad-arg",
        }
    }
    fn severity(&self) -> crate::_0_types::Severity { crate::_0_types::Severity::Error }
    fn primary(&self) -> &ParseSite {
        match self {
            RepoDiag::MissingArg { site } => site,
            RepoDiag::BadArg     { site, .. } => site,
        }
    }
    fn render(&self, out: &mut dyn Renderer) {
        match self {
            RepoDiag::MissingArg { site } => {
                out.header(self.code(), self.severity(),
                    "repo requires a glob pattern or capture (e.g. repo(myorg/*) or repo($R))");
                out.primary(site);
            }
            RepoDiag::BadArg { site, got } => {
                out.header(self.code(), self.severity(),
                    &format!("repo argument `{got}` is not a glob or capture"));
                out.primary(site);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use crate::_0_types::{Capture, FilePath, OpEvidence, RunId};

    fn dummy_site() -> Arc<ParseSite> {
        Arc::new(ParseSite {
            file:       Arc::from(Path::new("test.sprf")),
            path:       Arc::from(vec![].into_boxed_slice()),
            byte_range: 0..1,
        })
    }

    fn base_cursor(repo: &str, rev: &str, fs: Option<&str>) -> Cursor {
        Cursor {
            run_id: RunId(0),
            repo:   Arc::from(repo),
            rev:    Arc::from(rev),
            fs:     fs.map(|p| FilePath(Arc::from(Path::new(p)))),
            ..Default::default()
        }
    }

    #[test]
    fn scan_pointers_expose_repo_and_repo_norm() {
        let f = RepoFactory;
        let ps = Operator::scan_pointers(&f);
        let sigils: Vec<&str> = ps.iter().map(|p| p.sigil).collect();
        assert_eq!(sigils, vec!["repo", "repo_norm"]);
        let c = base_cursor("org/alpha", "main", None);
        assert_eq!(&*(ps[0].read)(&c).unwrap(), "org/alpha");
        assert_eq!(&*(ps[1].read)(&c).unwrap(), "org/alpha");
    }

    #[test]
    fn hover_capture_groups_by_file_rev() {
        let site = dummy_site();
        let op = RepoOp { mode: RepoMode::Bind(Arc::from("R")), parse_site: site.clone() };

        let mut c1 = base_cursor("org/alpha", "main", Some("Cargo.toml"));
        c1.captures.insert(Arc::from("R"), Capture::new(Arc::from("org/alpha")));

        let mut c2 = base_cursor("org/beta", "main", Some("Cargo.toml"));
        c2.captures.insert(Arc::from("R"), Capture::new(Arc::from("org/beta")));

        let mut c3 = base_cursor("org/gamma", "v2", Some("other/Cargo.toml"));
        c3.captures.insert(Arc::from("R"), Capture::new(Arc::from("org/gamma")));

        let md = op.hover_capture("R", &[c1, c2, c3]).unwrap();

        assert!(md.contains("### `Cargo.toml`"), "missing file heading: {md}");
        assert!(md.contains("- `org/alpha`"), "missing alpha: {md}");
        assert!(md.contains("- `org/beta`"), "missing beta: {md}");
        assert!(md.contains("### `other/Cargo.toml`"), "missing other heading: {md}");
        assert!(md.contains("- `org/gamma`"), "missing gamma: {md}");
    }

    #[test]
    fn hover_match_flat_when_no_fs() {
        // Cursors without fs (repo-only context) → flat bullet list.
        let site = dummy_site();
        let op = RepoOp {
            mode: RepoMode::Filter(Arc::new(
                crate::_16_pattern::CompiledPattern::compile("org/*").unwrap()
            )),
            parse_site: site.clone(),
        };

        let mut c1 = base_cursor("org/alpha", "main", None);
        c1.evidence.push(OpEvidence {
            op_name:    "repo",
            parse_site: site.clone(),
            matched:    Arc::from("org/alpha"),
            capture:    None,
        });
        let mut c2 = base_cursor("org/beta", "main", None);
        c2.evidence.push(OpEvidence {
            op_name:    "repo",
            parse_site: site.clone(),
            matched:    Arc::from("org/beta"),
            capture:    None,
        });

        let md = op.hover_match(site.as_ref(), &[c1, c2]).unwrap();

        assert!(!md.contains("###"), "unexpected heading in flat mode: {md}");
        assert!(md.contains("- `org/alpha`"), "missing alpha: {md}");
        assert!(md.contains("- `org/beta`"), "missing beta: {md}");
    }
}
