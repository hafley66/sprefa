//! $rev — filter/bind on `cursor.rev`. Structurally identical to $repo.

use std::sync::Arc;

use futures_util::stream::StreamExt;
use futures_core::stream::BoxStream;

use crate::_0_types::{Capture, Cursor, ParseSite};
use crate::_1_diagnostic::{Diagnostic, Renderer};
use crate::_5_op::{
    hover_render_grouped, BraceMode, CompletionItem, GrammarRef, Op, OpCtx, OpInvocation,
    Operator, Pipeline, ProgramCtx,
};
use crate::_8_parse::{classify_token, glob_match, TokenClass};

pub struct RevFactory;

impl Operator for RevFactory {
    fn name(&self) -> &'static str { "rev" }
    fn paren_grammar(&self) -> GrammarRef { GrammarRef(Arc::from("rev-arg")) }
    fn brace_mode(&self) -> BraceMode { BraceMode::DefaultFork }

    fn completion_item(&self) -> CompletionItem {
        CompletionItem {
            label:  "rev".to_string(),
            detail: "rev(glob | $CAP)".to_string(),
            doc:    "# rev\n\nFilter cursors by revision glob, or bind the revision to a capture.".to_string(),
        }
    }

    fn parse(&self, inv: &OpInvocation, _pctx: &mut ProgramCtx)
        -> Result<Pipeline, Vec<Box<dyn Diagnostic>>>
    {
        let paren = inv.paren_src.as_ref().ok_or_else(|| {
            vec![Box::new(RevDiag::MissingArg { site: (*inv.parse_site).clone() })
                as Box<dyn Diagnostic>]
        })?;
        let arg = paren.src.trim();
        let mode = match classify_token(arg) {
            TokenClass::Capture(c)  => RevMode::Bind(c.name),
            TokenClass::Literal if arg == "*" => {
                return Err(vec![Box::new(RevDiag::NoOp {
                    site: (*inv.parse_site).clone(),
                }) as _]);
            }
            TokenClass::Literal     => RevMode::Filter(Arc::from(arg)),
            _ => return Err(vec![Box::new(RevDiag::BadArg {
                site: (*inv.parse_site).clone(),
                got:  Arc::from(arg),
            }) as _]),
        };
        Ok(Pipeline::Op(Arc::new(RevOp { mode, parse_site: inv.parse_site.clone() })))
    }
}

pub struct RevOp { mode: RevMode, parse_site: Arc<ParseSite> }
enum RevMode { Filter(Arc<str>), Bind(Arc<str>) }

impl Op for RevOp {
    fn name(&self) -> &'static str { "rev" }
    fn step(&self) -> u16 { 0 }
    fn parse_site(&self) -> &Arc<ParseSite> { &self.parse_site }

    fn witness(&self, c: &Cursor) -> Option<Arc<str>> { Some(c.rev.clone()) }

    fn capture_name(&self) -> Option<Arc<str>> {
        match &self.mode { RevMode::Bind(n) => Some(n.clone()), _ => None }
    }

    fn hover_self(&self) -> String {
        let src = match &self.mode {
            RevMode::Filter(g) => g.as_ref().to_string(),
            RevMode::Bind(n)   => format!("${}", n),
        };
        format!("# rev({})", src)
    }

    fn hover_capture(&self, cap: &str, cursors: &[Cursor]) -> Option<String> {
        let header = format!("**`${cap}`** revisions:");
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
        let glob = match &self.mode {
            RevMode::Filter(g) => g.as_ref().to_string(),
            RevMode::Bind(n)   => format!("${}", n),
        };
        let header = format!("**rev** glob: `{glob}`");
        let entries: Vec<(Option<String>, String, String)> = cursors.iter()
            .filter(|c| c.evidence.iter().any(|ev|
                ev.op_name == "rev" && ev.parse_site.as_ref() == site
            ))
            .map(|c| (
                c.fs.as_ref().map(|fp| fp.0.to_string_lossy().into_owned()),
                c.rev.to_string(),
                c.rev.to_string(),
            ))
            .collect();
        if entries.is_empty() {
            return Some(format!("{header}\n\n(no matches yet)"));
        }
        hover_render_grouped(&header, &entries)
    }

    fn pipe(&self, input: BoxStream<'static, Cursor>, _ctx: OpCtx)
        -> BoxStream<'static, Cursor>
    {
        match &self.mode {
            RevMode::Filter(glob) => {
                let g = glob.clone();
                input.filter(move |c| { let k = glob_match(&g, &c.rev); async move { k } }).boxed()
            }
            RevMode::Bind(name) => {
                let name = name.clone();
                input.map(move |mut c| {
                    let v = c.rev.clone();
                    c.captures.insert(name.clone(), Capture { value: v, ref_id: None });
                    c
                }).boxed()
            }
        }
    }
}

#[derive(Debug)]
enum RevDiag {
    MissingArg { site: ParseSite },
    BadArg     { site: ParseSite, got: Arc<str> },
    NoOp       { site: ParseSite },
}

impl Diagnostic for RevDiag {
    fn code(&self) -> &str {
        match self {
            RevDiag::MissingArg { .. } => "rev/missing-arg",
            RevDiag::BadArg     { .. } => "rev/bad-arg",
            RevDiag::NoOp       { .. } => "rev/no-op-glob",
        }
    }
    fn severity(&self) -> crate::_0_types::Severity { crate::_0_types::Severity::Error }
    fn primary(&self) -> &ParseSite {
        match self {
            RevDiag::MissingArg { site }    => site,
            RevDiag::BadArg     { site, .. } => site,
            RevDiag::NoOp       { site }    => site,
        }
    }
    fn render(&self, out: &mut dyn Renderer) {
        match self {
            RevDiag::MissingArg { site } => {
                out.header(self.code(), self.severity(),
                    "rev requires a glob pattern or capture (e.g. rev(main) or rev($V))");
                out.primary(site);
            }
            RevDiag::BadArg { site, got } => {
                out.header(self.code(), self.severity(),
                    &format!("rev argument `{got}` is not a glob or capture"));
                out.primary(site);
            }
            RevDiag::NoOp { site } => {
                out.header(self.code(), self.severity(),
                    "rev(*) matches every revision — it's a no-op. Use rev(HEAD) to mean \"current revision\" (resolves to current branch name in reports).");
                out.primary(site);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use crate::_5_op::ParenSlot;
    use crate::_10_registry::OperatorRegistry;

    fn dummy_site() -> Arc<ParseSite> {
        Arc::new(ParseSite {
            file:       Arc::from(Path::new("test.sprf")),
            path:       Arc::from(vec![].into_boxed_slice()),
            byte_range: 0..1,
        })
    }

    fn dummy_pctx() -> ProgramCtx {
        use crate::_2_config::{Config, RuntimeConfig};
        let config = Config {
            repos:        vec![],
            revs:         vec![],
            fs_exclude:   vec![],
            sprf_files:   vec![],
            shell_allow:  vec![],
            runtime:      RuntimeConfig {
                worker_threads:    1,
                buffer_size:       64,
                flush_interval_ms: 100,
                collect_witnesses: false,
            },
            content_hash: 0,
        };
        ProgramCtx {
            rules:     Default::default(),
            constants: Default::default(),
            config:    Arc::new(config),
            registry:  Arc::new(OperatorRegistry::new()),
        }
    }

    #[test]
    fn star_glob_is_no_op_diag() {
        let inv = OpInvocation {
            name:       Arc::from("rev"),
            brackets:   vec![],
            paren_src:  Some(ParenSlot { src: Arc::from("*"), byte_range: 0..1 }),
            brace_src:  None,
            parse_site: dummy_site(),
        };
        let result = RevFactory.parse(&inv, &mut dummy_pctx());
        assert!(result.is_err(), "expected parse to fail for rev(*)");
        let err = result.err().unwrap();
        assert_eq!(err.len(), 1);
        assert_eq!(err[0].code(), "rev/no-op-glob");
    }

    // -----------------------------------------------------------------------
    // Hover grouping tests
    // -----------------------------------------------------------------------

    fn base_cursor(rev: &str, fs: Option<&str>) -> Cursor {
        use crate::_0_types::{FilePath, RunId, SprfPath};
        use std::path::Path;
        Cursor {
            run_id:   RunId(0),
            repo:     Arc::from("org/repo"),
            rev:      Arc::from(rev),
            fs:       fs.map(|p| FilePath(Arc::from(Path::new(p)))),
            captures: Default::default(),
            fks:      Default::default(),
            path:     SprfPath(Arc::from(vec![].into_boxed_slice())),
            evidence: vec![],
            content:  None,
        }
    }

    #[test]
    fn hover_capture_groups_by_file_rev() {
        use crate::_0_types::Capture;
        let site = dummy_site();
        let op = RevOp { mode: RevMode::Bind(Arc::from("V")), parse_site: site.clone() };

        let mut c1 = base_cursor("main", Some("go.mod"));
        c1.captures.insert(Arc::from("V"), Capture { value: Arc::from("main"), ref_id: None });

        let mut c2 = base_cursor("v2", Some("go.mod"));
        c2.captures.insert(Arc::from("V"), Capture { value: Arc::from("v2"), ref_id: None });

        let mut c3 = base_cursor("main", Some("sub/go.mod"));
        c3.captures.insert(Arc::from("V"), Capture { value: Arc::from("main"), ref_id: None });

        let md = op.hover_capture("V", &[c1, c2, c3]).unwrap();

        assert!(md.contains("### `go.mod`"), "missing go.mod heading: {md}");
        assert!(md.contains("- `main`"), "missing main: {md}");
        assert!(md.contains("- `v2`"), "missing v2: {md}");
        assert!(md.contains("### `sub/go.mod`"), "missing sub/go.mod heading: {md}");
    }

    #[test]
    fn hover_match_flat_when_no_fs() {
        use crate::_0_types::OpEvidence;
        let site = dummy_site();
        let op = RevOp { mode: RevMode::Filter(Arc::from("main")), parse_site: site.clone() };

        let mut c1 = base_cursor("main", None);
        c1.evidence.push(OpEvidence {
            op_name:    "rev",
            parse_site: site.clone(),
            matched:    Arc::from("main"),
            capture:    None,
        });
        let mut c2 = base_cursor("develop", None);
        c2.evidence.push(OpEvidence {
            op_name:    "rev",
            parse_site: site.clone(),
            matched:    Arc::from("develop"),
            capture:    None,
        });

        let md = op.hover_match(site.as_ref(), &[c1, c2]).unwrap();

        assert!(!md.contains("###"), "unexpected heading in flat mode: {md}");
        assert!(md.contains("- `main`"), "missing main: {md}");
        assert!(md.contains("- `develop`"), "missing develop: {md}");
    }
}
