//! Diagnostic trait. Each op owns its own diagnostic types; this is the
//! object-safe surface the framework sees.

use std::sync::Arc;

use crate::_0_types::{ParseSite, RunCtx, Severity};

/// Sink-facing renderer. The CLI implements one, the LSP another, the test
/// harness a third. Ops call into this from their `Diagnostic::render` impls.
pub trait Renderer {
    fn header(&mut self, code: &str, severity: Severity, message: &str);
    fn primary(&mut self, site: &ParseSite);
    fn related(&mut self, site: &ParseSite, message: &str);
    fn note(&mut self, message: &str);
}

/// Object-safe. Op-owned variants live in the op's module.
pub trait Diagnostic: Send + Sync + std::fmt::Debug {
    /// Namespaced stable code, e.g. "json/leaf-miss", "fs/empty".
    fn code(&self)     -> &str;
    fn severity(&self) -> Severity;
    fn primary(&self)  -> &ParseSite;
    fn render(&self, out: &mut dyn Renderer);
    fn run_ctx(&self)  -> Option<&RunCtx> { None }
}

// ---------------------------------------------------------------------------
// Framework-owned: scan-pointer checker pass (see `_13_scan_check.rs`)
// ---------------------------------------------------------------------------

/// Warn emitted by the scan-pointer checker when a Claimed capture fails
/// to verify against the configured universe (repo/rev lists, git tree, …).
/// Anchored at the capture's binding site when available; falls back to a
/// synthetic site at the rule declaration otherwise.
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

/// Warn emitted by the scan-check loop when the configured depth is reached
/// before the Config stops growing. Indicates a cyclic or runaway claim
/// explosion (content keeps naming new repos/revs the loop hasn't seen).
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
