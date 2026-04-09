use proc_macro2::LineColumn;
use syn::{Item, UseTree, spanned::Spanned};
use syn::visit::Visit;

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
        let mut gc: u32 = 0;
        extract_items(&file.items, &offsets, &mut refs, &mut gc);
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

fn extract_items(items: &[Item], offsets: &[usize], refs: &mut Vec<RawRef>, gc: &mut u32) {
    for item in items {
        match item {
            Item::Use(u) => {
                flatten_use_tree(&u.tree, &String::new(), None, offsets, refs, gc);
            }
            Item::Mod(m) => {
                let (s, e) = span_of(offsets, m.ident.span());
                let path_attr = extract_path_attr(&m.attrs);
                refs.push(RawRef {
                    value: m.ident.to_string(),
                    span_start: s,
                    span_end: e,
                    kind: kind::RS_MOD.into(),
                    rule_name: kind::RS_MOD.into(),
                    is_path: false,
                    parent_key: None,
                    node_path: path_attr,
                    scan: None,
                    group: { *gc += 1; Some(*gc - 1) },
                });
                if let Some((_, inner)) = &m.content {
                    extract_items(inner, offsets, refs, gc);
                }
            }
            Item::Fn(f) => push_declare(refs, &f.sig.ident, offsets, gc),
            Item::Struct(s) => push_declare(refs, &s.ident, offsets, gc),
            Item::Enum(e) => push_declare(refs, &e.ident, offsets, gc),
            Item::Union(u) => push_declare(refs, &u.ident, offsets, gc),
            Item::Type(t) => push_declare(refs, &t.ident, offsets, gc),
            Item::Const(c) => push_declare(refs, &c.ident, offsets, gc),
            Item::Static(s) => push_declare(refs, &s.ident, offsets, gc),
            Item::Trait(t) => {
                push_declare(refs, &t.ident, offsets, gc);
                for item in &t.items {
                    match item {
                        syn::TraitItem::Fn(f) => push_declare(refs, &f.sig.ident, offsets, gc),
                        syn::TraitItem::Type(t) => push_declare(refs, &t.ident, offsets, gc),
                        syn::TraitItem::Const(c) => push_declare(refs, &c.ident, offsets, gc),
                        _ => {}
                    }
                }
            }
            Item::Impl(i) => {
                for item in &i.items {
                    match item {
                        syn::ImplItem::Fn(f) => push_declare(refs, &f.sig.ident, offsets, gc),
                        syn::ImplItem::Type(t) => push_declare(refs, &t.ident, offsets, gc),
                        syn::ImplItem::Const(c) => push_declare(refs, &c.ident, offsets, gc),
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
                    rule_name: kind::DEP_NAME.into(),
                    is_path: false,
                    parent_key: None,
                    node_path: None,
                    scan: None,
                    group: { *gc += 1; Some(*gc - 1) },
                });
            }
            _ => {}
        }
    }
}

fn push_declare(refs: &mut Vec<RawRef>, ident: &syn::Ident, offsets: &[usize], gc: &mut u32) {
    let (s, e) = span_of(offsets, ident.span());
    refs.push(RawRef {
        value: ident.to_string(),
        span_start: s,
        span_end: e,
        kind: kind::RS_DECLARE.into(),
        rule_name: kind::RS_DECLARE.into(),
        is_path: false,
        parent_key: None,
        node_path: None,
        scan: None,
        group: { *gc += 1; Some(*gc - 1) },
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
    gc: &mut u32,
) {
    match tree {
        UseTree::Path(p) => {
            let ident_str = p.ident.to_string();
            // `self::` means "this module" -- transparent, keep current prefix.
            // `super::` means "parent module" -- can't resolve statically, strip it.
            // Both can repeat: `super::super::foo` strips all leading super:: segments.
            let (new_prefix, new_start) = if ident_str == "self" || ident_str == "super" {
                (prefix.to_string(), prefix_start)
            } else if prefix.is_empty() {
                (ident_str, Some(p.ident.span()))
            } else {
                (format!("{}::{}", prefix, p.ident), prefix_start.or_else(|| Some(p.ident.span())))
            };
            let start = new_start.unwrap_or_else(|| p.ident.span());
            flatten_use_tree(&p.tree, &new_prefix, Some(start), offsets, refs, gc);
        }
        UseTree::Name(n) => {
            let ident_str = n.ident.to_string();
            let value = if ident_str == "self" {
                if prefix.is_empty() { return; }
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
                rule_name: kind::RS_USE.into(),
                is_path: false,
                parent_key: None,
                node_path: None,
                scan: None,
                group: { *gc += 1; Some(*gc - 1) },
            });
        }
        UseTree::Rename(r) => {
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
                rule_name: kind::RS_USE.into(),
                is_path: false,
                parent_key: None,
                node_path: None,
                scan: None,
                group: { *gc += 1; Some(*gc - 1) },
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
                rule_name: kind::RS_USE.into(),
                is_path: false,
                parent_key: None,
                node_path: None,
                scan: None,
                group: { *gc += 1; Some(*gc - 1) },
            });
        }
        UseTree::Group(g) => {
            for item in &g.items {
                flatten_use_tree(item, prefix, prefix_start, offsets, refs, gc);
            }
        }
    }
}

// ── syn-based rewriter ──────────────────────────────────────────────────────

/// Rewrite module references in Rust source after a module rename.
///
/// Parses source with syn, walks use trees and mod declarations, finds idents
/// matching `old_stem` in the correct path context, replaces them with `new_stem`.
///
/// `use_prefixes`: path segments that must precede `old_stem` in use trees.
///   e.g. `&["crate"]` matches `use crate::old_stem::Foo`
///   e.g. `&["sprefa_rules"]` matches `use sprefa_rules::old_stem::Foo`
///   Empty slice matches bare uses like `use old_stem::*` (relative paths).
///
/// `rewrite_mod_decl`: also rewrite `mod old_stem;` declarations.
///
/// Returns (rewritten_source, edit_count).
pub fn rewrite_module_refs(
    source: &str,
    old_stem: &str,
    new_stem: &str,
    use_prefixes: &[&str],
    rewrite_mod_decl: bool,
) -> (String, usize) {
    let file = match syn::parse_file(source) {
        Ok(f) => f,
        Err(_) => return (source.to_string(), 0),
    };
    let offsets = line_offsets(source);
    let mut spans: Vec<(usize, usize)> = Vec::new();

    // When rewriting mod decls, also match bare relative uses (e.g. `pub use types::*`)
    // since we're in the parent module where relative paths resolve to the child.
    let mut extended_prefixes: Vec<&str> = use_prefixes.to_vec();
    if rewrite_mod_decl {
        extended_prefixes.push("");
    }

    for item in &file.items {
        match item {
            Item::Use(u) => {
                collect_use_ident_spans(&u.tree, &[], old_stem, &extended_prefixes, &offsets, &mut spans);
            }
            Item::Mod(m) if rewrite_mod_decl => {
                if m.ident == old_stem {
                    let (s, e) = span_of(&offsets, m.ident.span());
                    spans.push((s as usize, e as usize));
                }
                if let Some((_, inner)) = &m.content {
                    collect_items_use_spans(inner, old_stem, &extended_prefixes, &offsets, &mut spans);
                }
            }
            Item::Mod(m) => {
                if let Some((_, inner)) = &m.content {
                    collect_items_use_spans(inner, old_stem, &extended_prefixes, &offsets, &mut spans);
                }
            }
            _ => {}
        }
    }

    // Walk the entire AST for inline qualified paths (fn sigs, struct fields,
    // impl blocks, where clauses, etc). Catches `crate::types::Foo` anywhere
    // that isn't a `use` statement.
    let mut visitor = PathVisitor {
        old_stem,
        use_prefixes: &extended_prefixes,
        offsets: &offsets,
        spans: &mut spans,
    };
    visitor.visit_file(&file);

    // Macro arguments are opaque TokenStreams to syn. Re-parse them with
    // proc_macro2 to find qualified paths inside matches!(), vec!(), etc.
    let mut macro_visitor = MacroTokenVisitor {
        old_stem,
        use_prefixes: &extended_prefixes,
        offsets: &offsets,
        spans: &mut spans,
    };
    macro_visitor.visit_file(&file);

    if spans.is_empty() {
        return (source.to_string(), 0);
    }

    // Deduplicate and sort descending so replacements don't shift offsets.
    spans.sort_unstable();
    spans.dedup();
    spans.reverse();

    let count = spans.len();
    let mut result = source.to_string();
    for (start, end) in &spans {
        result.replace_range(*start..*end, new_stem);
    }

    (result, count)
}

fn collect_items_use_spans(
    items: &[Item],
    old_stem: &str,
    use_prefixes: &[&str],
    offsets: &[usize],
    spans: &mut Vec<(usize, usize)>,
) {
    for item in items {
        match item {
            Item::Use(u) => {
                collect_use_ident_spans(&u.tree, &[], old_stem, use_prefixes, offsets, spans);
            }
            Item::Mod(m) => {
                if let Some((_, inner)) = &m.content {
                    collect_items_use_spans(inner, old_stem, use_prefixes, offsets, spans);
                }
            }
            _ => {}
        }
    }
}

/// Walk a use tree, tracking the path segments seen so far.
/// When we find an ident matching `old_stem` whose preceding path matches
/// one of the `use_prefixes`, record its byte span.
fn collect_use_ident_spans(
    tree: &UseTree,
    path_so_far: &[String],
    old_stem: &str,
    use_prefixes: &[&str],
    offsets: &[usize],
    spans: &mut Vec<(usize, usize)>,
) {
    match tree {
        UseTree::Path(p) => {
            let ident_str = p.ident.to_string();
            if ident_str == old_stem && prefix_matches(path_so_far, use_prefixes) {
                let (s, e) = span_of(offsets, p.ident.span());
                spans.push((s as usize, e as usize));
            }
            let mut next_path = path_so_far.to_vec();
            if ident_str != "self" && ident_str != "super" {
                next_path.push(ident_str);
            }
            collect_use_ident_spans(&p.tree, &next_path, old_stem, use_prefixes, offsets, spans);
        }
        UseTree::Name(n) => {
            if n.ident == old_stem && prefix_matches(path_so_far, use_prefixes) {
                let (s, e) = span_of(offsets, n.ident.span());
                spans.push((s as usize, e as usize));
            }
        }
        UseTree::Rename(r) => {
            if r.ident == old_stem && prefix_matches(path_so_far, use_prefixes) {
                let (s, e) = span_of(offsets, r.ident.span());
                spans.push((s as usize, e as usize));
            }
        }
        UseTree::Glob(_) => {}
        UseTree::Group(g) => {
            for item in &g.items {
                collect_use_ident_spans(item, path_so_far, old_stem, use_prefixes, offsets, spans);
            }
        }
    }
}

/// Check if the accumulated path matches any of the required prefixes.
/// An empty prefix slice means "match any prefix" (bare/relative uses).
fn prefix_matches(path_so_far: &[String], use_prefixes: &[&str]) -> bool {
    if use_prefixes.is_empty() {
        return true;
    }
    for pfx in use_prefixes {
        if pfx.is_empty() {
            if path_so_far.is_empty() {
                return true;
            }
            continue;
        }
        let pfx_segments: Vec<&str> = pfx.split("::").collect();
        if path_so_far.len() >= pfx_segments.len()
            && path_so_far[..pfx_segments.len()]
                .iter()
                .zip(&pfx_segments)
                .all(|(a, b)| a == b)
        {
            return true;
        }
    }
    false
}

/// Walks the entire syn AST to find inline qualified paths like
/// `crate::types::Foo` in fn signatures, struct fields, impl blocks, etc.
struct PathVisitor<'a> {
    old_stem: &'a str,
    use_prefixes: &'a [&'a str],
    offsets: &'a [usize],
    spans: &'a mut Vec<(usize, usize)>,
}

impl<'a, 'ast> Visit<'ast> for PathVisitor<'a> {
    fn visit_path(&mut self, path: &'ast syn::Path) {
        // Check if any segment matches old_stem with the right prefix.
        // e.g. for `crate::types::Rule`, segments are [crate, types, Rule].
        // With prefix ["crate"] and old_stem "types", segment index 1 matches.
        let segments: Vec<String> = path.segments.iter()
            .map(|s| s.ident.to_string())
            .collect();

        for (i, seg) in segments.iter().enumerate() {
            if seg != self.old_stem {
                continue;
            }
            // Check the prefix: segments before this one must match a use_prefix.
            let before: Vec<String> = segments[..i].to_vec();
            if prefix_matches(&before, self.use_prefixes) {
                let ident = &path.segments[i].ident;
                let (s, e) = span_of(self.offsets, ident.span());
                self.spans.push((s as usize, e as usize));
            }
        }

        // Continue visiting children (e.g. generic args like Foo<crate::types::Bar>).
        syn::visit::visit_path(self, path);
    }

    // For use items, delegate to collect_use_ident_spans which handles
    // grouped imports correctly. This catches `use` inside function bodies
    // that the top-level item walk misses. Dedup handles overlap.
    fn visit_item_use(&mut self, node: &'ast syn::ItemUse) {
        collect_use_ident_spans(&node.tree, &[], self.old_stem, self.use_prefixes, self.offsets, self.spans);
    }
}

/// Walks macro invocations, tokenizes their arguments, and finds qualified
/// paths like `crate::types::Foo` that syn's Visit can't see.
struct MacroTokenVisitor<'a> {
    old_stem: &'a str,
    use_prefixes: &'a [&'a str],
    offsets: &'a [usize],
    spans: &'a mut Vec<(usize, usize)>,
}

impl<'a, 'ast> Visit<'ast> for MacroTokenVisitor<'a> {
    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        scan_token_stream(&mac.tokens, self.old_stem, self.use_prefixes, self.offsets, self.spans);
        syn::visit::visit_macro(self, mac);
    }
}

/// Scan a proc_macro2 token stream for path-like sequences matching
/// `prefix::old_stem`. Token streams preserve span info, so we get
/// exact byte offsets without string guessing.
fn scan_token_stream(
    tokens: &proc_macro2::TokenStream,
    old_stem: &str,
    use_prefixes: &[&str],
    offsets: &[usize],
    spans: &mut Vec<(usize, usize)>,
) {
    // Collect tokens into a vec for lookahead.
    let toks: Vec<proc_macro2::TokenTree> = tokens.clone().into_iter().collect();

    // Walk looking for ident :: ident :: ident sequences.
    // Build up path segments, check for prefix::old_stem match.
    let mut i = 0;
    while i < toks.len() {
        // Try to parse a path starting at position i.
        if let proc_macro2::TokenTree::Ident(first) = &toks[i] {
            let mut segments: Vec<(String, proc_macro2::Span)> = vec![(first.to_string(), first.span())];
            let mut j = i + 1;
            // Consume :: ident pairs
            while j + 1 < toks.len() {
                if let proc_macro2::TokenTree::Punct(p) = &toks[j] {
                    if p.as_char() == ':' && p.spacing() == proc_macro2::Spacing::Joint {
                        if let Some(proc_macro2::TokenTree::Punct(p2)) = toks.get(j + 1) {
                            if p2.as_char() == ':' {
                                if let Some(proc_macro2::TokenTree::Ident(next)) = toks.get(j + 2) {
                                    segments.push((next.to_string(), next.span()));
                                    j += 3;
                                    continue;
                                }
                            }
                        }
                    }
                }
                break;
            }

            // Check if any segment is old_stem with correct prefix.
            for (si, (name, span)) in segments.iter().enumerate() {
                if name == old_stem {
                    let before: Vec<String> = segments[..si].iter().map(|(n, _)| n.clone()).collect();
                    if prefix_matches(&before, use_prefixes) {
                        let (s, e) = span_of(offsets, *span);
                        spans.push((s as usize, e as usize));
                    }
                }
            }

            i = j;
        } else if let proc_macro2::TokenTree::Group(g) = &toks[i] {
            // Recurse into groups (parens, brackets, braces inside macros).
            scan_token_stream(&g.stream(), old_stem, use_prefixes, offsets, spans);
            i += 1;
        } else {
            i += 1;
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

    // ── super:: / self:: prefix stripping ────────────────────────────────

    #[test]
    fn use_super_simple() {
        let refs = extract("use super::foo;");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].value, "foo");
        assert_eq!(refs[0].kind, kind::RS_USE);
    }

    #[test]
    fn use_self_simple() {
        let refs = extract("use self::bar;");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].value, "bar");
        assert_eq!(refs[0].kind, kind::RS_USE);
    }

    #[test]
    fn use_super_nested() {
        let refs = extract("use super::foo::Bar;");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].value, "foo::Bar");
        assert_eq!(refs[0].kind, kind::RS_USE);
    }

    #[test]
    fn use_self_nested() {
        let refs = extract("use self::util::helper;");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].value, "util::helper");
        assert_eq!(refs[0].kind, kind::RS_USE);
    }

    #[test]
    fn use_super_super_nested() {
        let refs = extract("use super::super::common::Shared;");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].value, "common::Shared");
        assert_eq!(refs[0].kind, kind::RS_USE);
    }

    #[test]
    fn use_super_in_group() {
        let refs = extract("use super::{foo, bar::Baz};");
        let values: Vec<&str> = refs.iter().map(|r| r.value.as_str()).collect();
        assert!(values.contains(&"foo"));
        assert!(values.contains(&"bar::Baz"));
        assert_eq!(refs.len(), 2);
    }

    #[test]
    fn use_self_glob() {
        let refs = extract("use self::module::*;");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].value, "module::*");
    }

    #[test]
    fn inline_mod_with_path_attribute() {
        let refs = extract("#[path = \"alt\"]\nmod stuff {\n    fn inner() {}\n}");
        let m = refs.iter().find(|r| r.kind == kind::RS_MOD).unwrap();
        assert_eq!(m.node_path.as_deref(), Some("alt"));
        // inner fn still extracted
        assert!(refs.iter().any(|r| r.value == "inner" && r.kind == kind::RS_DECLARE));
    }

    // ── rewrite_module_refs ──────────────────────────────────────────────

    #[test]
    fn rewrite_simple_use() {
        let src = "use crate::types::Foo;";
        let (out, n) = rewrite_module_refs(src, "types", "_0_types", &["crate"], false);
        assert_eq!(out, "use crate::_0_types::Foo;");
        assert_eq!(n, 1);
    }

    #[test]
    fn rewrite_grouped_use() {
        let src = "use crate::{types::Foo, ast};";
        let (out, n) = rewrite_module_refs(src, "types", "_0_types", &["crate"], false);
        assert_eq!(out, "use crate::{_0_types::Foo, ast};");
        assert_eq!(n, 1);
    }

    #[test]
    fn rewrite_multi_item_grouped_use() {
        let src = "use crate::{\n    ast, emit,\n    types::{AstSelector, LineMatcher, MatchDef, RuleSet, SelectStep},\n    walk,\n};";
        let (out, n) = rewrite_module_refs(src, "types", "_0_types", &["crate"], false);
        assert!(out.contains("_0_types::{AstSelector"), "got: {}", out);
        assert!(out.contains("ast, emit,"), "should not touch other items");
        assert_eq!(n, 1);
    }

    #[test]
    fn rewrite_mod_decl() {
        let src = "pub mod types;\npub mod ast;";
        let (out, n) = rewrite_module_refs(src, "types", "_0_types", &["crate"], true);
        assert_eq!(out, "pub mod _0_types;\npub mod ast;");
        assert_eq!(n, 1);
    }

    #[test]
    fn rewrite_mod_decl_skipped_when_false() {
        let src = "pub mod types;";
        let (out, n) = rewrite_module_refs(src, "types", "_0_types", &["crate"], false);
        assert_eq!(out, "pub mod types;");
        assert_eq!(n, 0);
    }

    #[test]
    fn rewrite_cross_crate() {
        let src = "use sprefa_rules::types::RuleSet;";
        let (out, n) = rewrite_module_refs(src, "types", "_0_types", &["sprefa_rules"], false);
        assert_eq!(out, "use sprefa_rules::_0_types::RuleSet;");
        assert_eq!(n, 1);
    }

    #[test]
    fn rewrite_multiple_prefixes() {
        let src = "use crate::types::Foo;\nuse sprefa_rules::types::Bar;";
        let (out, n) = rewrite_module_refs(src, "types", "_0_types", &["crate", "sprefa_rules"], false);
        assert!(out.contains("crate::_0_types::Foo"));
        assert!(out.contains("sprefa_rules::_0_types::Bar"));
        assert_eq!(n, 2);
    }

    #[test]
    fn rewrite_no_false_positive() {
        let src = "use other::types::Foo;";
        let (out, n) = rewrite_module_refs(src, "types", "_0_types", &["crate"], false);
        assert_eq!(out, src, "should not rewrite types under wrong prefix");
        assert_eq!(n, 0);
    }

    #[test]
    fn rewrite_pub_use_glob() {
        let src = "pub use types::*;";
        // Relative use (no crate:: prefix) -- empty prefix matches
        let (out, n) = rewrite_module_refs(src, "types", "_0_types", &[], false);
        assert_eq!(out, "pub use _0_types::*;");
        assert_eq!(n, 1);
    }

    #[test]
    fn rewrite_full_lib_rs() {
        let src = "pub mod ast;\npub mod emit;\npub mod types;\npub mod walk;\n\npub use types::*;";
        let (out, n) = rewrite_module_refs(src, "types", "_0_types", &["crate"], true);
        assert!(out.contains("pub mod _0_types;"), "mod decl rewritten");
        assert!(out.contains("pub use _0_types::*;"), "pub use rewritten -- got: {}", out);
        assert_eq!(n, 2);
    }

    // ── inline path rewriting (syn visitor) ─────────────────────────────

    #[test]
    fn rewrite_inline_path_in_fn_sig() {
        let src = "fn compile(r: &crate::types::Rule) -> Result<()> { todo!() }";
        let (out, n) = rewrite_module_refs(src, "types", "_0_types", &["crate"], false);
        assert_eq!(out, "fn compile(r: &crate::_0_types::Rule) -> Result<()> { todo!() }");
        assert_eq!(n, 1);
    }

    #[test]
    fn rewrite_inline_path_in_struct_field() {
        let src = "struct Foo { bar: crate::types::Bar }";
        let (out, n) = rewrite_module_refs(src, "types", "_0_types", &["crate"], false);
        assert_eq!(out, "struct Foo { bar: crate::_0_types::Bar }");
        assert_eq!(n, 1);
    }

    #[test]
    fn rewrite_inline_path_in_impl() {
        let src = "impl crate::types::Foo { fn new() -> Self { todo!() } }";
        let (out, n) = rewrite_module_refs(src, "types", "_0_types", &["crate"], false);
        assert_eq!(out, "impl crate::_0_types::Foo { fn new() -> Self { todo!() } }");
        assert_eq!(n, 1);
    }

    #[test]
    fn rewrite_inline_path_in_generic() {
        let src = "fn f() -> Vec<crate::types::Item> { todo!() }";
        let (out, n) = rewrite_module_refs(src, "types", "_0_types", &["crate"], false);
        assert_eq!(out, "fn f() -> Vec<crate::_0_types::Item> { todo!() }");
        assert_eq!(n, 1);
    }

    #[test]
    fn rewrite_inline_path_no_false_positive() {
        let src = "fn f(r: &other::types::Rule) {}";
        let (out, n) = rewrite_module_refs(src, "types", "_0_types", &["crate"], false);
        assert_eq!(out, src, "should not touch types under wrong prefix");
        assert_eq!(n, 0);
    }

    #[test]
    fn rewrite_inline_and_use_together() {
        let src = "use crate::types::Foo;\nfn f(r: &crate::types::Bar) {}";
        let (out, n) = rewrite_module_refs(src, "types", "_0_types", &["crate"], false);
        assert!(out.contains("use crate::_0_types::Foo;"), "use rewritten");
        assert!(out.contains("crate::_0_types::Bar"), "inline path rewritten");
        assert_eq!(n, 2);
    }

    #[test]
    fn rewrite_path_inside_matches_macro() {
        let src = r#"fn f(s: &Step) -> bool { matches!(s, crate::types::SelectStep::File { .. }) }"#;
        let (out, n) = rewrite_module_refs(src, "types", "_0_types", &["crate"], false);
        assert!(out.contains("crate::_0_types::SelectStep"), "macro path rewritten\n{}", out);
        assert_eq!(n, 1);
    }

    #[test]
    fn rewrite_path_inside_vec_macro() {
        let src = r#"fn f() -> Vec<Box<dyn Any>> { vec![Box::new(crate::types::Foo)] }"#;
        let (out, n) = rewrite_module_refs(src, "types", "_0_types", &["crate"], false);
        assert!(out.contains("crate::_0_types::Foo"), "vec! macro path rewritten\n{}", out);
        assert_eq!(n, 1);
    }

    #[test]
    fn rewrite_cross_crate_in_matches_macro() {
        let src = r#"fn f(s: &Step) -> bool { matches!(s, sprefa_rules::types::SelectStep::File { .. }) }"#;
        let (out, n) = rewrite_module_refs(src, "types", "_0_types", &["sprefa_rules"], false);
        assert!(out.contains("sprefa_rules::_0_types::SelectStep"), "cross-crate macro\n{}", out);
        assert_eq!(n, 1);
    }

    #[test]
    fn rewrite_where_clause() {
        let src = "fn f<T>() where T: crate::types::Trait {}";
        let (out, n) = rewrite_module_refs(src, "types", "_0_types", &["crate"], false);
        assert!(out.contains("crate::_0_types::Trait"), "where clause rewritten\n{}", out);
        assert_eq!(n, 1);
    }

    #[test]
    fn rewrite_use_inside_fn_body() {
        let src = "fn f() {\n    use crate::types::Foo;\n    let _ = Foo;\n}";
        let (out, n) = rewrite_module_refs(src, "types", "_0_types", &["crate"], false);
        assert!(out.contains("use crate::_0_types::Foo;"), "use inside fn rewritten\n{}", out);
        assert_eq!(n, 1);
    }
}
