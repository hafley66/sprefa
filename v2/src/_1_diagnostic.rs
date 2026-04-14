//! Diagnostic trait. Each op owns its own diagnostic types; this is the
//! object-safe surface the framework sees.

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
