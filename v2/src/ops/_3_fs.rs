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

use futures::future::join_all;
use futures_core::stream::BoxStream;
use futures_util::stream::{self, StreamExt};
use tracing::{info_span, Instrument};

use crate::_17_parallel::rayon_map;

use crate::_0_types::{Capture, Cursor, FilePath, ParseSite, Tri};
use crate::_16_pattern::CompiledPattern;
use crate::_1_diagnostic::{Diagnostic, Renderer};
use crate::_5_op::{
    hover_render_grouped, BraceMode, CompletionItem, GrammarRef, Op, OpCtx, OpInvocation, Operator,
    Pipeline, ProgramCtx, ScanPointer,
};
use crate::_8_parse::{classify_token, TokenClass};

// ---------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------

pub struct FsFactory;

impl Operator for FsFactory {
    fn name(&self) -> &'static str {
        "fs"
    }
    fn paren_grammar(&self) -> GrammarRef {
        GrammarRef(Arc::from("fs-arg"))
    }
    fn brace_mode(&self) -> BraceMode {
        BraceMode::DefaultFork
    }

    fn scan_pointers(&self) -> &'static [ScanPointer] {
        static P: &[ScanPointer] = &[
            ScanPointer {
                sigil: "fs",
                read: |c| {
                    c.fs.as_ref()
                        .map(|fp| Arc::from(fp.0.to_string_lossy().as_ref()))
                },
            },
            ScanPointer {
                sigil: "fs_norm",
                read: |c| {
                    c.fs.as_ref()
                        .map(|fp| Arc::from(fp.0.to_string_lossy().as_ref()))
                },
            },
        ];
        P
    }

    fn completion_item(&self) -> CompletionItem {
        CompletionItem {
            label:  "fs".to_string(),
            detail: "fs(glob | $CAP)".to_string(),
            doc:    "# fs\n\nFan out cursors over files matching a glob, or bind each matched path to a capture.".to_string(),
        }
    }

    fn wildcard_instance(&self, parse_site: Arc<ParseSite>) -> Option<Arc<dyn Op>> {
        let pat = CompiledPattern::compile("**").expect("'**' is a valid glob");
        Some(Arc::new(FsOp {
            mode: FsMode::Filter(Arc::new(pat)),
            parse_site,
        }))
    }

    fn parse(
        &self,
        inv: &OpInvocation,
        _pctx: &mut ProgramCtx,
    ) -> Result<Pipeline, Vec<Box<dyn Diagnostic>>> {
        let paren = inv.paren_src.as_ref().ok_or_else(|| {
            vec![Box::new(FsDiag::MissingArg {
                site: (*inv.parse_site).clone(),
            }) as Box<dyn Diagnostic>]
        })?;
        let arg = paren.src.trim();
        let mode = match classify_token(arg) {
            TokenClass::Capture(c) => {
                let pat = CompiledPattern::compile("**").expect("'**' is a valid glob");
                FsMode::Bind(c.name, Arc::new(pat))
            }
            TokenClass::Literal => match CompiledPattern::compile(arg) {
                Ok(pat) => FsMode::Filter(Arc::new(pat)),
                Err(_) => {
                    return Err(vec![Box::new(FsDiag::BadArg {
                        site: (*inv.parse_site).clone(),
                        got: Arc::from(arg),
                    }) as _])
                }
            },
            _ => {
                return Err(vec![Box::new(FsDiag::BadArg {
                    site: (*inv.parse_site).clone(),
                    got: Arc::from(arg),
                }) as _])
            }
        };
        Ok(Pipeline::Op(
            Arc::new(FsOp {
                mode,
                parse_site: inv.parse_site.clone(),
            })
            .into(),
        ))
    }
}

// ---------------------------------------------------------------------------
// Op
// ---------------------------------------------------------------------------

pub struct FsOp {
    mode: FsMode,
    parse_site: Arc<ParseSite>,
}

enum FsMode {
    Filter(Arc<CompiledPattern>),         // pattern handed to Reader.files
    Bind(Arc<str>, Arc<CompiledPattern>), // (capture name, pattern = "**")
}

impl FsMode {
    fn pattern(&self) -> Arc<CompiledPattern> {
        match self {
            FsMode::Filter(p) | FsMode::Bind(_, p) => p.clone(),
        }
    }
    fn bind_name(&self) -> Option<Arc<str>> {
        match self {
            FsMode::Bind(n, _) => Some(n.clone()),
            _ => None,
        }
    }
}

fn fs_path_md(path_str: &str) -> String {
    // Paths are repo-relative; we don't know the git-root in op scope, so
    // render as inline code. VS Code's path detector still offers cmd+click
    // inside the workspace.
    format!("- `{}`", path_str)
}

impl Op for FsOp {
    fn name(&self) -> &'static str {
        "fs"
    }
    fn step(&self) -> u16 {
        0
    }
    fn parse_site(&self) -> &Arc<ParseSite> {
        &self.parse_site
    }

    fn witness(&self, c: &Cursor) -> Option<Arc<str>> {
        c.fs.as_ref()
            .map(|fp| Arc::from(fp.0.to_string_lossy().as_ref()))
    }

    fn capture_name(&self) -> Option<Arc<str>> {
        self.mode.bind_name()
    }

    fn hover_self(&self) -> String {
        let src = match &self.mode {
            FsMode::Filter(p) => p.src.as_ref().to_string(),
            FsMode::Bind(n, _) => format!("${}", n),
        };
        format!("# fs({})", src)
    }

    fn hover_capture(&self, cap: &str, cursors: &[Cursor]) -> Option<String> {
        let header = format!("**`${cap}`** paths:");
        let entries: Vec<(Option<String>, String, String)> = cursors
            .iter()
            .filter_map(|c| {
                c.captures.get(cap).map(|capture| {
                    (
                        c.fs.as_ref().map(|fp| fp.0.to_string_lossy().into_owned()),
                        c.rev.to_string(),
                        capture.value.to_string(),
                    )
                })
            })
            .collect();
        hover_render_grouped(&header, &entries)
    }

    fn hover_match(&self, site: &crate::_0_types::ParseSite, cursors: &[Cursor]) -> Option<String> {
        let pattern = self.mode.pattern();
        let header = format!("**fs** glob: `{}`", pattern.src);
        // Flat list: one line per matched path. For fs the path IS the content;
        // nesting it as both heading and bullet reads as a duplicate.
        let mut seen: Vec<String> = Vec::new();
        let mut lines: Vec<String> = Vec::new();
        let single_rev = {
            let revs: std::collections::HashSet<Arc<str>> = cursors
                .iter()
                .filter(|c| {
                    c.evidence
                        .iter()
                        .any(|ev| ev.op_name == "fs" && ev.parse_site.as_ref() == site)
                })
                .map(|c| c.rev.clone())
                .collect();
            revs.len() == 1
        };
        for c in cursors {
            if !c
                .evidence
                .iter()
                .any(|ev| ev.op_name == "fs" && ev.parse_site.as_ref() == site)
            {
                continue;
            }
            let Some(fp) = c.fs.as_ref() else { continue };
            let path = fp.0.to_string_lossy().into_owned();
            if seen.iter().any(|s| s == &path) {
                continue;
            }
            seen.push(path.clone());
            let line = if single_rev {
                format!("- {}", crate::_5_op::path_link_or_code(&path))
            } else {
                format!(
                    "- {} (rev: {})",
                    crate::_5_op::path_link_or_code(&path),
                    c.rev
                )
            };
            lines.push(line);
            if lines.len() >= 20 {
                break;
            }
        }
        if lines.is_empty() {
            return Some(format!("{header}\n\n(no matches yet)"));
        }
        Some(format!("{}\n\n{}", header, lines.join("\n")))
    }

    fn pipe(
        &self,
        input: BoxStream<'static, Arc<[Cursor]>>,
        ctx: OpCtx,
    ) -> BoxStream<'static, Arc<[Cursor]>> {
        let _pipe_span = info_span!("op.fs.pipe").entered();
        let reader = ctx.reader.clone();
        let pattern = self.mode.pattern();
        let bind_name = self.mode.bind_name();
        let parse_site = self.parse_site.clone();
        let diags = ctx.diags.clone();

        // Fan-out shape: each input cursor produces its own output batch of
        // matched paths. An input batch of N cursors flows out as N output
        // batches, independent of the upstream grouping. Empty matches emit
        // `Arc<[]>` AND the fs/no-match diagnostic, mirroring prior per-cursor
        // semantics — downstream sees the empty batch to drive aggregation.
        //
        // Per batch:
        //   Phase 1 (async) — join_all reader.files() across the batch. Under the
        //     hood GitBlobReader grabs a per-repo mutex and tree-walks; concurrent
        //     cursors on distinct (repo, rev) pairs can proceed in parallel.
        //   Phase 2 (rayon) — par_iter expands (cursor, files) pairs into output
        //     cursors. Cursor clone + Arc<str> coercion + HashMap insert. oneshot
        //     bridges back into async.
        input
            .flat_map(move |batch| {
                let batch_span = info_span!("op.fs.batch", n = batch.len());
                let items: Vec<Cursor> = batch.iter().cloned().collect();
                let reader = reader.clone();
                let pattern = pattern.clone();
                let bind_name = bind_name.clone();
                let parse_site = parse_site.clone();
                let diags = diags.clone();
                let fut = async move {
                    // Phase 1 — concurrent I/O fan-out
                    let pairs: Vec<(Cursor, Vec<FilePath>)> =
                        join_all(items.into_iter().map(|c| {
                            let reader = reader.clone();
                            let pattern = pattern.clone();
                            async move {
                                let mut s = reader.files(&c.repo, &c.rev, &pattern);
                                let files = s.next().await.unwrap_or_default();
                                (c, files)
                            }
                        }))
                        .await;

                    // Phase 2 — CPU expansion on rayon pool
                    let bind_name_r = bind_name.clone();
                    let results: Vec<(Cursor, Vec<Cursor>)> =
                        rayon_map(pairs, move |(c, files)| {
                            let expanded: Vec<Cursor> = files
                                .into_iter()
                                .map(|fp| {
                                    let mut c2 = c.clone();
                                    let v: Arc<str> = Arc::from(fp.0.to_string_lossy().as_ref());
                                    c2.fs = Some(fp);
                                    if let Some(name) = bind_name_r.as_ref() {
                                        c2.captures.insert(
                                            name.clone(),
                                            Capture::new(v)
                                                .with_scan(Arc::from("fs"), Tri::Verified),
                                        );
                                    }
                                    c2
                                })
                                .collect();
                            (c, expanded)
                        })
                        .await;

                    // Diags on the async side; emit one Arc<[Cursor]> per input cursor.
                    let batches: Vec<Arc<[Cursor]>> = results
                        .into_iter()
                        .map(|(c, expanded)| {
                            if expanded.is_empty() {
                                diags.0(Box::new(FsDiag::NoMatch {
                                    site: (*parse_site).clone(),
                                    pattern: pattern.src.clone(),
                                    repo: c.repo.clone(),
                                    rev: c.rev.clone(),
                                }));
                            }
                            Arc::<[Cursor]>::from(expanded.into_boxed_slice())
                        })
                        .collect();

                    stream::iter(batches)
                }
                .instrument(batch_span);
                stream::once(fut).flatten()
            })
            .boxed()
    }
}

// ---------------------------------------------------------------------------
// Diagnostics
// ---------------------------------------------------------------------------

#[derive(Debug)]
enum FsDiag {
    MissingArg {
        site: ParseSite,
    },
    BadArg {
        site: ParseSite,
        got: Arc<str>,
    },
    NoMatch {
        site: ParseSite,
        pattern: Arc<str>,
        repo: Arc<str>,
        rev: Arc<str>,
    },
}

impl Diagnostic for FsDiag {
    fn code(&self) -> &str {
        match self {
            FsDiag::MissingArg { .. } => "fs/missing-arg",
            FsDiag::BadArg { .. } => "fs/bad-arg",
            FsDiag::NoMatch { .. } => "fs/no-match",
        }
    }
    fn severity(&self) -> crate::_0_types::Severity {
        match self {
            FsDiag::NoMatch { .. } => crate::_0_types::Severity::Warn,
            _ => crate::_0_types::Severity::Error,
        }
    }
    fn primary(&self) -> &ParseSite {
        match self {
            FsDiag::MissingArg { site } => site,
            FsDiag::BadArg { site, .. } => site,
            FsDiag::NoMatch { site, .. } => site,
        }
    }
    fn render(&self, out: &mut dyn Renderer) {
        match self {
            FsDiag::MissingArg { site } => {
                out.header(
                    self.code(),
                    self.severity(),
                    "fs requires a glob pattern or capture (e.g. fs(src/**/*.rs) or fs($F))",
                );
                out.primary(site);
            }
            FsDiag::BadArg { site, got } => {
                out.header(
                    self.code(),
                    self.severity(),
                    &format!("fs argument `{got}` is not a glob or capture"),
                );
                out.primary(site);
            }
            FsDiag::NoMatch {
                site,
                pattern,
                repo,
                rev,
            } => {
                out.header(
                    self.code(),
                    self.severity(),
                    &format!("fs glob `{pattern}` matched no files in {repo}@{rev}"),
                );
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
    use crate::_0_types::{OpId, RunId};
    use crate::_5_op::{DiagSink, EventSink, OpCtx};
    use crate::readers::MemReader;
    use crate::writers::MemWriter;
    use crate::{Config, RuntimeConfig};
    use futures_util::stream;
    use std::path::Path;
    use std::sync::Mutex;

    fn dummy_site() -> Arc<ParseSite> {
        Arc::new(ParseSite {
            file: Arc::from(Path::new("test.sprf")),
            path: Arc::from(vec![].into_boxed_slice()),
            byte_range: 0..1,
        })
    }

    fn make_config() -> Arc<Config> {
        Arc::new(Config {
            repos: vec![],
            revs: vec![],
            fs_exclude: vec![],
            sprf_files: vec![],
            shell_allow: vec![],
            runtime: RuntimeConfig {
                worker_threads: 1,
                buffer_size: 64,
                flush_interval_ms: 100,
                collect_witnesses: false,
                xref_cartesian_limit: 10_000,
                max_passes: 8,
                max_claims_per_pass: 10_000,
                max_cursors_per_root: 1_000_000,
                max_file_bytes: 1_048_576,
            },
            content_hash: 0,
        })
    }

    #[tokio::test]
    async fn no_match_emits_warn_diag() {
        let config = make_config();
        // MemReader with the repo/rev registered but no files — glob will match nothing.
        let reader: Arc<dyn crate::_3_reader::Reader + Send + Sync> =
            Arc::new(MemReader::new(config.clone()).with_repo("myrepo", &["HEAD"]));
        let writer: Arc<dyn crate::_4_writer::Writer + Send + Sync> = Arc::new(MemWriter::new());

        let fired: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(vec![]));
        let fired2 = fired.clone();
        let diags = DiagSink(Arc::new(
            move |d: Box<dyn crate::_1_diagnostic::Diagnostic>| {
                fired2.lock().unwrap().push(d.code().to_string());
            },
        ));
        let events = EventSink(Arc::new(|_| {}));

        let (result_store, xref_seen) = OpCtx::fresh_xref_state();
        let (__mtx, __mrx) = tokio::sync::mpsc::channel::<crate::mutations::MutationRequest>(32);
        std::mem::drop(__mrx);
        let ctx = OpCtx {
            run_id: RunId(0),
            op_id: OpId(0),
            reader,
            writer,
            config,
            diags,
            events,
            result_store,
            xref_seen,
            store: std::sync::Arc::new(crate::store::NoopStore::new())
                as std::sync::Arc<dyn crate::store::Store>,
            mutations: __mtx,
            cancel: tokio_util::sync::CancellationToken::new(),
            expr_name: None,
            current_site: std::sync::Arc::new(crate::_0_types::ParseSite {
                file: std::sync::Arc::from(std::path::Path::new("")),
                path: std::sync::Arc::from(
                    Vec::<crate::_0_types::ParseSeg>::new().into_boxed_slice(),
                ),
                byte_range: 0..0,
            }),
        };

        let op = FsOp {
            mode: FsMode::Filter(Arc::new(CompiledPattern::compile("src/**/*.rs").unwrap())),
            parse_site: dummy_site(),
        };

        let cursor = Cursor {
            run_id: RunId(0),
            repo: Arc::from("myrepo"),
            rev: Arc::from("HEAD"),
            ..Default::default()
        };

        let input: BoxStream<'static, Arc<[Cursor]>> =
            Box::pin(stream::iter(vec![Arc::<[Cursor]>::from(
                vec![cursor].into_boxed_slice(),
            )]));
        let mut out = op.pipe(input, ctx);
        use futures_util::StreamExt;
        while out.next().await.is_some() {}

        let codes = fired.lock().unwrap().clone();
        assert_eq!(
            codes,
            vec!["fs/no-match"],
            "expected exactly one fs/no-match diag, got {codes:?}"
        );
    }

    #[test]
    fn scan_pointers_fs_present_and_absent() {
        let f = FsFactory;
        let ps = Operator::scan_pointers(&f);
        let sigils: Vec<&str> = ps.iter().map(|p| p.sigil).collect();
        assert_eq!(sigils, vec!["fs", "fs_norm"]);
        let present = base_cursor_hover("main", Some("src/lib.rs"));
        let absent = base_cursor_hover("main", None);
        assert_eq!(&*(ps[0].read)(&present).unwrap(), "src/lib.rs");
        assert_eq!(&*(ps[1].read)(&present).unwrap(), "src/lib.rs");
        assert!((ps[0].read)(&absent).is_none());
        assert!((ps[1].read)(&absent).is_none());
    }

    // -----------------------------------------------------------------------
    // Hover grouping tests
    // -----------------------------------------------------------------------

    fn base_cursor_hover(rev: &str, fs: Option<&str>) -> Cursor {
        Cursor {
            run_id: crate::_0_types::RunId(0),
            repo: Arc::from("org/repo"),
            rev: Arc::from(rev),
            fs: fs.map(|p| crate::_0_types::FilePath(Arc::from(Path::new(p)))),
            ..Default::default()
        }
    }

    #[test]
    fn hover_match_groups_by_file_rev() {
        use crate::_0_types::OpEvidence;
        let site = dummy_site();
        let op = FsOp {
            mode: FsMode::Filter(Arc::new(CompiledPattern::compile("**/Cargo.toml").unwrap())),
            parse_site: site.clone(),
        };

        let mut c1 = base_cursor_hover("main", Some("crates/a/Cargo.toml"));
        c1.evidence.push(OpEvidence {
            op_name: "fs",
            parse_site: site.clone(),
            matched: Arc::from("crates/a/Cargo.toml"),
            capture: None,
        });
        let mut c2 = base_cursor_hover("main", Some("crates/b/Cargo.toml"));
        c2.evidence.push(OpEvidence {
            op_name: "fs",
            parse_site: site.clone(),
            matched: Arc::from("crates/b/Cargo.toml"),
            capture: None,
        });
        let mut c3 = base_cursor_hover("v2", Some("crates/a/Cargo.toml"));
        c3.evidence.push(OpEvidence {
            op_name: "fs",
            parse_site: site.clone(),
            matched: Arc::from("crates/a/Cargo.toml"),
            capture: None,
        });

        let md = op.hover_match(site.as_ref(), &[c1, c2, c3]).unwrap();

        // fs hover_match is a flat bullet list — path is both heading and
        // content. Each unique path appears once, with rev suffix when the
        // cursor set spans multiple revs.
        assert!(
            md.contains("crates/a/Cargo.toml"),
            "missing a/Cargo path: {md}"
        );
        assert!(
            md.contains("crates/b/Cargo.toml"),
            "missing b/Cargo path: {md}"
        );
        assert!(
            md.contains("(rev: v2)") || md.contains("(rev: main)"),
            "expected rev suffix for multi-rev cursor set: {md}"
        );
    }

    // -----------------------------------------------------------------------
    // Pattern parity with v1: re:, segment capture, | alternation, braces.
    // These ran green in v1 via `compile_patterns` inside file_match. v2
    // regressed when Reader carried its own hand-rolled `glob_match`. This
    // test locks in the promoted CompiledPattern path end-to-end through
    // the Reader so the regression stays buried.
    // -----------------------------------------------------------------------

    async fn collect_paths(
        op: FsOp,
        reader: Arc<dyn crate::_3_reader::Reader + Send + Sync>,
    ) -> Vec<String> {
        let config = make_config();
        let writer: Arc<dyn crate::_4_writer::Writer + Send + Sync> = Arc::new(MemWriter::new());
        let diags = DiagSink(Arc::new(|_d| {}));
        let events = EventSink(Arc::new(|_| {}));
        let (result_store, xref_seen) = OpCtx::fresh_xref_state();
        let (__mtx, __mrx) = tokio::sync::mpsc::channel::<crate::mutations::MutationRequest>(32);
        std::mem::drop(__mrx);
        let ctx = OpCtx {
            run_id: RunId(0),
            op_id: OpId(0),
            reader,
            writer,
            config,
            diags,
            events,
            result_store,
            xref_seen,
            store: std::sync::Arc::new(crate::store::NoopStore::new()),
            mutations: __mtx,
            cancel: tokio_util::sync::CancellationToken::new(),
            expr_name: None,
            current_site: std::sync::Arc::new(crate::_0_types::ParseSite {
                file: std::sync::Arc::from(std::path::Path::new("")),
                path: std::sync::Arc::from(
                    Vec::<crate::_0_types::ParseSeg>::new().into_boxed_slice(),
                ),
                byte_range: 0..0,
            }),
        };
        let cursor = Cursor {
            run_id: RunId(0),
            repo: Arc::from("r"),
            rev: Arc::from("main"),
            ..Default::default()
        };
        let input: BoxStream<'static, Arc<[Cursor]>> =
            Box::pin(stream::iter(vec![Arc::<[Cursor]>::from(
                vec![cursor].into_boxed_slice(),
            )]));
        use futures_util::StreamExt;
        op.pipe(input, ctx)
            .flat_map(|batch| stream::iter(batch.iter().cloned().collect::<Vec<_>>()))
            .filter_map(
                |c| async move { c.fs.as_ref().map(|fp| fp.0.to_string_lossy().into_owned()) },
            )
            .collect::<Vec<_>>()
            .await
    }

    fn mem_reader_with_files(paths: &[&str]) -> Arc<dyn crate::_3_reader::Reader + Send + Sync> {
        let config = make_config();
        Arc::new(
            MemReader::new(config)
                .with_repo("r", &["main"])
                .with_files("r", "main", paths),
        )
    }

    fn fs_op(pattern: &str) -> FsOp {
        FsOp {
            mode: FsMode::Filter(Arc::new(CompiledPattern::compile(pattern).unwrap())),
            parse_site: dummy_site(),
        }
    }

    #[tokio::test]
    async fn pattern_parity_regex_prefix() {
        // `re:` prefix routes through PatternMatcher::Regex, NOT glob_match.
        // Pre-regression this returned zero.
        let reader =
            mem_reader_with_files(&["src/foo.rs", "src/bar.rs", "tests/baz.rs", "docs/readme.md"]);
        let op = fs_op(r"re:^src/.*\.rs$");
        let mut got = collect_paths(op, reader).await;
        got.sort();
        assert_eq!(
            got,
            vec!["src/bar.rs".to_string(), "src/foo.rs".to_string()]
        );
    }

    #[tokio::test]
    async fn pattern_parity_segment_capture() {
        // `$ORG/$REPO`-style patterns route through SegmentCapture. Using
        // filename segments here because MemReader treats FS paths verbatim.
        let reader = mem_reader_with_files(&["acme/frontend", "acme/backend", "other/frontend"]);
        let op = fs_op("$ORG/frontend");
        let mut got = collect_paths(op, reader).await;
        got.sort();
        assert_eq!(
            got,
            vec!["acme/frontend".to_string(), "other/frontend".to_string()]
        );
    }

    #[tokio::test]
    async fn pattern_parity_pipe_alternation() {
        // `a|b` splits into two Glob matchers — OR semantics.
        let reader = mem_reader_with_files(&["values.yaml", "config.yml", "main.rs", "lib.rs"]);
        let op = fs_op("*.yaml|*.yml");
        let mut got = collect_paths(op, reader).await;
        got.sort();
        assert_eq!(
            got,
            vec!["config.yml".to_string(), "values.yaml".to_string()]
        );
    }

    #[tokio::test]
    async fn pattern_parity_char_class_and_braces() {
        // `[abc]` and `{a,b}` are globset features — would be invisible to
        // the hand-rolled glob_match.
        let reader = mem_reader_with_files(&["a.rs", "b.rs", "c.rs", "d.rs"]);
        let op = fs_op("[ab].rs");
        let mut got = collect_paths(op, reader).await;
        got.sort();
        assert_eq!(got, vec!["a.rs".to_string(), "b.rs".to_string()]);

        let reader2 = mem_reader_with_files(&["x.rs", "y.rs", "z.rs"]);
        let op2 = fs_op("{x,z}.rs");
        let mut got2 = collect_paths(op2, reader2).await;
        got2.sort();
        assert_eq!(got2, vec!["x.rs".to_string(), "z.rs".to_string()]);
    }
}
