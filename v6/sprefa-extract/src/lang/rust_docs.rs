//! Rust doc-comment facts: the `///` lines, sections and owners of a file,
//! as TypeF doc rows. Port of v5 `rust_docs_from`.

use crate::family::{DocFact, DocTag, TypeF};
use crate::rows::FamilyBundle;
use crate::shape::Strings;

use super::rust::syn_span;
use super::rust_type_refs::primary_type;

/// Port of v5 `rust_docs_from`. The walked set is v5's: struct, enum, union,
/// trait, fn and impl methods. A documented const or alias mints no row.
pub(crate) fn doc_facts(
    parsed: &syn::File,
    line_starts: &[u32],
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
) {
    doc_facts_in_items(&parsed.items, line_starts, strings, sink);
}

fn doc_facts_in_items(
    items: &[syn::Item],
    line_starts: &[u32],
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
) {
    for item in items {
        match item {
            syn::Item::Struct(s) => {
                push_doc(sink, strings, line_starts, s.ident.span(), &s.attrs, None)
            }
            syn::Item::Enum(en) => {
                push_doc(sink, strings, line_starts, en.ident.span(), &en.attrs, None)
            }
            syn::Item::Union(u) => {
                push_doc(sink, strings, line_starts, u.ident.span(), &u.attrs, None)
            }
            syn::Item::Trait(t) => {
                push_doc(sink, strings, line_starts, t.ident.span(), &t.attrs, None)
            }
            syn::Item::Fn(f) => push_doc(
                sink,
                strings,
                line_starts,
                f.sig.ident.span(),
                &f.attrs,
                None,
            ),
            syn::Item::Impl(i) => {
                let owner = primary_type(&i.self_ty);
                for ii in &i.items {
                    if let syn::ImplItem::Fn(m) = ii {
                        push_doc(
                            sink,
                            strings,
                            line_starts,
                            m.sig.ident.span(),
                            &m.attrs,
                            owner.as_deref(),
                        );
                    }
                }
            }
            syn::Item::Mod(m) => {
                if let Some((_, inner)) = &m.content {
                    doc_facts_in_items(inner, line_starts, strings, sink);
                }
            }
            _ => {}
        }
    }
}

fn push_doc(
    sink: &mut FamilyBundle<TypeF>,
    strings: &mut Strings,
    line_starts: &[u32],
    span: proc_macro2::Span,
    attrs: &[syn::Attribute],
    parent: Option<&str>,
) {
    let lines = doc_lines(attrs);
    if lines.is_empty() {
        return;
    }
    let text = lines.join("\n");
    let tags = doc_sections(&text, strings);
    sink.aux.docs.push(DocFact {
        owner: syn_span(line_starts, span),
        parent: parent.map(|name| strings.intern(name)),
        text: strings.intern(&text),
        tags,
    });
}

/// Each `#[doc = "..."]` value, the single leading space syn keeps from `/// x`
/// stripped. Port of v5 `rust_doc_lines`.
fn doc_lines(attrs: &[syn::Attribute]) -> Vec<String> {
    let mut lines = Vec::new();
    for attr in attrs {
        if !attr.path().is_ident("doc") {
            continue;
        }
        if let syn::Meta::NameValue(nv) = &attr.meta {
            if let syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(s),
                ..
            }) = &nv.value
            {
                let value = s.value();
                lines.push(value.strip_prefix(' ').unwrap_or(&value).to_string());
            }
        }
    }
    lines
}

/// Rustdoc `# Heading` sections, each a `section` tag whose `arg` is the heading
/// and whose text is the body. Port of v5 `parse_rust_sections`.
fn doc_sections(text: &str, strings: &mut Strings) -> Vec<DocTag> {
    let mut out: Vec<(String, Vec<&str>)> = Vec::new();
    for line in text.lines() {
        if let Some(rest) = line.trim_start().strip_prefix("# ") {
            out.push((rest.trim().to_string(), Vec::new()));
        } else if let Some((_, body)) = out.last_mut() {
            body.push(line);
        }
    }
    out.into_iter()
        .map(|(heading, body)| DocTag {
            tag: strings.intern("section"),
            arg: Some(strings.intern(&heading)),
            text: strings.intern(body.join("\n").trim()),
        })
        .collect()
}

