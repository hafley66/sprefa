//! fs — file-path enumeration across (repo, rev). Not a filesystem listing.
//!
//! Paths come from `Reader::files(repo, rev, pattern)`. For the real git2-
//! backed Reader this means walking the rev's commit tree (blobs, not the
//! working directory). MemReader is a static stand-in with the same shape.
//!
//! Paren slot:
//!   - literal glob (e.g. `src/**/*.rs`) → Filter: pattern passed to Reader
//!   - capture     (e.g. `$F`)           → Bind: pattern = `**`, bind each
//!                                          matched path into `F`
//!
//! Fan-out: one input cursor × N matched files → N output cursors, each
//! with `cursor.fs = Some(fp)`. Empty match drops the input cursor (zero-
//! match diagnostic is a §12 future; not emitted here).

use std::sync::Arc;

use futures_core::stream::BoxStream;
use futures_util::stream::{self, StreamExt};

use crate::_0_types::{Capture, Cursor, FilePath, ParseSite};
use crate::_1_diagnostic::{Diagnostic, Renderer};
use crate::_5_op::{
    hover_render_grouped, BraceMode, CompletionItem, GrammarRef, Op, OpCtx, OpInvocation,
    Operator, Pipeline, ProgramCtx,
};
use crate::_8_parse::{classify_token, TokenClass};

// ---------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------

pub struct FsFactory;

impl Operator for FsFactory {
    fn name(&self) -> &'static str { "fs" }
    fn paren_grammar(&self) -> GrammarRef { GrammarRef(Arc::from("fs-arg")) }
    fn brace_mode(&self) -> BraceMode { BraceMode::DefaultFork }

    fn completion_item(&self) -> CompletionItem {
        CompletionItem {
            label:  "fs".to_string(),
            detail: "fs(glob | $CAP)".to_string(),
            doc:    "# fs\n\nFan out cursors over files matching a glob, or bind each matched path to a capture.".to_string(),
        }
    }

    fn parse(&self, inv: &OpInvocation, _pctx: &mut ProgramCtx)
        -> Result<Pipeline, Vec<Box<dyn Diagnostic>>>
    {
        let paren = inv.paren_src.as_ref().ok_or_else(|| {
            vec![Box::new(FsDiag::MissingArg { site: (*inv.parse_site).clone() })
                as Box<dyn Diagnostic>]
        })?;
        let arg = paren.src.trim();
        let mode = match classify_token(arg) {
            TokenClass::Capture(c)  => FsMode::Bind(c.name, Arc::from("**")),
            TokenClass::Literal     => FsMode::Filter(Arc::from(arg)),
            _ => return Err(vec![Box::new(FsDiag::BadArg {
                site: (*inv.parse_site).clone(),
                got:  Arc::from(arg),
            }) as _]),
        };
        Ok(Pipeline::Op(Arc::new(FsOp { mode, parse_site: inv.parse_site.clone() }).into()))
    }
}

// ---------------------------------------------------------------------------
// Op
// ---------------------------------------------------------------------------

pub struct FsOp {
    mode:       FsMode,
    parse_site: Arc<ParseSite>,
}

enum FsMode {
    Filter(Arc<str>),             // glob pattern handed to Reader.files
    Bind(Arc<str>, Arc<str>),     // (capture name, pattern = "**")
}

impl FsMode {
    fn pattern(&self) -> Arc<str> {
        match self {
            FsMode::Filter(p) | FsMode::Bind(_, p) => p.clone(),
        }
    }
    fn bind_name(&self) -> Option<Arc<str>> {
        match self { FsMode::Bind(n, _) => Some(n.clone()), _ => None }
    }
}

fn fs_path_md(path_str: &str) -> String {
    // Paths are repo-relative; we don't know the git-root in op scope, so
    // render as inline code. VS Code's path detector still offers cmd+click
    // inside the workspace.
    format!("- `{}`", path_str)
}

impl Op for FsOp {
    fn name(&self) -> &'static str { "fs" }
    fn step(&self) -> u16 { 0 }
    fn parse_site(&self) -> &Arc<ParseSite> { &self.parse_site }

    fn witness(&self, c: &Cursor) -> Option<Arc<str>> {
        c.fs.as_ref().map(|fp| Arc::from(fp.0.to_string_lossy().as_ref()))
    }

    fn capture_name(&self) -> Option<Arc<str>> { self.mode.bind_name() }

    fn hover_self(&self) -> String {
        let src = match &self.mode {
            FsMode::Filter(p)    => p.as_ref().to_string(),
            FsMode::Bind(n, _)   => format!("${}", n),
        };
        format!("# fs({})", src)
    }

    fn hover_capture(&self, cap: &str, cursors: &[Cursor]) -> Option<String> {
        let header = format!("**`${cap}`** paths:");
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
        let pattern = self.mode.pattern();
        let header = format!("**fs** glob: `{pattern}`");
        let entries: Vec<(Option<String>, String, String)> = cursors.iter()
            .filter(|c| c.evidence.iter().any(|ev|
                ev.op_name == "fs" && ev.parse_site.as_ref() == site
            ))
            .filter_map(|c| {
                c.fs.as_ref().map(|fp| (
                    Some(fp.0.to_string_lossy().into_owned()),
                    c.rev.to_string(),
                    fp.0.to_string_lossy().into_owned(),
                ))
            })
            .collect();
        if entries.is_empty() {
            return Some(format!("{header}\n\n(no matches yet)"));
        }
        hover_render_grouped(&header, &entries)
    }

    fn pipe(&self, input: BoxStream<'static, Cursor>, ctx: OpCtx)
        -> BoxStream<'static, Cursor>
    {
        let reader     = ctx.reader.clone();
        let pattern    = self.mode.pattern();
        let bind_name  = self.mode.bind_name();
        let parse_site = self.parse_site.clone();
        let diags      = ctx.diags.clone();

        input.then(move |c| {
            let reader     = reader.clone();
            let pattern    = pattern.clone();
            let bind_name  = bind_name.clone();
            let parse_site = parse_site.clone();
            let diags      = diags.clone();
            async move {
                let mut s = reader.files(&c.repo, &c.rev, &pattern);
                let files: Vec<FilePath> = s.next().await.unwrap_or_default();
                if files.is_empty() {
                    diags.0(Box::new(FsDiag::NoMatch {
                        site:    (*parse_site).clone(),
                        pattern: pattern.clone(),
                        repo:    c.repo.clone(),
                        rev:     c.rev.clone(),
                    }));
                    return stream::iter(vec![]);
                }
                let items: Vec<Cursor> = files.into_iter().map(|fp| {
                    let mut c2 = c.clone();
                    let v: Arc<str> = Arc::from(fp.0.to_string_lossy().as_ref());
                    c2.fs = Some(fp);
                    if let Some(name) = bind_name.as_ref() {
                        c2.captures.insert(name.clone(), Capture { value: v, ref_id: None });
                    }
                    c2
                }).collect();
                stream::iter(items)
            }
        }).flatten().boxed()
    }
}

// ---------------------------------------------------------------------------
// Diagnostics
// ---------------------------------------------------------------------------

#[derive(Debug)]
enum FsDiag {
    MissingArg { site: ParseSite },
    BadArg     { site: ParseSite, got: Arc<str> },
    NoMatch    { site: ParseSite, pattern: Arc<str>, repo: Arc<str>, rev: Arc<str> },
}

impl Diagnostic for FsDiag {
    fn code(&self) -> &str {
        match self {
            FsDiag::MissingArg { .. } => "fs/missing-arg",
            FsDiag::BadArg     { .. } => "fs/bad-arg",
            FsDiag::NoMatch    { .. } => "fs/no-match",
        }
    }
    fn severity(&self) -> crate::_0_types::Severity {
        match self {
            FsDiag::NoMatch { .. } => crate::_0_types::Severity::Warn,
            _                      => crate::_0_types::Severity::Error,
        }
    }
    fn primary(&self) -> &ParseSite {
        match self {
            FsDiag::MissingArg { site }        => site,
            FsDiag::BadArg     { site, .. }    => site,
            FsDiag::NoMatch    { site, .. }    => site,
        }
    }
    fn render(&self, out: &mut dyn Renderer) {
        match self {
            FsDiag::MissingArg { site } => {
                out.header(self.code(), self.severity(),
                    "fs requires a glob pattern or capture (e.g. fs(src/**/*.rs) or fs($F))");
                out.primary(site);
            }
            FsDiag::BadArg { site, got } => {
                out.header(self.code(), self.severity(),
                    &format!("fs argument `{got}` is not a glob or capture"));
                out.primary(site);
            }
            FsDiag::NoMatch { site, pattern, repo, rev } => {
                out.header(self.code(), self.severity(),
                    &format!("fs glob `{pattern}` matched no files in {repo}@{rev}"));
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
    use std::sync::Mutex;
    use futures_util::stream;
    use crate::_0_types::{OpId, RunId};
    use crate::_5_op::{DiagSink, EventSink, OpCtx};
    use crate::readers::MemReader;
    use crate::writers::MemWriter;
    use crate::{Config, RuntimeConfig};

    fn dummy_site() -> Arc<ParseSite> {
        Arc::new(ParseSite {
            file:       Arc::from(Path::new("test.sprf")),
            path:       Arc::from(vec![].into_boxed_slice()),
            byte_range: 0..1,
        })
    }

    fn make_config() -> Arc<Config> {
        Arc::new(Config {
            repos: vec![], revs: vec![], fs_exclude: vec![],
            sprf_files: vec![], shell_allow: vec![],
            runtime: RuntimeConfig {
                worker_threads: 1, buffer_size: 64,
                flush_interval_ms: 100, collect_witnesses: false,
            xref_cartesian_limit: 10_000,
            },
            content_hash: 0,
        })
    }

    #[tokio::test]
    async fn no_match_emits_warn_diag() {
        let config  = make_config();
        // MemReader with the repo/rev registered but no files — glob will match nothing.
        let reader: Arc<dyn crate::_3_reader::Reader + Send + Sync> = Arc::new(
            MemReader::new(config.clone()).with_repo("myrepo", &["HEAD"])
        );
        let writer: Arc<dyn crate::_4_writer::Writer + Send + Sync> =
            Arc::new(MemWriter::new());

        let fired: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(vec![]));
        let fired2 = fired.clone();
        let diags = DiagSink(Arc::new(move |d: Box<dyn crate::_1_diagnostic::Diagnostic>| {
            fired2.lock().unwrap().push(d.code().to_string());
        }));
        let events = EventSink(Arc::new(|_| {}));

        let (result_store, xref_seen) = OpCtx::fresh_xref_state();
        let ctx = OpCtx {
            run_id: RunId(0),
            op_id:  OpId(0),
            reader,
            writer,
            config,
            diags,
            events,
            result_store,
            xref_seen,
        };

        let op = FsOp {
            mode:       FsMode::Filter(Arc::from("src/**/*.rs")),
            parse_site: dummy_site(),
        };

        let cursor = Cursor {
            run_id:   RunId(0),
            path:     crate::_0_types::SprfPath(Arc::from(vec![].into_boxed_slice())),
            repo:     Arc::from("myrepo"),
            rev:      Arc::from("HEAD"),
            fs:       None,
            captures: Default::default(),
            fks:      Default::default(),
            evidence: vec![],
            content:  None,
        };

        let input: BoxStream<'static, Cursor> = Box::pin(stream::iter(vec![cursor]));
        let mut out = op.pipe(input, ctx);
        use futures_util::StreamExt;
        while out.next().await.is_some() {}

        let codes = fired.lock().unwrap().clone();
        assert_eq!(codes, vec!["fs/no-match"], "expected exactly one fs/no-match diag, got {codes:?}");
    }

    // -----------------------------------------------------------------------
    // Hover grouping tests
    // -----------------------------------------------------------------------

    fn base_cursor_hover(rev: &str, fs: Option<&str>) -> Cursor {
        Cursor {
            run_id:   crate::_0_types::RunId(0),
            repo:     Arc::from("org/repo"),
            rev:      Arc::from(rev),
            fs:       fs.map(|p| crate::_0_types::FilePath(Arc::from(Path::new(p)))),
            captures: Default::default(),
            fks:      Default::default(),
            path:     crate::_0_types::SprfPath(Arc::from(vec![].into_boxed_slice())),
            evidence: vec![],
            content:  None,
        }
    }

    #[test]
    fn hover_match_groups_by_file_rev() {
        use crate::_0_types::OpEvidence;
        let site = dummy_site();
        let op = FsOp {
            mode:       FsMode::Filter(Arc::from("**/Cargo.toml")),
            parse_site: site.clone(),
        };

        let mut c1 = base_cursor_hover("main", Some("crates/a/Cargo.toml"));
        c1.evidence.push(OpEvidence {
            op_name:    "fs",
            parse_site: site.clone(),
            matched:    Arc::from("crates/a/Cargo.toml"),
            capture:    None,
        });
        let mut c2 = base_cursor_hover("main", Some("crates/b/Cargo.toml"));
        c2.evidence.push(OpEvidence {
            op_name:    "fs",
            parse_site: site.clone(),
            matched:    Arc::from("crates/b/Cargo.toml"),
            capture:    None,
        });
        let mut c3 = base_cursor_hover("v2", Some("crates/a/Cargo.toml"));
        c3.evidence.push(OpEvidence {
            op_name:    "fs",
            parse_site: site.clone(),
            matched:    Arc::from("crates/a/Cargo.toml"),
            capture:    None,
        });

        let md = op.hover_match(site.as_ref(), &[c1, c2, c3]).unwrap();

        assert!(md.contains("### `crates/a/Cargo.toml`"), "missing a/Cargo heading: {md}");
        assert!(md.contains("### `crates/b/Cargo.toml`"), "missing b/Cargo heading: {md}");
        // c3 is rev=v2 so the rev suffix must appear
        assert!(md.contains("(rev: v2)") || md.contains("(rev: main)"),
            "expected rev suffix for multi-rev groups: {md}");
    }
}
