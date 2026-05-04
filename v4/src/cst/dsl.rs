//! DSL trait surface.
//!
//! Three implementation strategies must coexist:
//! - tree-sitter grammar.js (re, glob): `injection_grammar` returns Some
//! - hand-rolled parser (json):           `injection_grammar` returns None,
//!                                        `compile` parses raw bytes itself
//! - borrowed engine (ast-grep):          same as hand-rolled
//!
//! `Compiled::match_into` runs the compiled pattern against arbitrary target
//! bytes and emits `CaptureRow`s into a sink. Rows are name-keyed; downstream
//! consumers (sprf cursor bag, anything else) join by name.

use std::ops::ControlFlow;
use std::ops::Range;
use std::sync::Arc;

use crate::cst::diag::{Diag, DiagSink};
use crate::cst::path::PathBuilder;
use crate::cst::lsp::providers::DslBodyLsp;

pub trait Dsl: Send + Sync + 'static {
    fn id(&self) -> &'static str;

    /// Compile a body into a runnable matcher.
    /// Fatal failure → `Err(Diag)`. Non-fatal observations → push to `diags`.
    fn compile(&self, body: &[u8], diags: &dyn DiagSink) -> Result<Box<dyn Compiled>, Diag>;

    /// Some when the DSL body is itself parsed by tree-sitter (re, glob).
    /// None when the DSL parses by hand or via a borrowed engine (json, ast).
    fn injection_grammar(&self) -> Option<tree_sitter::Language> { None }

    /// Optional LSP body-feature provider. Default: no LSP support.
    fn lsp(&self) -> Option<&dyn DslBodyLsp> { None }
}

pub trait Compiled: Send + Sync {
    /// Names of captures this compiled pattern can bind, in declaration order.
    fn declared_captures(&self) -> &[Arc<str>];

    /// Run against `target` and emit captures. `target_off` is the absolute
    /// byte offset of `target` within whatever larger document (used to make
    /// emitted spans absolute).
    fn match_into(&self, target: &[u8], target_off: usize, sink: &mut dyn CaptureSink);

    /// Contribute path items for a position inside the compiled body. Used to
    /// build the global path scheme. TS-backed DSLs can use the helper at
    /// `crate::cst::path::ts_default_emit`. Default: no segments emitted.
    fn emit_path_items(&self, _body_byte: usize, _builder: &mut PathBuilder) {}
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaptureRow {
    pub name: Arc<str>,
    pub kind: CaptureKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CaptureKind {
    Span    { byte_range: Range<usize> },
    Literal { value: Arc<[u8]> },
}

pub trait CaptureSink: Send {
    fn emit(&mut self, row: CaptureRow) -> ControlFlow<()>;
}

/// Vec-backed sink for tests and LSP collection paths.
#[derive(Default)]
pub struct VecCaptureSink { pub rows: Vec<CaptureRow> }

impl VecCaptureSink {
    pub fn new() -> Self { Self::default() }
}

impl CaptureSink for VecCaptureSink {
    fn emit(&mut self, row: CaptureRow) -> ControlFlow<()> {
        self.rows.push(row);
        ControlFlow::Continue(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_sink_collects() {
        let mut sink = VecCaptureSink::new();
        let _ = sink.emit(CaptureRow {
            name: Arc::from("X"),
            kind: CaptureKind::Span { byte_range: 0..3 },
        });
        let _ = sink.emit(CaptureRow {
            name: Arc::from("Y"),
            kind: CaptureKind::Literal { value: Arc::from(b"hi".as_slice()) },
        });
        assert_eq!(sink.rows.len(), 2);
    }
}
