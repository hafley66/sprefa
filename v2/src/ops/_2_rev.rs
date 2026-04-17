//! $rev — filter/bind on `cursor.rev`. Structurally identical to $repo.

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

pub struct RevFactory;

impl Operator for RevFactory {
    fn name(&self) -> &'static str { "rev" }
    fn paren_grammar(&self) -> GrammarRef { GrammarRef(Arc::from("rev-arg")) }
    fn brace_mode(&self) -> BraceMode { BraceMode::DefaultFork }

    fn scan_pointers(&self) -> &'static [ScanPointer] {
        static P: &[ScanPointer] = &[
            ScanPointer { sigil: "rev",      read: |c| Some(c.rev.clone()) },
            ScanPointer { sigil: "rev_norm", read: |c| Some(c.rev.clone()) },
        ];
        P
    }

    fn completion_item(&self) -> CompletionItem {
        CompletionItem {
            label:  "rev".to_string(),
            detail: "rev(glob | $CAP)".to_string(),
            doc:    "# rev\n\nFilter cursors by revision glob, or bind the revision to a capture.".to_string(),
        }
    }

    fn wildcard_instance(&self, parse_site: Arc<ParseSite>) -> Option<Arc<dyn Op>> {
        let pat = CompiledPattern::compile("*").ok()?;
        Some(Arc::new(RevOp {
            mode: RevMode::Filter(Arc::new(pat)),
            parse_site,
        }))
    }

    fn parse(&self, inv: &OpInvocation, _pctx: &mut ProgramCtx)
        -> Result<Pipeline, Vec<Box<dyn Diagnostic>>>
    {
        let paren = inv.paren_src.as_ref().ok_or_else(|| {
            vec![Box::new(RevDiag::MissingArg { site: (*inv.parse_site).clone() })
                as Box<dyn Diagnostic>]
        })?;
        let arg = paren.src.trim();
        let bad_arg = |site: &ParseSite| Box::new(RevDiag::BadArg {
            site: site.clone(),
            got:  Arc::from(arg),
        }) as Box<dyn Diagnostic>;
        let mode = match classify_token(arg) {
            TokenClass::Capture(c)  => RevMode::Bind(c.name),
            TokenClass::Literal     => {
                let pat = CompiledPattern::compile(arg)
                    .map_err(|_| vec![bad_arg(&inv.parse_site)])?;
                if matches!(pat.src.as_ref(), "*" | "**" | "**/*" | "*/*") {
                    return Err(vec![Box::new(RevDiag::UnboundedWildcard {
                        site:    (*inv.parse_site).clone(),
                        pattern: pat.src.clone(),
                    }) as Box<dyn Diagnostic>]);
                }
                RevMode::Filter(Arc::new(pat))
            }
            _ => return Err(vec![bad_arg(&inv.parse_site)]),
        };
        Ok(Pipeline::Op(Arc::new(RevOp { mode, parse_site: inv.parse_site.clone() }).into()))
    }
}

pub struct RevOp { mode: RevMode, parse_site: Arc<ParseSite> }
enum RevMode { Filter(Arc<CompiledPattern>), Bind(Arc<str>) }

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
            RevMode::Filter(p) => p.src.as_ref().to_string(),
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
            RevMode::Filter(p) => p.src.as_ref().to_string(),
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

    fn pipe(&self, input: BoxStream<'static, Arc<[Cursor]>>, _ctx: OpCtx)
        -> BoxStream<'static, Arc<[Cursor]>>
    {
        match &self.mode {
            RevMode::Filter(pat) => {
                let pat = pat.clone();
                input.map(move |batch| {
                    let kept: Vec<Cursor> = batch.iter()
                        .filter(|c| pat.is_match(&c.rev))
                        .cloned()
                        .collect();
                    Arc::<[Cursor]>::from(kept.into_boxed_slice())
                }).boxed()
            }
            RevMode::Bind(name) => {
                let name = name.clone();
                input.map(move |batch| {
                    let mapped: Vec<Cursor> = batch.iter().map(|c| {
                        let mut c = c.clone();
                        let v = c.rev.clone();
                        c.captures.insert(name.clone(), Capture::new(v).with_scan(Arc::from("rev"), Tri::Verified));
                        c
                    }).collect();
                    Arc::<[Cursor]>::from(mapped.into_boxed_slice())
                }).boxed()
            }
        }
    }
}

#[derive(Debug)]
enum RevDiag {
    MissingArg         { site: ParseSite },
    BadArg             { site: ParseSite, got: Arc<str> },
    UnboundedWildcard  { site: ParseSite, pattern: Arc<str> },
}

impl Diagnostic for RevDiag {
    fn code(&self) -> &str {
        match self {
            RevDiag::MissingArg        { .. } => "rev/missing-arg",
            RevDiag::BadArg            { .. } => "rev/bad-arg",
            RevDiag::UnboundedWildcard { .. } => "rev/unbounded-wildcard",
        }
    }
    fn severity(&self) -> crate::_0_types::Severity { crate::_0_types::Severity::Error }
    fn primary(&self) -> &ParseSite {
        match self {
            RevDiag::MissingArg        { site }    => site,
            RevDiag::BadArg            { site, .. } => site,
            RevDiag::UnboundedWildcard { site, .. } => site,
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
            RevDiag::UnboundedWildcard { site, pattern } => {
                out.header(self.code(), self.severity(),
                    &format!(
                        "rev pattern `{pattern}` matches every rev unconditionally. \
Each rev materializes a git worktree on first query. Narrow with a prefix/suffix \
so the set is bounded:\n    rev(v1.*)      # tags starting with v1.\n    \
rev(*-stable)  # tags ending in -stable\n    rev(main)      # literal"
                    ));
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
            xref_cartesian_limit: 10_000,
            max_passes:           8,
            max_claims_per_pass:  10_000,
            max_cursors_per_root: 1_000_000,
            },
            content_hash: 0,
        };
        ProgramCtx::new(Arc::new(config), Arc::new(OperatorRegistry::new()))
    }

    #[test]
    fn scan_pointers_expose_rev_and_rev_norm() {
        let f = RevFactory;
        let ps = Operator::scan_pointers(&f);
        let sigils: Vec<&str> = ps.iter().map(|p| p.sigil).collect();
        assert_eq!(sigils, vec!["rev", "rev_norm"]);
        let c = base_cursor("v1.2.3", None);
        assert_eq!(&*(ps[0].read)(&c).unwrap(), "v1.2.3");
        assert_eq!(&*(ps[1].read)(&c).unwrap(), "v1.2.3");
    }

    #[test]
    fn prefixed_wildcard_parses_as_filter() {
        let inv = OpInvocation {
            name:       Arc::from("rev"),
            brackets:   vec![],
            paren_src:  Some(ParenSlot { src: Arc::from("v1.*"), byte_range: 0..4 }),
            brace_src:  None,
            parse_site: dummy_site(),
            crossrefs:  vec![],
        };
        let result = RevFactory.parse(&inv, &mut dummy_pctx());
        assert!(result.is_ok(), "expected rev(v1.*) to parse — bounded prefix wildcard");
    }

    #[test]
    fn unbounded_wildcard_emits_diag_and_errors() {
        for bare in &["*", "**", "**/*", "*/*"] {
            let inv = OpInvocation {
                name:       Arc::from("rev"),
                brackets:   vec![],
                paren_src:  Some(ParenSlot { src: Arc::from(*bare), byte_range: 0..bare.len() }),
                brace_src:  None,
                parse_site: dummy_site(),
                crossrefs:  vec![],
            };
            let result = RevFactory.parse(&inv, &mut dummy_pctx());
            assert!(result.is_err(), "expected rev({bare}) to be rejected as unbounded");
            let diags = match result { Err(d) => d, Ok(_) => unreachable!() };
            assert_eq!(diags.len(), 1, "expected exactly one diag for `{bare}`");
            assert_eq!(diags[0].code(), "rev/unbounded-wildcard",
                "wrong diag code for pattern `{bare}`");
        }
    }

    // -----------------------------------------------------------------------
    // Hover grouping tests
    // -----------------------------------------------------------------------

    fn base_cursor(rev: &str, fs: Option<&str>) -> Cursor {
        use crate::_0_types::{FilePath, RunId};
        use std::path::Path;
        Cursor {
            run_id: RunId(0),
            repo:   Arc::from("org/repo"),
            rev:    Arc::from(rev),
            fs:     fs.map(|p| FilePath(Arc::from(Path::new(p)))),
            ..Default::default()
        }
    }

    #[test]
    fn hover_capture_groups_by_file_rev() {
        use crate::_0_types::Capture;
        let site = dummy_site();
        let op = RevOp { mode: RevMode::Bind(Arc::from("V")), parse_site: site.clone() };

        let mut c1 = base_cursor("main", Some("go.mod"));
        c1.captures.insert(Arc::from("V"), Capture::new(Arc::from("main")));

        let mut c2 = base_cursor("v2", Some("go.mod"));
        c2.captures.insert(Arc::from("V"), Capture::new(Arc::from("v2")));

        let mut c3 = base_cursor("main", Some("sub/go.mod"));
        c3.captures.insert(Arc::from("V"), Capture::new(Arc::from("main")));

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
        let op = RevOp {
            mode: RevMode::Filter(Arc::new(
                crate::_16_pattern::CompiledPattern::compile("main").unwrap()
            )),
            parse_site: site.clone(),
        };

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
