//! sprefa_macros — proc-macros for pipeline op authoring.
//!
//! The single attribute today is `#[sprf_pattern_op(name = "...")]`.
//! Applied to a pattern op struct definition, it emits the boilerplate
//! `impl Op` that wires `name / language / highlights / pipe` to the
//! companion `PatternOp` implementation and the generated
//! `op_languages` table (§14.5, spec at
//! `v3/crates/sprefa_parse/parse.md`).
//!
//! Example:
//!
//! ```ignore
//! use sprefa_macros::sprf_pattern_op;
//! use pipeline::{PatternOp, CompiledPattern, PatternDiagnostic, Cursor, Op};
//!
//! #[sprf_pattern_op(name = "glob")]
//! pub struct GlobOp { /* ...compiled state... */ }
//!
//! impl PatternOp for GlobOp {
//!     fn compile(...) -> Result<CompiledPattern, Vec<PatternDiagnostic>> { ... }
//!     fn binds_captures(...) -> Vec<std::sync::Arc<str>> { ... }
//!     fn pattern_pipe<'a>(&'a self, ctx: &'a effect_runtime::RtCtx, c: Cursor)
//!         -> effect_runtime::BoxFuture<'a, Vec<Cursor>> { ... }
//! }
//! // `impl Op for GlobOp` is generated automatically.
//! ```
//!
//! Expansion relies on a `crate::op_languages` module (step 4 landing)
//! exposing `language_of(&str) -> Option<tree_sitter::Language>` and
//! `highlights_of(&str) -> Option<&'static str>`. Until that module
//! exists, using the macro will fail to compile with a missing-module
//! error — exactly the drift check we want.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{parse_macro_input, ItemStruct, LitStr, Meta, MetaNameValue, Expr, ExprLit, Lit};

#[proc_macro_attribute]
pub fn sprf_pattern_op(attr: TokenStream, item: TokenStream) -> TokenStream {
    let name = match parse_name_attr(attr.into()) {
        Ok(n) => n,
        Err(e) => return e.to_compile_error().into(),
    };
    let item = parse_macro_input!(item as ItemStruct);
    let ident = &item.ident;

    let expanded = quote! {
        #item

        impl ::pipeline::Op for #ident {
            fn name(&self) -> &'static str { #name }

            fn language(&self) -> ::core::option::Option<::tree_sitter::Language> {
                ::core::option::Option::Some(
                    crate::op_languages::language_of(#name)
                        .expect(concat!(
                            "sprf_pattern_op(",
                            #name,
                            "): op_languages::language_of returned None — ",
                            "build.rs did not register this op",
                        )),
                )
            }

            fn highlights(&self) -> ::core::option::Option<&'static str> {
                crate::op_languages::highlights_of(#name)
            }

            fn pipe<'a>(
                &'a self,
                ctx: &'a ::effect_runtime::RtCtx,
                c: ::pipeline::Cursor,
            ) -> ::effect_runtime::BoxFuture<'a, ::std::vec::Vec<::pipeline::Cursor>> {
                <Self as ::pipeline::PatternOp>::pattern_pipe(self, ctx, c)
            }
        }
    };

    expanded.into()
}

fn parse_name_attr(tokens: TokenStream2) -> syn::Result<LitStr> {
    let meta: Meta = syn::parse2(tokens)?;
    match meta {
        Meta::NameValue(MetaNameValue {
            path,
            value: Expr::Lit(ExprLit { lit: Lit::Str(s), .. }),
            ..
        }) if path.is_ident("name") => Ok(s),
        _ => Err(syn::Error::new_spanned(
            meta,
            "expected #[sprf_pattern_op(name = \"<op>\")]",
        )),
    }
}
