//! `DslBodyLsp`: per-DSL body-pure LSP feature provider.
//!
//! All methods take raw body bytes and an optional position. The DSL caches
//! its parsed body internally if it wants. Lib never sees the parsed shape.
//!
//! Cross-DSL features (completion, definition, references, signature help,
//! code action, code lens) live consumer-side. The lib exposes `locate()`
//! and the consumer dispatches.
//!
//! Returned ranges are body-relative; the consumer (or `crate::cst::lsp::shift`)
//! shifts them into host-document LSP `Range`s.

use std::ops::Range;

use lsp_types::{
    ColorInformation, ColorPresentation, CompletionItem, DocumentHighlight, DocumentLink,
    DocumentSymbol, FoldingRange, Hover, InlayHint, SelectionRange, TextEdit,
};

use crate::cst::diag::DiagSink;

/// One semantic token: body-relative byte range plus a token type/modifier
/// index pair into the consumer-assembled legend.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticToken {
    pub byte_range:    Range<usize>,
    pub token_type:    u32,
    pub token_modifiers: u32,
}

pub trait DslBodyLsp: Send + Sync {
    fn hover(&self, _body: &[u8], _byte: usize) -> Option<Hover> { None }

    fn diagnostics(&self, _body: &[u8], _diags: &dyn DiagSink) {}

    fn semantic_tokens(&self, _body: &[u8]) -> Vec<SemanticToken> { vec![] }

    /// Type-ahead suggestions at `byte` inside the body. Default empty.
    /// Each dsl decides what to surface (keywords, captured names, etc.).
    fn completions(&self, _body: &[u8], _byte: usize) -> Vec<CompletionItem> { vec![] }

    fn folding_ranges(&self, _body: &[u8]) -> Vec<FoldingRange> { vec![] }

    fn document_symbols(&self, _body: &[u8]) -> Vec<DocumentSymbol> { vec![] }

    fn selection_ranges(&self, _body: &[u8], _byte: usize) -> Vec<SelectionRange> { vec![] }

    fn document_highlights(&self, _body: &[u8], _byte: usize) -> Vec<DocumentHighlight> { vec![] }

    fn inlay_hints(&self, _body: &[u8], _range: Range<usize>) -> Vec<InlayHint> { vec![] }

    fn document_links(&self, _body: &[u8]) -> Vec<DocumentLink> { vec![] }

    fn formatting(&self, _body: &[u8]) -> Option<Vec<TextEdit>> { None }

    fn prepare_rename(&self, _body: &[u8], _byte: usize) -> Option<lsp_types::Range> { None }

    fn rename(&self, _body: &[u8], _byte: usize, _new_name: &str) -> Option<Vec<TextEdit>> { None }

    fn document_color(&self, _body: &[u8]) -> Vec<ColorInformation> { vec![] }

    fn color_presentation(
        &self,
        _body: &[u8],
        _color: lsp_types::Color,
        _range: lsp_types::Range,
    ) -> Vec<ColorPresentation> { vec![] }
}
