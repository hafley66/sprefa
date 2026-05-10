//! Op-name → `DslBodyLsp` dispatcher. Hover/completion locate the dsl
//! body by calling `app::lsp_locate_dsl` (cached `DocState.program`);
//! this file only owns the trait-object handle that wraps each Dsl
//! impl. New DSLs hook in by adding one match arm here.
//!
//! Mirrors the pattern in `semantic.rs::dsl_tokens_for`.

use v4::cst::dsl::Dsl;
use v4::cst::dsls::{glob::GlobDsl, json::JsonDsl, markdown::MarkdownDsl, re::ReDsl};
use v4::cst::lsp::providers::DslBodyLsp;

/// `None` for op names without a `DslBodyLsp` impl (host-rendered by default).
pub fn provider_for(op_name: &str) -> Option<Box<dyn ProviderHandle>> {
    match op_name {
        "re"   => Some(Box::new(ReHandle(ReDsl::new()))),
        "glob" => Some(Box::new(GlobHandle(GlobDsl::new()))),
        "json" => Some(Box::new(JsonHandle(JsonDsl::new()))),
        "render" | "render_markdown" | "render.markdown" => {
            Some(Box::new(MarkdownHandle(MarkdownDsl::new())))
        }
        // ast — TODO once AstDsl gains a DslBodyLsp impl (#8).
        _ => None,
    }
}

/// Owns a Dsl instance and exposes its `lsp()` borrow with the right
/// lifetime. Boxing the trait object directly tangles lifetimes; this
/// adaptor keeps the borrow chain tidy.
pub trait ProviderHandle: Send + Sync {
    fn lsp(&self) -> Option<&dyn DslBodyLsp>;
}

struct ReHandle(ReDsl);
struct GlobHandle(GlobDsl);
struct JsonHandle(JsonDsl);
struct MarkdownHandle(MarkdownDsl);

impl ProviderHandle for ReHandle   { fn lsp(&self) -> Option<&dyn DslBodyLsp> { self.0.lsp() } }
impl ProviderHandle for GlobHandle { fn lsp(&self) -> Option<&dyn DslBodyLsp> { self.0.lsp() } }
impl ProviderHandle for JsonHandle { fn lsp(&self) -> Option<&dyn DslBodyLsp> { self.0.lsp() } }
impl ProviderHandle for MarkdownHandle { fn lsp(&self) -> Option<&dyn DslBodyLsp> { self.0.lsp() } }
