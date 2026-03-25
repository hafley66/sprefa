use proc_macro2::LineColumn;
use syn::{Item, UseTree, spanned::Spanned};

use sprefa_extract::{Extractor, RawRef};
use sprefa_schema::RefKind;

const EXTENSIONS: &[&str] = &["rs"];

pub struct RsExtractor;

impl Extractor for RsExtractor {
    fn extensions(&self) -> &[&str] {
        EXTENSIONS
    }

    fn extract(&self, source: &[u8], _path: &str) -> Vec<RawRef> {
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
                refs.push(RawRef {
                    value: m.ident.to_string(),
                    span_start: s,
                    span_end: e,
                    kind: RefKind::RsMod,
                    is_path: false,
                    parent_key: None,
                    node_path: None,
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
                    kind: RefKind::DepName,
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
        kind: RefKind::RsDeclare,
        is_path: false,
        parent_key: None,
        node_path: None,
    });
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
                kind: RefKind::RsUse,
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
                kind: RefKind::RsUse,
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
                kind: RefKind::RsUse,
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
        RsExtractor.extract(src.as_bytes(), "src/lib.rs")
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
        assert_eq!(refs[0].kind, RefKind::DepName);
    }
}
