//! Shift body-relative byte ranges into host-document LSP `Range`s.
//! Convert internal `Diag` records to `lsp_types::Diagnostic`s.

use std::ops::Range;

use lsp_types::{Diagnostic, DiagnosticSeverity, Range as LspRange};

use crate::cst::diag::{Diag, Severity};
use crate::cst::lsp::position::byte_to_position;

/// Shift `byte_range` (relative to `body_off=0`) by `host_off` into a host LSP
/// `Range` against `host_source`.
pub fn shift_to_host(host_source: &str, host_off: usize, byte_range: Range<usize>) -> LspRange {
    let start = host_off + byte_range.start;
    let end   = host_off + byte_range.end;
    LspRange {
        start: byte_to_position(host_source, start),
        end:   byte_to_position(host_source, end),
    }
}

pub fn severity_to_lsp(s: Severity) -> DiagnosticSeverity {
    match s {
        Severity::Error   => DiagnosticSeverity::ERROR,
        Severity::Warning => DiagnosticSeverity::WARNING,
        Severity::Info    => DiagnosticSeverity::INFORMATION,
        Severity::Hint    => DiagnosticSeverity::HINT,
    }
}

pub fn diag_to_lsp(host_source: &str, host_off: usize, d: &Diag) -> Diagnostic {
    Diagnostic {
        range: shift_to_host(host_source, host_off, d.byte_range.clone()),
        severity: Some(severity_to_lsp(d.severity)),
        code: Some(lsp_types::NumberOrString::String(d.code.to_string())),
        code_description: None,
        source: None,
        message: d.message.clone(),
        related_information: None,
        tags: None,
        data: None,
    }
}

pub fn diags_to_lsp(host_source: &str, host_off: usize, diags: &[Diag]) -> Vec<Diagnostic> {
    diags.iter().map(|d| diag_to_lsp(host_source, host_off, d)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shift_offsets() {
        let host = "abcdef\nghi";
        let r = shift_to_host(host, 7, 0..2); // body byte 0..2 → host 7..9 → ghi[0..2]
        assert_eq!(r.start, lsp_types::Position { line: 1, character: 0 });
        assert_eq!(r.end,   lsp_types::Position { line: 1, character: 2 });
    }

    #[test]
    fn diag_conversion() {
        let host = "abc\ndef";
        let d = Diag::error("E001", "boom", 0..3);
        let l = diag_to_lsp(host, 4, &d);
        assert_eq!(l.severity, Some(DiagnosticSeverity::ERROR));
        assert_eq!(l.range.start, lsp_types::Position { line: 1, character: 0 });
        assert_eq!(l.range.end,   lsp_types::Position { line: 1, character: 3 });
    }
}
