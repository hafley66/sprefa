//! `&.PATH` — input-cursor field read.
//!
//! Per session 20260426 vision lock: `&` is the "input cursor" op.
//! `&.fs.ext`, `&.repo`, `&.byte_range.start` etc. read fixed fields off
//! the in-flight `Cursor` and emit a `Synthesized` capture under the
//! joined-by-dot path name. `last_bound` is set to the same name so the
//! next op's `${PATH}` carveout reads it.
//!
//! Term reads in scope of a binding graph (`${fs.ext}`) are sugar for
//! `&.fs.ext`; that rewrite lives elsewhere — this op just owns the
//! explicit-form runtime.
//!
//! MVP path set (hardcoded; type-graph dynamic walks deferred):
//!   &.repo                 -> cursor.repo
//!   &.rev                  -> cursor.rev
//!   &.fs / &.fs.path       -> cursor.fs (path string)
//!   &.fs.ext               -> cursor.fs.extension()
//!   &.fs.stem              -> cursor.fs.file_stem()
//!   &.fs.name              -> cursor.fs.file_name()
//!   &.byte_range           -> "{start}..{end}"
//!   &.byte_range.start     -> cursor.byte_range.start
//!   &.byte_range.end       -> cursor.byte_range.end
//!   &.byte_range.len       -> end - start
//!
//! Unrecognized paths emit `cursor-ref/unknown-path` at lower time.

use std::path::Path;
use std::sync::Arc;

use effect_runtime::{BoxFuture, RtCtx};
use tree_sitter::Node;

use crate::_0_cursor::{Capture, Cursor};
use crate::_1_op::Op;
use crate::op_ctor::OpCtor;
use crate::pattern_op::PatternDiagnostic;
use crate::value::Value;

/// Compiled `&.PATH` call site. Holds the dotted path components in
/// source order plus the joined name used for the emitted capture.
#[derive(Debug)]
pub struct CursorRefOp {
    pub path:     Vec<Arc<str>>,
    pub joined:   Arc<str>,
}

impl CursorRefOp {
    pub fn new(path: Vec<Arc<str>>) -> Self {
        let joined = Arc::<str>::from(
            path.iter()
                .map(|s| s.as_ref())
                .collect::<Vec<&str>>()
                .join("."),
        );
        Self { path, joined }
    }

    fn resolve(&self, c: &Cursor) -> Option<String> {
        let parts: Vec<&str> = self.path.iter().map(|s| s.as_ref()).collect();
        match parts.as_slice() {
            ["repo"] => Some((*c.repo).to_string()),
            ["rev"]  => Some((*c.rev).to_string()),
            ["fs"] | ["fs", "path"] => c.fs.as_ref().map(path_to_string),
            ["fs", "ext"] => c.fs.as_ref().and_then(|p| p.extension())
                .and_then(|s| s.to_str()).map(str::to_string),
            ["fs", "stem"] => c.fs.as_ref().and_then(|p| p.file_stem())
                .and_then(|s| s.to_str()).map(str::to_string),
            ["fs", "name"] => c.fs.as_ref().and_then(|p| p.file_name())
                .and_then(|s| s.to_str()).map(str::to_string),
            ["byte_range"] => Some(format!("{}..{}", c.byte_range.start, c.byte_range.end)),
            ["byte_range", "start"] => Some(c.byte_range.start.to_string()),
            ["byte_range", "end"]   => Some(c.byte_range.end.to_string()),
            ["byte_range", "len"]   => Some((c.byte_range.end - c.byte_range.start).to_string()),
            _ => None,
        }
    }
}

fn path_to_string(p: &Arc<Path>) -> String {
    p.to_string_lossy().into_owned()
}

impl Op for CursorRefOp {
    fn name(&self) -> &'static str { "&" }

    fn pipe<'a>(&'a self, _ctx: &'a RtCtx, c: Cursor) -> BoxFuture<'a, Vec<Cursor>> {
        Box::pin(async move {
            let value = self.resolve(&c).unwrap_or_default();
            let mut out = c;
            out.captures.push(Capture::synthesized(
                self.joined.clone(),
                Arc::from(value),
            ));
            out.last_bound = Some(self.joined.clone());
            vec![out]
        })
    }
}

impl OpCtor for CursorRefOp {
    const NAME: &'static str = "&";
    const BODY_GRAMMAR: &'static str = "&.IDENT(.IDENT)*";
    const DOC: &'static str = "\
**&**.PATH

Read a fixed field off the input cursor and emit a synthesized capture
under the joined path name. PATH is `IDENT(.IDENT)*`.

Recognized paths:
- `&.repo` / `&.rev`
- `&.fs` / `&.fs.path` / `&.fs.ext` / `&.fs.stem` / `&.fs.name`
- `&.byte_range` / `&.byte_range.{start,end,len}`
";

    fn from_values(_: Vec<Value>) -> Result<Self, Vec<PatternDiagnostic>> {
        // Standard value-arg path is not used; cursor_ref nodes route
        // through `from_paren_node` below.
        Ok(CursorRefOp::new(Vec::new()))
    }

    fn from_paren_node(
        inv_node: Node<'_>,
        src:      &[u8],
    ) -> Option<Result<Self, Vec<PatternDiagnostic>>> {
        if inv_node.kind() != "cursor_ref" {
            return None;
        }
        Some(lower_cursor_ref(inv_node, src))
    }
}

fn lower_cursor_ref(
    node: Node<'_>,
    src:  &[u8],
) -> Result<CursorRefOp, Vec<PatternDiagnostic>> {
    let Some(path_node) = node.child_by_field_name("path") else {
        return Err(vec![PatternDiagnostic {
            code:       "cursor-ref/missing-path",
            message:    "`&` op requires a dotted path: `&.fs.ext`".into(),
            byte_range: node.byte_range(),
        }]);
    };
    let mut walk = path_node.walk();
    let parts: Vec<Arc<str>> = path_node
        .named_children(&mut walk)
        .filter(|n| n.kind() == "identifier")
        .filter_map(|n| std::str::from_utf8(&src[n.byte_range()]).ok())
        .map(Arc::<str>::from)
        .collect();
    if parts.is_empty() {
        return Err(vec![PatternDiagnostic {
            code:       "cursor-ref/empty-path",
            message:    "`&.` requires at least one path segment".into(),
            byte_range: node.byte_range(),
        }]);
    }
    let op = CursorRefOp::new(parts);
    if !is_recognized(&op.path) {
        return Err(vec![PatternDiagnostic {
            code:       "cursor-ref/unknown-path",
            message:    format!(
                "unknown cursor field `&.{}`. \
                 known: repo, rev, fs[.ext|.stem|.name|.path], \
                 byte_range[.start|.end|.len]",
                op.joined,
            ),
            byte_range: node.byte_range(),
        }]);
    }
    Ok(op)
}

fn is_recognized(path: &[Arc<str>]) -> bool {
    let parts: Vec<&str> = path.iter().map(|s| s.as_ref()).collect();
    matches!(parts.as_slice(),
        ["repo"] | ["rev"]
        | ["fs"] | ["fs", "path"] | ["fs", "ext"] | ["fs", "stem"] | ["fs", "name"]
        | ["byte_range"]
        | ["byte_range", "start"] | ["byte_range", "end"] | ["byte_range", "len"]
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tree_sitter::Parser;

    fn host_lang() -> tree_sitter::Language {
        tree_sitter_sprefa::LANGUAGE.into()
    }

    fn lower(src: &str) -> Result<CursorRefOp, Vec<PatternDiagnostic>> {
        let mut parser = Parser::new();
        parser.set_language(&host_lang()).unwrap();
        let tree = parser.parse(src, None).unwrap();
        let root = tree.root_node();
        let node = find_cursor_ref(root).expect("no cursor_ref in source");
        lower_cursor_ref(node, src.as_bytes())
    }

    fn find_cursor_ref(n: Node<'_>) -> Option<Node<'_>> {
        if n.kind() == "cursor_ref" {
            return Some(n);
        }
        let mut walk = n.walk();
        for ch in n.named_children(&mut walk) {
            if let Some(v) = find_cursor_ref(ch) {
                return Some(v);
            }
        }
        None
    }

    #[test]
    fn lower_repo() {
        let op = lower("&.repo").unwrap();
        assert_eq!(op.path.iter().map(|s| s.as_ref()).collect::<Vec<_>>(), vec!["repo"]);
        assert_eq!(&*op.joined, "repo");
    }

    #[test]
    fn lower_fs_ext() {
        let op = lower("&.fs.ext > void").unwrap();
        assert_eq!(op.path.iter().map(|s| s.as_ref()).collect::<Vec<_>>(), vec!["fs", "ext"]);
        assert_eq!(&*op.joined, "fs.ext");
    }

    #[test]
    fn lower_rejects_unknown_field() {
        let err = lower("&.nope").unwrap_err();
        assert_eq!(err[0].code, "cursor-ref/unknown-path");
    }

    #[test]
    fn resolve_repo() {
        let mut c = Cursor::new(Arc::from(b"".as_slice()));
        c.repo = Arc::from("acme/widgets");
        let op = CursorRefOp::new(vec![Arc::from("repo")]);
        assert_eq!(op.resolve(&c).as_deref(), Some("acme/widgets"));
    }

    #[test]
    fn resolve_fs_ext() {
        let mut c = Cursor::new(Arc::from(b"".as_slice()));
        c.fs = Some(Arc::<Path>::from(PathBuf::from("src/lib.rs")));
        let op = CursorRefOp::new(vec![Arc::from("fs"), Arc::from("ext")]);
        assert_eq!(op.resolve(&c).as_deref(), Some("rs"));
    }

    #[test]
    fn resolve_byte_range_len() {
        let mut c = Cursor::new(Arc::from(b"hello world".as_slice()));
        c.byte_range = 6..11;
        let op = CursorRefOp::new(vec![Arc::from("byte_range"), Arc::from("len")]);
        assert_eq!(op.resolve(&c).as_deref(), Some("5"));
    }

    #[tokio::test]
    async fn pipe_emits_synthesized_capture() {
        let ctx = RtCtx::default();
        let mut c = Cursor::new(Arc::from(b"".as_slice()));
        c.repo = Arc::from("acme/widgets");
        let op = CursorRefOp::new(vec![Arc::from("repo")]);
        let out = op.pipe(&ctx, c).await;
        assert_eq!(out.len(), 1);
        let cap = out[0].capture("repo").expect("repo capture");
        assert_eq!(cap.bytes(&out[0].content), b"acme/widgets");
        assert_eq!(out[0].last_bound.as_deref().unwrap(), "repo");
    }
}
