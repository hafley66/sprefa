//! Row dump plus the syntactic-shape census for the rust type leg: the
//! diagnosis tool the recall-grind arcs (68_rust_receivers .. 79_rust_variant_payload)
//! were ground against. It is for the NEXT grind — join a missed oracle row to
//! the syntactic position our walk never visits. Ignored by default for the same
//! reason `ratchet_recall.rs` is: the corpus is a machine-local checkout.

mod bench;

use std::collections::BTreeSet;

/// `RUST_TYPE_DUMP=<path>` names the output tsv of our own projected rows.
#[test]
#[ignore = "local corpora only; run with RUST_TYPE_DUMP=<path>"]
fn dump_rust_type_rows() {
    let Ok(out) = std::env::var("RUST_TYPE_DUMP") else {
        panic!("set RUST_TYPE_DUMP=<path>");
    };
    let corpus = bench::corpus("rust");
    assert!(
        corpus.root.is_dir(),
        "corpus root {} missing",
        corpus.root.display()
    );
    let measurement = bench::run("rust", bench::Tier::Checker);
    let body: Vec<&str> = measurement
        .forms
        .type_edges
        .iter()
        .map(String::as_str)
        .collect();
    std::fs::write(&out, body.join("\n") + "\n").unwrap();
    println!("wrote {} rows to {out}", body.len());
    if let Ok(calls) = std::env::var("RUST_CALL_DUMP") {
        let body: Vec<&str> = measurement.forms.call.iter().map(String::as_str).collect();
        std::fs::write(&calls, body.join("\n") + "\n").unwrap();
        println!("wrote {} call rows to {calls}", body.len());
    }
}

// ── the shape census ────────────────────────────────────────────────────────
//
// A port of the oracle's `owner_of` (`plans/extract-bench-2026-08-29/
// ra_ide_probe/main.rs:130-147`): the nearest enclosing Struct/Enum/Union/
// Trait/TypeAlias/Impl names the owner and marks the row a type declaration;
// a nearer Fn/Const/Static takes the ownership away. Every type path under
// such an owner is emitted with the SYNTACTIC POSITION that produced it, so
// an oracle row we miss can be joined to the position our candidate walk
// never visits.

/// `SHAPE_CENSUS=<path>` names the output tsv: file, owner, dst, root, leaf,
/// qualified. `root` is the top-level declaration position the reference sits
/// under, `leaf` its position inside that type expression, `qualified` whether
/// the path carries more than one segment (ours joins every segment into ONE
/// candidate name, so a qualified path resolves against no bare-name index).
#[test]
#[ignore = "local corpora only; run with SHAPE_CENSUS=<path>"]
fn dump_shape_census() {
    let Ok(out) = std::env::var("SHAPE_CENSUS") else {
        panic!("set SHAPE_CENSUS=<path>");
    };
    let corpus = bench::corpus("rust");
    assert!(
        corpus.root.is_dir(),
        "corpus root {} missing",
        corpus.root.display()
    );
    let files = bench::enumerate(&corpus);
    let root = corpus.root.to_str().unwrap().to_string();
    let mut rows: BTreeSet<String> = BTreeSet::new();
    for path in &files {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        let Ok(parsed) = syn::parse_file(&text) else {
            continue;
        };
        let rel = path
            .strip_prefix(&corpus.root)
            .map(|rel| rel.to_string_lossy().to_string())
            .unwrap_or_else(|_| path.to_string_lossy().to_string());
        let mut walk = Census {
            rel,
            rows: &mut rows,
        };
        for item in &parsed.items {
            walk.item(item);
        }
    }
    let body: Vec<&str> = rows.iter().map(String::as_str).collect();
    std::fs::write(&out, body.join("\n") + "\n").unwrap();
    println!("wrote {} shape rows to {out} (root {root})", body.len());
}

struct Census<'a> {
    rel: String,
    rows: &'a mut BTreeSet<String>,
}

impl Census<'_> {
    fn push(&mut self, owner: &str, dst: &str, root: &str, leaf: &str, qualified: bool) {
        if owner.is_empty() || dst.is_empty() {
            return;
        }
        self.rows.insert(format!(
            "{}\t{owner}\t{dst}\t{root}\t{leaf}\t{}",
            self.rel, qualified as u8
        ));
    }

    /// Every name a type expression references, head segments and generic
    /// arguments alike. `root` names the declaration position the whole
    /// expression sits under; `leaf` the position inside it.
    fn ty(&mut self, owner: &str, ty: &syn::Type, root: &str, leaf: &str) {
        match ty {
            syn::Type::Array(t) => self.ty(owner, &t.elem, root, leaf),
            syn::Type::Group(t) => self.ty(owner, &t.elem, root, leaf),
            syn::Type::Paren(t) => self.ty(owner, &t.elem, root, leaf),
            syn::Type::Ptr(t) => self.ty(owner, &t.elem, root, leaf),
            syn::Type::Reference(t) => self.ty(owner, &t.elem, root, leaf),
            syn::Type::Slice(t) => self.ty(owner, &t.elem, root, leaf),
            syn::Type::Tuple(t) => {
                for elem in &t.elems {
                    self.ty(owner, elem, root, "tuple-elem");
                }
            }
            syn::Type::BareFn(t) => {
                for input in &t.inputs {
                    self.ty(owner, &input.ty, root, "bare-fn");
                }
                if let syn::ReturnType::Type(_, inner) = &t.output {
                    self.ty(owner, inner, root, "bare-fn");
                }
            }
            syn::Type::ImplTrait(t) => {
                for bound in &t.bounds {
                    self.bound(owner, bound, root, "impl-trait");
                }
            }
            syn::Type::TraitObject(t) => {
                for bound in &t.bounds {
                    self.bound(owner, bound, root, "dyn");
                }
            }
            syn::Type::Path(t) => {
                if let Some(qself) = &t.qself {
                    self.ty(owner, &qself.ty, root, "qself");
                }
                self.path(owner, &t.path, root, leaf);
            }
            syn::Type::Macro(_) => self.push(owner, "<macro>", root, "macro", false),
            _ => {}
        }
    }

    fn path(&mut self, owner: &str, path: &syn::Path, root: &str, leaf: &str) {
        let last = path.segments.len().saturating_sub(1);
        let qualified = path.segments.len() > 1;
        for (pos, seg) in path.segments.iter().enumerate() {
            let name = seg.ident.to_string();
            if pos == last {
                self.push(owner, &name, root, leaf, qualified);
            } else {
                self.push(owner, &name, root, "path-prefix", qualified);
            }
            if let syn::PathArguments::AngleBracketed(args) = &seg.arguments {
                for arg in &args.args {
                    match arg {
                        syn::GenericArgument::Type(t) => self.ty(owner, t, root, "generic-arg"),
                        syn::GenericArgument::AssocType(t) => {
                            self.ty(owner, &t.ty, root, "assoc-binding")
                        }
                        syn::GenericArgument::Constraint(c) => {
                            for bound in &c.bounds {
                                self.bound(owner, bound, root, "assoc-constraint");
                            }
                        }
                        _ => {}
                    }
                }
            }
            if let syn::PathArguments::Parenthesized(args) = &seg.arguments {
                for input in &args.inputs {
                    self.ty(owner, input, root, "fn-trait-arg");
                }
                if let syn::ReturnType::Type(_, inner) = &args.output {
                    self.ty(owner, inner, root, "fn-trait-ret");
                }
            }
        }
    }

    fn bound(&mut self, owner: &str, bound: &syn::TypeParamBound, root: &str, leaf: &str) {
        if let syn::TypeParamBound::Trait(t) = bound {
            self.path(owner, &t.path, root, leaf);
        }
    }

    fn generics(&mut self, owner: &str, generics: &syn::Generics) {
        for param in &generics.params {
            if let syn::GenericParam::Type(t) = param {
                for bound in &t.bounds {
                    self.bound(owner, bound, "bound", "head");
                }
                if let Some(default) = &t.default {
                    self.ty(owner, default, "generic-param-default", "head");
                }
            }
        }
        if let Some(where_clause) = &generics.where_clause {
            for pred in &where_clause.predicates {
                if let syn::WherePredicate::Type(t) = pred {
                    self.ty(owner, &t.bounded_ty, "where-bounded-ty", "head");
                    for bound in &t.bounds {
                        self.bound(owner, bound, "bound", "head");
                    }
                }
            }
        }
    }

    fn fields(&mut self, owner: &str, fields: &syn::Fields, root: &str) {
        for field in fields.iter() {
            self.ty(owner, &field.ty, root, "head");
        }
    }

    /// Items nested in a fn/const/static body keep their own ownership; the
    /// bodies themselves own nothing the typedecl oracle records.
    fn block(&mut self, block: &syn::Block) {
        for stmt in &block.stmts {
            if let syn::Stmt::Item(item) = stmt {
                self.item(item);
            }
        }
    }

    fn item(&mut self, item: &syn::Item) {
        match item {
            syn::Item::Struct(s) => {
                let owner = s.ident.to_string();
                self.generics(&owner, &s.generics);
                self.fields(&owner, &s.fields, "field");
            }
            syn::Item::Union(u) => {
                let owner = u.ident.to_string();
                self.generics(&owner, &u.generics);
                self.fields(&owner, &syn::Fields::Named(u.fields.clone()), "field");
            }
            syn::Item::Enum(e) => {
                let owner = e.ident.to_string();
                self.generics(&owner, &e.generics);
                for variant in &e.variants {
                    self.fields(&owner, &variant.fields, "variant-payload");
                }
            }
            syn::Item::Trait(t) => {
                let owner = t.ident.to_string();
                self.generics(&owner, &t.generics);
                for bound in &t.supertraits {
                    self.bound(&owner, bound, "supertrait", "head");
                }
                for member in &t.items {
                    match member {
                        // An assoc type is an `ast::TypeAlias`, so `owner_of`
                        // names IT, not the enclosing trait.
                        syn::TraitItem::Type(assoc) => {
                            let owner = assoc.ident.to_string();
                            self.generics(&owner, &assoc.generics);
                            for bound in &assoc.bounds {
                                self.bound(&owner, bound, "assoc-type", "head");
                            }
                            if let Some((_, default)) = &assoc.default {
                                self.ty(&owner, default, "assoc-type", "head");
                            }
                        }
                        syn::TraitItem::Fn(f) => {
                            if let Some(block) = &f.default {
                                self.block(block);
                            }
                        }
                        _ => {}
                    }
                }
            }
            syn::Item::Type(alias) => {
                let owner = alias.ident.to_string();
                self.generics(&owner, &alias.generics);
                self.ty(&owner, &alias.ty, "alias-rhs", "head");
            }
            syn::Item::Impl(imp) => {
                let Some(owner) = self_ty_head(&imp.self_ty) else {
                    return;
                };
                self.ty(&owner, &imp.self_ty, "impl-self-ty", "head");
                self.generics(&owner, &imp.generics);
                if let Some((_, path, _)) = &imp.trait_ {
                    self.path(&owner, path, "impl-trait", "head");
                }
                for member in &imp.items {
                    match member {
                        syn::ImplItem::Type(assoc) => {
                            let owner = assoc.ident.to_string();
                            self.generics(&owner, &assoc.generics);
                            self.ty(&owner, &assoc.ty, "assoc-type", "head");
                        }
                        syn::ImplItem::Fn(f) => self.block(&f.block),
                        _ => {}
                    }
                }
            }
            syn::Item::Mod(m) => {
                if let Some((_, inner)) = &m.content {
                    for nested in inner {
                        self.item(nested);
                    }
                }
            }
            syn::Item::Fn(f) => self.block(&f.block),
            _ => {}
        }
    }
}

/// The impl self-type's head name, the owner spelling the oracle's
/// `impl_self_name` produces.
fn self_ty_head(ty: &syn::Type) -> Option<String> {
    match ty {
        syn::Type::Group(t) => self_ty_head(&t.elem),
        syn::Type::Paren(t) => self_ty_head(&t.elem),
        syn::Type::Ptr(t) => self_ty_head(&t.elem),
        syn::Type::Reference(t) => self_ty_head(&t.elem),
        syn::Type::Path(t) => t.path.segments.last().map(|seg| seg.ident.to_string()),
        _ => None,
    }
}
