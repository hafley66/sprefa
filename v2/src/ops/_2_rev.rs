//! $rev — filter/bind on `cursor.rev`. Structurally identical to $repo.

use std::sync::Arc;

use futures_util::stream::StreamExt;
use futures_core::stream::BoxStream;

use crate::_0_types::{Capture, Cursor, ParseSite};
use crate::_1_diagnostic::{Diagnostic, Renderer};
use crate::_5_op::{
    BraceMode, CompletionItem, GrammarRef, Op, OpCtx, OpInvocation, Operator, Pipeline, ProgramCtx,
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
        let mut vals: Vec<&str> = Vec::new();
        for c in cursors {
            if let Some(capture) = c.captures.get(cap) {
                let v = capture.value.as_ref();
                if !vals.contains(&v) {
                    vals.push(v);
                    if vals.len() >= 20 { break; }
                }
            }
        }
        if vals.is_empty() { return None; }
        let lines: Vec<String> = vals.iter().map(|v| format!("- `{}`", v)).collect();
        Some(format!("**`${cap}`** revisions:\n\n{}", lines.join("\n")))
    }

    fn hover_match(&self, site: &crate::_0_types::ParseSite, cursors: &[Cursor]) -> Option<String> {
        let glob = match &self.mode {
            RevMode::Filter(g) => g.as_ref().to_string(),
            RevMode::Bind(n)   => format!("${}", n),
        };
        let mut vals: Vec<&str> = Vec::new();
        for c in cursors {
            let touched = c.evidence.iter().any(|ev|
                ev.op_name == "rev" && ev.parse_site.as_ref() == site
            );
            if !touched { continue; }
            let v = c.rev.as_ref();
            if !vals.contains(&v) {
                vals.push(v);
                if vals.len() >= 20 { break; }
            }
        }
        if vals.is_empty() {
            return Some(format!("**rev** glob: `{glob}`\n\n(no matches yet)"));
        }
        let lines: Vec<String> = vals.iter().map(|v| format!("- `{}`", v)).collect();
        Some(format!("**rev** glob: `{glob}`\n\n{} matches:\n\n{}", vals.len(), lines.join("\n")))
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
}
