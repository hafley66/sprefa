use proc_macro2::LineColumn;
use syn::{Item, UseTree, spanned::Spanned};

use sprefa_extract::{kind, ExtractContext, Extractor, RawRef};

const EXTENSIONS: &[&str] = &["rs"];

pub struct RsExtractor;

impl Extractor for RsExtractor {
    fn extensions(&self) -> &[&str] {
        EXTENSIONS
    }

    fn extract(&self, source: &[u8], _path: &str, _ctx: &ExtractContext) -> Vec<RawRef> {
        let Ok(source_text) = std::str::from_utf8(source) else {
            return vec![];
        };
        let Ok(file) = syn::parse_file(source_text) else {
            return vec![];
        };
        let offsets = line_offsets(source_text);
        let mut refs = Vec::new();
        extract_items(&file.items, &offsets, &mut refs);
        refs
    }
}

// ── span helpers ─────────────────────────────────────────────────────────────

fn line_offsets(source: &str) -> Vec<usize> {
    let mut offsets = vec![0]; // line 1 starts at byte 0
    for (i, b) in source.bytes().enumerate() {
        if b == b'\n' {
            offsets.push(i + 1);
        }
    }
    offsets
}

fn to_byte(offsets: &[usize], lc: LineColumn) -> u32 {
    let line_start = offsets.get(lc.line - 1).copied().unwrap_or(0);
    (line_start + lc.column) as u32
}

fn span_of(offsets: &[usize], span: proc_macro2::Span) -> (u32, u32) {
    (to_byte(offsets, span.start()), to_byte(offsets, span.end()))
}

// ── item extraction ──────────────────────────────────────────────────────────

fn extract_items(items: &[Item], offsets: &[usize], refs: &mut Vec<RawRef>) {
    for item in items {
        match item {
            Item::Use(u) => {
                flatten_use_tree(&u.tree, &String::new(), None, offsets, refs);
            }
            Item::Mod(m) => {
                let (s, e) = span_of(offsets, m.ident.span());
                // Extract #[path = "..."] attribute if present.
                let path_attr = extract_path_attr(&m.attrs);
                refs.push(RawRef {
                    value: m.ident.to_string(),
                    span_start: s,
                    span_end: e,
                    kind: kind::RS_MOD.into(),
                    rule_name: "rs".into(),
                    is_path: false,
                    parent_key: None,
                    node_path: path_attr,
                });
                if let Some((_, inner)) = &m.content {
                    extract_items(inner, offsets, refs);
                }
            }
            Item::Fn(f) => push_declare(refs, &f.sig.ident, offsets),
            Item::Struct(s) => push_declare(refs, &s.ident, offsets),
            Item::Enum(e) => push_declare(refs, &e.ident, offsets),
            Item::Union(u) => push_declare(refs, &u.ident, offsets),
            Item::Type(t) => push_declare(refs, &t.ident, offsets),
            Item::Const(c) => push_declare(refs, &c.ident, offsets),
            Item::Static(s) => push_declare(refs, &s.ident, offsets),
            Item::Trait(t) => {
                push_declare(refs, &t.ident, offsets);
                for item in &t.items {
                    match item {
                        syn::TraitItem::Fn(f) => push_declare(refs, &f.sig.ident, offsets),
                        syn::TraitItem::Type(t) => push_declare(refs, &t.ident, offsets),
                        syn::TraitItem::Const(c) => push_declare(refs, &c.ident, offsets),
                        _ => {}
                    }
                }
            }
            Item::Impl(i) => {
                for item in &i.items {
                    match item {
                        syn::ImplItem::Fn(f) => push_declare(refs, &f.sig.ident, offsets),
                        syn::ImplItem::Type(t) => push_declare(refs, &t.ident, offsets),
                        syn::ImplItem::Const(c) => push_declare(refs, &c.ident, offsets),
                        _ => {}
                    }
                }
            }
            Item::ExternCrate(e) => {
                let (s, end) = span_of(offsets, e.ident.span());
                refs.push(RawRef {
                    value: e.ident.to_string(),
                    span_start: s,
                    span_end: end,
                    kind: kind::DEP_NAME.into(),
                    rule_name: "rs".into(),
                    is_path: false,
                    parent_key: None,
                    node_path: None,
                });
            }
            _ => {}
        }
    }
}

fn push_declare(refs: &mut Vec<RawRef>, ident: &syn::Ident, offsets: &[usize]) {
    let (s, e) = span_of(offsets, ident.span());
    refs.push(RawRef {
        value: ident.to_string(),
        span_start: s,
        span_end: e,
        kind: kind::RS_DECLARE.into(),
        rule_name: "rs".into(),
        is_path: false,
        parent_key: None,
        node_path: None,
    });
}

/// Extract the value of `#[path = "..."]` from a list of attributes, if present.
fn extract_path_attr(attrs: &[syn::Attribute]) -> Option<String> {
    for attr in attrs {
        if attr.path().is_ident("path") {
            if let syn::Meta::NameValue(nv) = &attr.meta {
                if let syn::Expr::Lit(syn::ExprLit { lit: syn::Lit::Str(s), .. }) = &nv.value {
                    return Some(s.value());
                }
            }
        }
    }
    None
}

// ── use-tree flattening ──────────────────────────────────────────────────────

fn flatten_use_tree(
    tree: &UseTree,
    prefix: &str,
    prefix_start: Option<proc_macro2::Span>,
    offsets: &[usize],
    refs: &mut Vec<RawRef>,
) {
    match tree {
        UseTree::Path(p) => {
            let new_prefix = if prefix.is_empty() {
                p.ident.to_string()
            } else {
                format!("{}::{}", prefix, p.ident)
            };
            let start = prefix_start.unwrap_or_else(|| p.ident.span());
            flatten_use_tree(&p.tree, &new_prefix, Some(start), offsets, refs);
        }
        UseTree::Name(n) => {
            let ident_str = n.ident.to_string();
            // `use std::io::{self}` -- `self` means import the parent path directly
            let value = if ident_str == "self" {
                if prefix.is_empty() { return; } // `use self;` alone is meaningless
                prefix.to_string()
            } else if prefix.is_empty() {
                ident_str
            } else {
                format!("{}::{}", prefix, n.ident)
            };
            let start = prefix_start.unwrap_or_else(|| n.ident.span());
            refs.push(RawRef {
                value,
                span_start: to_byte(offsets, start.start()),
                span_end: to_byte(offsets, n.ident.span().end()),
                kind: kind::RS_USE.into(),
                rule_name: "rs".into(),
                is_path: false,
                parent_key: None,
                node_path: None,
            });
        }
        UseTree::Rename(r) => {
            // use foo::Bar as Baz -- emit the original path, span covers original name
            let value = if prefix.is_empty() {
                r.ident.to_string()
            } else {
                format!("{}::{}", prefix, r.ident)
            };
            let start = prefix_start.unwrap_or_else(|| r.ident.span());
            refs.push(RawRef {
                value,
                span_start: to_byte(offsets, start.start()),
                span_end: to_byte(offsets, r.ident.span().end()),
                kind: kind::RS_USE.into(),
                rule_name: "rs".into(),
                is_path: false,
                parent_key: None,
                node_path: None,
            });
        }
        UseTree::Glob(g) => {
            let value = if prefix.is_empty() {
                "*".to_string()
            } else {
                format!("{}::*", prefix)
            };
            let start = prefix_start.unwrap_or_else(|| g.star_token.span());
            refs.push(RawRef {
                value,
                span_start: to_byte(offsets, start.start()),
                span_end: to_byte(offsets, g.star_token.span().end()),
                kind: kind::RS_USE.into(),
                rule_name: "rs".into(),
                is_path: false,
                parent_key: None,
                node_path: None,
            });
        }
        UseTree::Group(g) => {
            for item in &g.items {
                flatten_use_tree(item, prefix, prefix_start, offsets, refs);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extract(src: &str) -> Vec<RawRef> {
        RsExtractor.extract(src.as_bytes(), "src/lib.rs", &ExtractContext::default())
    }

    #[test]
    fn simple_use() {
        let refs = extract("use std::collections::HashMap;");
        insta::assert_yaml_snapshot!(refs);
    }

    #[test]
    fn grouped_use() {
        let refs = extract("use std::{io, fmt::Display};");
        insta::assert_yaml_snapshot!(refs);
    }

    #[test]
    fn nested_grouped_use() {
        let refs = extract("use std::{io::{self, Write}, fmt};");
        insta::assert_yaml_snapshot!(refs);
    }

    #[test]
    fn glob_use() {
        let refs = extract("use std::collections::*;");
        insta::assert_yaml_snapshot!(refs);
    }

    #[test]
    fn use_rename() {
        let refs = extract("use std::io::Result as IoResult;");
        insta::assert_yaml_snapshot!(refs);
    }

    #[test]
    fn mod_declarations() {
        let refs = extract("mod foo;\nmod bar;");
        insta::assert_yaml_snapshot!(refs);
    }

    #[test]
    fn fn_declarations() {
        let refs = extract("pub fn hello() {}\nfn private() {}");
        insta::assert_yaml_snapshot!(refs);
    }

    #[test]
    fn struct_and_enum() {
        let refs = extract("struct Foo {}\nenum Bar { A, B }");
        insta::assert_yaml_snapshot!(refs);
    }

    #[test]
    fn trait_with_items() {
        let refs = extract("trait MyTrait {\n    fn method(&self);\n    type Output;\n}");
        insta::assert_yaml_snapshot!(refs);
    }

    #[test]
    fn impl_items() {
        let refs = extract("struct Foo;\nimpl Foo {\n    fn bar(&self) {}\n    fn baz() {}\n}");
        insta::assert_yaml_snapshot!(refs);
    }

    #[test]
    fn inline_mod_recurses() {
        let refs = extract("mod inner {\n    fn hidden() {}\n}");
        insta::assert_yaml_snapshot!(refs);
    }

    #[test]
    fn span_points_at_use_path() {
        let src = "use std::collections::HashMap;";
        let refs = extract(src);
        let r = refs.iter().find(|r| r.value == "std::collections::HashMap").unwrap();
        let slice = &src[r.span_start as usize..r.span_end as usize];
        assert_eq!(slice, "std::collections::HashMap");
    }

    #[test]
    fn span_points_at_fn_ident() {
        let src = "pub fn my_function() {}";
        let refs = extract(src);
        let r = refs.iter().find(|r| r.value == "my_function").unwrap();
        let slice = &src[r.span_start as usize..r.span_end as usize];
        assert_eq!(slice, "my_function");
    }

    #[test]
    fn extern_crate() {
        let refs = extract("extern crate serde;");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].value, "serde");
        assert_eq!(refs[0].kind, kind::DEP_NAME);
    }

    // ── edge cases ────────────────────────────────────────────────────────

    #[test]
    fn empty_file() {
        let refs = extract("");
        assert!(refs.is_empty());
    }

    #[test]
    fn whitespace_only_file() {
        let refs = extract("   \n\n  \t  \n");
        assert!(refs.is_empty());
    }

    #[test]
    fn deeply_nested_use_group() {
        let refs = extract("use a::{b::{c::{D, E}}, f};");
        let values: Vec<&str> = refs.iter().map(|r| r.value.as_str()).collect();
        assert!(values.contains(&"a::b::c::D"));
        assert!(values.contains(&"a::b::c::E"));
        assert!(values.contains(&"a::f"));
        assert_eq!(refs.len(), 3);
    }

    #[test]
    fn use_self_in_group() {
        // use std::io::{self, Read} -- self means import std::io itself
        let refs = extract("use std::io::{self, Read};");
        let values: Vec<&str> = refs.iter().map(|r| r.value.as_str()).collect();
        assert!(values.contains(&"std::io"));       // from self
        assert!(values.contains(&"std::io::Read"));  // normal
        assert_eq!(refs.len(), 2);
    }

    #[test]
    fn pub_crate_fn() {
        // pub(crate) should still extract the declaration
        let refs = extract("pub(crate) fn internal_fn() {}");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].value, "internal_fn");
        assert_eq!(refs[0].kind, kind::RS_DECLARE);
    }

    #[test]
    fn async_fn() {
        let refs = extract("pub async fn do_work() {}");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].value, "do_work");
        assert_eq!(refs[0].kind, kind::RS_DECLARE);
    }

    #[test]
    fn const_and_static() {
        let refs = extract("const MAX: u32 = 100;\nstatic COUNTER: u32 = 0;");
        let values: Vec<&str> = refs.iter().map(|r| r.value.as_str()).collect();
        assert!(values.contains(&"MAX"));
        assert!(values.contains(&"COUNTER"));
        assert_eq!(refs.len(), 2);
    }

    #[test]
    fn type_alias() {
        let refs = extract("type Result<T> = std::result::Result<T, MyError>;");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].value, "Result");
        assert_eq!(refs[0].kind, kind::RS_DECLARE);
    }

    #[test]
    fn union_type() {
        let refs = extract("union MyUnion { f1: u32, f2: f32 }");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].value, "MyUnion");
        assert_eq!(refs[0].kind, kind::RS_DECLARE);
    }

    #[test]
    fn impl_with_trait() {
        // Items inside `impl Trait for Struct` should be extracted
        let refs = extract("struct Foo;\ntrait Bar { fn baz(&self); }\nimpl Bar for Foo { fn baz(&self) {} }");
        let values: Vec<&str> = refs.iter()
            .filter(|r| r.kind == kind::RS_DECLARE)
            .map(|r| r.value.as_str())
            .collect();
        assert!(values.contains(&"Foo"));
        assert!(values.contains(&"Bar"));
        // baz appears twice: once in trait, once in impl
        assert_eq!(values.iter().filter(|v| **v == "baz").count(), 2);
    }

    #[test]
    fn nested_mod_with_uses() {
        let refs = extract(
            "mod outer {\n    use std::io;\n    mod inner {\n        use std::fmt;\n        fn helper() {}\n    }\n}",
        );
        let mods: Vec<&str> = refs.iter()
            .filter(|r| r.kind == kind::RS_MOD)
            .map(|r| r.value.as_str())
            .collect();
        assert!(mods.contains(&"outer"));
        assert!(mods.contains(&"inner"));

        let uses: Vec<&str> = refs.iter()
            .filter(|r| r.kind == kind::RS_USE)
            .map(|r| r.value.as_str())
            .collect();
        assert!(uses.contains(&"std::io"));
        assert!(uses.contains(&"std::fmt"));

        assert!(refs.iter().any(|r| r.kind == kind::RS_DECLARE && r.value == "helper"));
    }

    #[test]
    fn use_rename_in_group() {
        // use std::{io::Error as IoError, fmt::Display as Disp};
        let refs = extract("use std::{io::Error as IoError, fmt::Display as Disp};");
        let values: Vec<&str> = refs.iter().map(|r| r.value.as_str()).collect();
        // Rename emits the original path, not the alias
        assert!(values.contains(&"std::io::Error"));
        assert!(values.contains(&"std::fmt::Display"));
        assert_eq!(refs.len(), 2);
    }

    #[test]
    fn glob_in_group() {
        let refs = extract("use std::{io::*, fmt::*};");
        let values: Vec<&str> = refs.iter().map(|r| r.value.as_str()).collect();
        assert!(values.contains(&"std::io::*"));
        assert!(values.contains(&"std::fmt::*"));
    }

    #[test]
    fn many_items_in_impl() {
        let refs = extract(
            "struct S;\nimpl S {\n    fn a() {}\n    fn b() {}\n    fn c() {}\n    type T = u32;\n    const N: u32 = 0;\n}",
        );
        let decl_values: Vec<&str> = refs.iter()
            .filter(|r| r.kind == kind::RS_DECLARE)
            .map(|r| r.value.as_str())
            .collect();
        assert!(decl_values.contains(&"S"));
        assert!(decl_values.contains(&"a"));
        assert!(decl_values.contains(&"b"));
        assert!(decl_values.contains(&"c"));
        assert!(decl_values.contains(&"T"));
        assert!(decl_values.contains(&"N"));
    }

    #[test]
    fn invalid_utf8_returns_empty() {
        let bytes: &[u8] = &[0xFF, 0xFE, 0x00, 0x00];
        let refs = RsExtractor.extract(bytes, "src/lib.rs", &ExtractContext::default());
        assert!(refs.is_empty());
    }

    #[test]
    fn use_crate_self() {
        // `use crate::self` -- `self` on bare `crate` means import crate itself, which is odd
        // but syntactically representable. The prefix is "crate", self means the prefix.
        // Actually `use crate::{self}` is the valid form.
        let refs = extract("use crate::{self};");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].value, "crate");
        assert_eq!(refs[0].kind, kind::RS_USE);
    }

    #[test]
    fn span_accuracy_on_grouped_use() {
        let src = "use std::{io, fmt};";
        let refs = extract(src);
        let io_ref = refs.iter().find(|r| r.value == "std::io").unwrap();
        let slice = &src[io_ref.span_start as usize..io_ref.span_end as usize];
        // Span starts at "std" and ends at "io"
        assert_eq!(slice, "std::{io");
    }

    #[test]
    fn extern_crate_with_rename() {
        let refs = extract("extern crate alloc as my_alloc;");
        // extern crate rename -- syn parses e.ident as "alloc"
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].value, "alloc");
        assert_eq!(refs[0].kind, kind::DEP_NAME);
    }

    // ── #[path] attribute extraction ─────────────────────────────────────

    #[test]
    fn mod_with_path_attribute() {
        let refs = extract("#[path = \"custom/location.rs\"]\nmod renamed;");
        let m = refs.iter().find(|r| r.kind == kind::RS_MOD).unwrap();
        assert_eq!(m.value, "renamed");
        assert_eq!(m.node_path.as_deref(), Some("custom/location.rs"));
    }

    #[test]
    fn mod_without_path_attribute() {
        let refs = extract("mod normal;");
        let m = refs.iter().find(|r| r.kind == kind::RS_MOD).unwrap();
        assert_eq!(m.value, "normal");
        assert_eq!(m.node_path, None);
    }

    #[test]
    fn mod_with_cfg_and_path_attributes() {
        let refs = extract("#[cfg(test)]\n#[path = \"test_helpers.rs\"]\nmod helpers;");
        let m = refs.iter().find(|r| r.kind == kind::RS_MOD).unwrap();
        assert_eq!(m.value, "helpers");
        assert_eq!(m.node_path.as_deref(), Some("test_helpers.rs"));
    }

    #[test]
    fn inline_mod_with_path_attribute() {
        let refs = extract("#[path = \"alt\"]\nmod stuff {\n    fn inner() {}\n}");
        let m = refs.iter().find(|r| r.kind == kind::RS_MOD).unwrap();
        assert_eq!(m.node_path.as_deref(), Some("alt"));
        // inner fn still extracted
        assert!(refs.iter().any(|r| r.value == "inner" && r.kind == kind::RS_DECLARE));
    }
}
