//! Diagnostic trait surface.
//!
//! Each op owns its own diagnostic types (one enum per op, e.g.
//! `JsonDiag`, `RuleDiag`, `FsDiag`). This module provides the
//! object-safe trait surface the framework sees plus a few
//! framework-owned diagnostic types for checks the framework runs
//! itself (scan-pointer verification).

use std::sync::Arc;

use crate::types::{ParseSite, RunCtx, Severity};

/// Sink-facing renderer. The CLI implements one, the LSP another, the
/// test harness a third. Ops call into this from their
/// `Diagnostic::render` impls to push header + primary/related spans +
/// notes.
///
/// **Why a trait, not a concrete struct:** ops should not know whether
/// they're rendering to ANSI terminal, LSP JSON, or test-snapshot text.
/// **Cardinality:** three or four impls across the whole project (CLI,
/// LSP, tests). One instance per diagnostic emission context.
/// **Call frequency:** each method is called 0-N times per `Diagnostic`
/// (header once, primary once, related a few times, note optional).
pub trait Renderer {
    /// Emit the top line — code + severity + summary message.
    fn header(&mut self, code: &str, severity: Severity, message: &str);
    /// Anchor the primary source span.
    fn primary(&mut self, site: &ParseSite);
    /// A secondary span with an explanation (e.g., "binding site is here").
    fn related(&mut self, site: &ParseSite, message: &str);
    /// A free-form note without a source anchor.
    fn note(&mut self, message: &str);
}

/// Object-safe diagnostic trait. Op-owned variants live in the op's
/// module and `impl Diagnostic` to adapt.
///
/// **Why object-safe:** the framework holds `Box<dyn Diagnostic>` in
/// `RunEvent::Diag` and `Vec<Box<dyn Diagnostic>>` in `ProgramCtx.diags`.
/// **Implementors:** every op defines a `FooDiag` enum and impls this.
/// Plus framework-owned types below for scan-pointer checks.
/// **Cardinality:** trait objects range thousands per run in an
/// error-heavy session; typically dozens in a clean run.
/// **Allocation:** one Box per diagnostic emission. The Diagnostic
/// itself is small (pointers into Arc'd source + a variant tag).
pub trait Diagnostic: Send + Sync + std::fmt::Debug {
    /// Namespaced stable code, e.g. "json/leaf-miss", "fs/empty". Used
    /// for tooling that wants to filter / suppress / document specific
    /// diagnostics. Must be stable across versions.
    fn code(&self)     -> &str;
    /// Error / Warn / Info / Hint. Drives CLI exit codes and LSP
    /// severity propagation to the client.
    fn severity(&self) -> Severity;
    /// The source span the diagnostic points at. LSP uses this for
    /// range highlighting; the CLI for line-number printing.
    fn primary(&self)  -> &ParseSite;
    /// Push formatted output through the Renderer. Called once per
    /// emission by the consumer side of the pipeline.
    fn render(&self, out: &mut dyn Renderer);
    /// Optional runtime identity triple. `None` for parse/lower diags
    /// that have no runtime provenance. `Some` for diagnostics emitted
    /// from inside `Op::pipe` that want to tell the user which run and
    /// which op-path produced the failure.
    fn run_ctx(&self)  -> Option<&RunCtx> { None }
}

// ---------------------------------------------------------------------------
// Framework-owned diagnostics — scan-pointer checker pass
// ---------------------------------------------------------------------------
//
// These live here because the scan-pointer verification loop is
// framework-owned (not op-owned). Every scan-pointer tag-op writes
// claims into the relations table; a single post-pass cross-checks
// them. The two warnings below are that pass's only output.

/// Warn emitted by the scan-pointer checker when a `Claimed` capture
/// fails to verify against the configured universe (repo/rev lists,
/// git tree, …). Anchored at the capture's binding site when
/// available; falls back to a synthetic site at the rule declaration
/// otherwise.
///
/// **When:** once per (claim, universe) pair that doesn't match.
/// **Cardinality:** typically zero in a passing scan; grows with
/// config drift or stale scan-pointer claims.
#[derive(Debug)]
pub struct ScanPointerUnverified {
    pub site:  ParseSite,
    pub sigil: Arc<str>,
    pub value: Arc<str>,
}

impl Diagnostic for ScanPointerUnverified {
    fn code(&self) -> &str { "scan-pointer/unverified" }
    fn severity(&self) -> Severity { Severity::Warn }
    fn primary(&self) -> &ParseSite { &self.site }
    fn render(&self, out: &mut dyn Renderer) {
        out.header(
            self.code(),
            self.severity(),
            &format!(
                "scan-pointer `${}` value `{}` did not verify against the configured universe",
                self.sigil, self.value,
            ),
        );
        out.primary(&self.site);
    }
}

/// Warn emitted when the scan-check loop reaches its configured depth
/// cap before the claim set stops growing. Indicates a cyclic or
/// runaway claim explosion (content keeps naming new repos/revs the
/// loop hasn't seen).
///
/// **When:** once per run that hits `RuntimeConfig.scan_check_depth`.
/// **Cardinality:** rare — usually one per pathological .sprf program,
/// or per corrupted index. Useful as a ceiling alarm.
#[derive(Debug)]
pub struct ScanPointerDepthExhausted {
    pub site:  ParseSite,
    pub depth: usize,
}

impl Diagnostic for ScanPointerDepthExhausted {
    fn code(&self) -> &str { "scan-pointer/depth-exhausted" }
    fn severity(&self) -> Severity { Severity::Warn }
    fn primary(&self) -> &ParseSite { &self.site }
    fn render(&self, out: &mut dyn Renderer) {
        out.header(
            self.code(),
            self.severity(),
            &format!(
                "scan-check loop hit depth cap `{}` before the known set stopped growing; \
                 some scan-pointer values may still be unverified",
                self.depth,
            ),
        );
        out.primary(&self.site);
    }
}
