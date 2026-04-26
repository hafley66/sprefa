//! `fs(...)` — path enumeration over `(cursor.repo, cursor.rev)`
//! (parse.md §14.5c).
//!
//! Two modes:
//!   * `fs(**/*.rs)` — filter: keep only files whose path matches the
//!     glob. One input cursor expands to N output cursors (one per kept
//!     file), each with `cursor.fs = Some(path)`. Empty match drops the
//!     input cursor.
//!   * `fs($P)` — bind: enumerate every file under `(repo, rev)`, set
//!     `cursor.fs`, and additionally write a synthesized capture `$P`
//!     carrying the path string.
//!
//! The file listing flows through the effect runtime as an
//! [`FsListFilesEffect`].

use std::path::Path;
use std::sync::Arc;

use effect_runtime::{BoxFuture, RtCtx};

use crate::_0_cursor::{Capture, Cursor};
use crate::_1_op::Op;
use crate::effects::FsListFilesEffect;
use crate::op_ctor::OpCtor;
use crate::pattern_op::PatternDiagnostic;
use crate::value::{ArgSpec, Value};

const FS_ARG_SPEC: &[ArgSpec] = &[ArgSpec::Op];

#[derive(Debug)]
pub struct FsOp {
    pub mode: FsMode,
}

#[derive(Debug)]
pub enum FsMode {
    Filter(Arc<dyn Op>),
    /// Bind mode. `pat` matches every file (`**`); `name` receives each
    /// path as a synthesized capture.
    Bind {
        name: Arc<str>,
        pat: Arc<dyn Op>,
    },
}

impl FsOp {
    pub fn from_source(src: &str) -> Result<Self, Vec<PatternDiagnostic>> {
        let arg = src.trim();
        if arg.is_empty() {
            return Err(vec![PatternDiagnostic {
                code: "fs/missing-arg",
                message: "fs requires a glob pattern or capture (e.g. fs(src/**/*.rs) or fs($F))"
                    .into(),
                byte_range: 0..src.len(),
            }]);
        }
        if let Some(name) = super::repo::parse_capture(arg) {
            let pat = super::glob::compile_str("**")?;
            return Ok(FsOp {
                mode: FsMode::Bind {
                    name,
                    pat: Arc::new(pat),
                },
            });
        }
        let pat = super::glob::compile_str(arg)?;
        Ok(FsOp {
            mode: FsMode::Filter(Arc::new(pat)),
        })
    }

    pub fn from_values(values: Vec<Value>) -> Result<Self, Vec<PatternDiagnostic>> {
        match values.as_slice() {
            [Value::Term { name, .. }] => Ok(FsOp::bind(name.clone())),
            [Value::Atom(s)] | [Value::Str(s)] => {
                let pat = super::glob::compile_str(s)?;
                Ok(FsOp {
                    mode: FsMode::Filter(Arc::new(pat)),
                })
            }
            [Value::Op(op)] if op.try_raw_regex().is_some() => Ok(FsOp {
                mode: FsMode::Filter(op.clone()),
            }),
            [Value::Op(_)] => Err(vec![PatternDiagnostic {
                code: "fs/unsupported-pattern",
                message: "fs requires an op exposing a raw regex (glob or re)".into(),
                byte_range: 0..0,
            }]),
            [] => Err(vec![PatternDiagnostic {
                code: "fs/missing-arg",
                message: "fs requires a glob pattern or capture (e.g. fs(src/**/*.rs) or fs($F))"
                    .into(),
                byte_range: 0..0,
            }]),
            _ => Err(vec![PatternDiagnostic {
                code: "value/arity-mismatch",
                message: format!("fs takes 1 argument, got {}", values.len()),
                byte_range: 0..0,
            }]),
        }
    }

    pub fn filter(pat: Arc<dyn Op>) -> Self {
        FsOp {
            mode: FsMode::Filter(pat),
        }
    }

    pub fn bind(name: impl Into<Arc<str>>) -> Self {
        let pat = super::glob::compile_str("**").expect("'**' is a valid glob");
        FsOp {
            mode: FsMode::Bind {
                name: name.into(),
                pat: Arc::new(pat),
            },
        }
    }

    fn pattern(&self) -> &Arc<dyn Op> {
        match &self.mode {
            FsMode::Filter(p) => p,
            FsMode::Bind { pat, .. } => pat,
        }
    }

    fn bind_name(&self) -> Option<&Arc<str>> {
        match &self.mode {
            FsMode::Bind { name, .. } => Some(name),
            FsMode::Filter(_) => None,
        }
    }
}

impl OpCtor for FsOp {
    const NAME: &'static str = "fs";
    const BODY_GRAMMAR: &'static str = "glob (`*` `**` `?` `$NAME`)";
    const DOC: &'static str = "\
**fs**(_glob_ | `$NAME`)

Enumerate files under `(cursor.repo, cursor.rev)`. Listings are cached
via `FsListFilesEffect` in the `fs` domain.

- `fs(**/*.rs)` — filter-enumerate; emits one cursor per match.
- `fs($P)` — bind mode; emits one cursor per file with capture `P`.
";

    fn from_values(values: Vec<Value>) -> Result<Self, Vec<PatternDiagnostic>> {
        Self::from_values(values)
    }
}

impl Op for FsOp {
    fn name(&self) -> &'static str {
        "fs"
    }

    fn pipe<'a>(&'a self, ctx: &'a RtCtx, batch: Arc<[Cursor]>) -> BoxFuture<'a, Arc<[Cursor]>> {
        Box::pin(crate::_1_op::per_cursor(batch, move |c| async move {
            let pat = self.pattern();
            let regex = match pat.try_raw_regex() {
                Some(r) => r,
                None => return vec![],
            };
            let paths = ctx
                .put(FsListFilesEffect {
                    repo: c.repo.clone(),
                    rev: c.rev.clone(),
                })
                .await;
            let mut out = Vec::new();
            for p in paths {
                let path_str = p.to_string_lossy();
                let Some(caps) = regex.captures(path_str.as_bytes()) else {
                    continue;
                };
                let mut nc = c.clone();
                nc.fs = Some(Path::new(path_str.as_ref()).into());

                // Emit one synthesized capture per named group. Path
                // strings are not stored in `cursor.content`, so
                // SpanBacked is out; Synthesized carries the matched
                // bytes inline. `last_bound` points at the rightmost
                // glob capture so downstream scan-pointer ops see the
                // freshest one.
                let mut last_glob_cap: Option<Arc<str>> = None;
                for name in regex.capture_names().flatten() {
                    if let Some(m) = caps.name(name) {
                        let val: Arc<str> =
                            Arc::from(std::str::from_utf8(m.as_bytes()).unwrap_or(""));
                        let cap_name: Arc<str> = Arc::from(name);
                        nc.captures
                            .push(Capture::synthesized(cap_name.clone(), val));
                        last_glob_cap = Some(cap_name);
                    }
                }

                if let Some(name) = self.bind_name() {
                    nc.captures.push(Capture::synthesized(
                        name.clone(),
                        Arc::from(path_str.as_ref()),
                    ));
                    nc.last_bound = Some(name.clone());
                } else if let Some(n) = last_glob_cap {
                    nc.last_bound = Some(n);
                }
                out.push(nc);
            }
            out
        }))
    }

    fn language(&self) -> Option<tree_sitter::Language> {
        crate::op_languages::language_of("glob")
    }

    fn arg_spec(&self) -> &[ArgSpec] {
        FS_ARG_SPEC
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::FsListFilesBatcher;
    use crate::readers::{FileSource, MemFileSource};
    use effect_runtime::RtCtxBuilder;

    fn rt_with(paths: &[&str]) -> RtCtx {
        let source: Arc<dyn FileSource> =
            Arc::new(MemFileSource::new().with_files("r", "main", paths));
        RtCtxBuilder::new()
            .register_pure::<FsListFilesEffect, _>(64, FsListFilesBatcher::new(source))
            .build()
    }

    fn cursor_at(repo: &str, rev: &str) -> Cursor {
        let mut c = Cursor::default();
        c.repo = Arc::from(repo);
        c.rev = Arc::from(rev);
        c
    }

    #[tokio::test]
    async fn filter_and_bind_scenarios() {
        let ctx = rt_with(&[
            "src/lib.rs",
            "src/foo/lib.rs",
            "src/foo/main.rs",
            "tests/it.rs",
            "README.md",
            "Cargo.toml",
        ]);

        let op = FsOp::from_source("src/**/*.rs").unwrap();
        let out_arc = op.pipe(&ctx, Arc::from(vec![cursor_at("r", "main")])).await;
        let mut out: Vec<Cursor> = out_arc.iter().cloned().collect();
        out.sort_by(|a, b| a.fs.cmp(&b.fs));
        let got: Vec<String> = out
            .iter()
            .map(|c| c.fs.as_ref().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            got,
            vec![
                "src/foo/lib.rs".to_string(),
                "src/foo/main.rs".to_string(),
                "src/lib.rs".to_string(),
            ]
        );
        assert!(out[0].captures.is_empty(), "filter mode does not bind");

        let op = FsOp::from_source("$P").unwrap();
        let out = op.pipe(&ctx, Arc::from(vec![cursor_at("r", "main")])).await;
        assert_eq!(out.len(), 6);
        assert!(out.iter().all(|c| c.captures.len() == 1));
        assert!(out.iter().all(|c| &*c.captures[0].name == "P"));
        assert_eq!(
            out[0].captures[0].bytes(&out[0].content),
            out[0].fs.as_ref().unwrap().to_string_lossy().as_bytes()
        );
    }

    #[tokio::test]
    async fn no_match_drops_cursor() {
        let ctx = rt_with(&["README.md"]);
        let op = FsOp::from_source("src/**/*.rs").unwrap();
        let out = op.pipe(&ctx, Arc::from(vec![cursor_at("r", "main")])).await;
        assert!(out.is_empty());
    }

    #[tokio::test]
    async fn unknown_repo_rev_yields_empty() {
        let ctx = rt_with(&["a/b.rs"]);
        let op = FsOp::from_source("**/*.rs").unwrap();
        let out = op
            .pipe(&ctx, Arc::from(vec![cursor_at("other", "main")]))
            .await;
        assert!(out.is_empty());
    }

    #[test]
    fn missing_arg_diagnoses() {
        let err = match FsOp::from_source("  ") {
            Err(e) => e,
            Ok(_) => panic!("expected fs/missing-arg"),
        };
        assert_eq!(err[0].code, "fs/missing-arg");
    }
}
