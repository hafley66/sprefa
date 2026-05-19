//! `sprefa-extract` — language source → type/entity IR via tree-sitter.
//!
//! Boundary: this crate knows LANGUAGES, not sprf. It produces a plain
//! `TyEntity` IR and nothing else. The `reify` op (in v4) owns IR→sprf
//! text + the in-file macro region. sem-core and the archive `RawRef`
//! are ref/entity-graph shaped and lack the field/type layer — that
//! layer is the entire reason this crate exists.
//!
//! Trait shape mirrors the archive `sprefa_extract::Extractor`
//! (`extensions()` + an `extract*` method). No compilers, no `syn`:
//! tree-sitter only, so every language goes through one mechanism.
//!
//! Scope of this first slice: Rust `struct` → its named fields and
//! their type text. Enums/traits/impls/imports are later kinds; the
//! IR already has room for them (`kind` is open).

use tree_sitter::{Node, Parser};

/// A field's type, classified just enough for the sprf emitter to pick
/// `t.<prim>` vs a named-type reference. No resolution here — that is a
/// later unit (SCIP key / tier-3 scope). `Named` is whatever text the
/// grammar gave (`Foo`, `Vec<u8>`, `&str`); the emitter/typer decides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TyRef {
    Prim(String),
    Named(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TyField {
    pub name: String,
    pub ty: TyRef,
}

/// One declared type. `kind` is open ("struct" today; "enum"/"trait"/…
/// later). `fields` is empty for fieldless kinds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TyEntity {
    pub name: String,
    pub kind: String,
    pub fields: Vec<TyField>,
}

/// Per-language extractor. One impl per grammar. Mirrors the archive
/// `Extractor` trait's `extensions()` + `extract*` shape.
pub trait LangExtract: Send + Sync {
    fn extensions(&self) -> &[&str];
    fn extract_types(&self, source: &str) -> Vec<TyEntity>;
}

const RUST_PRIMS: &[&str] = &[
    "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize", "f32",
    "f64", "bool", "char", "str",
];

pub struct RustExtract;

impl LangExtract for RustExtract {
    fn extensions(&self) -> &[&str] {
        &["rs"]
    }

    fn extract_types(&self, source: &str) -> Vec<TyEntity> {
        let mut parser = Parser::new();
        if parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .is_err()
        {
            return Vec::new();
        }
        let Some(tree) = parser.parse(source, None) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        collect_structs(tree.root_node(), source.as_bytes(), &mut out);
        out
    }
}

fn classify(text: &str) -> TyRef {
    if RUST_PRIMS.contains(&text) {
        TyRef::Prim(text.to_string())
    } else {
        TyRef::Named(text.to_string())
    }
}

/// Walk the whole tree (structs can nest in mods/fns); collect every
/// `struct_item` with a `field_declaration_list` body.
fn collect_structs(node: Node, src: &[u8], out: &mut Vec<TyEntity>) {
    if node.kind() == "struct_item" {
        if let Some(name) = node.child_by_field_name("name") {
            let nm = name.utf8_text(src).unwrap_or("").to_string();
            let mut fields = Vec::new();
            if let Some(body) = node.child_by_field_name("body") {
                let mut c = body.walk();
                for fd in body.children(&mut c) {
                    if fd.kind() != "field_declaration" {
                        continue;
                    }
                    let fname = fd
                        .child_by_field_name("name")
                        .and_then(|x| x.utf8_text(src).ok())
                        .unwrap_or("")
                        .to_string();
                    let ftype = fd
                        .child_by_field_name("type")
                        .and_then(|x| x.utf8_text(src).ok())
                        .unwrap_or("")
                        .to_string();
                    if !fname.is_empty() {
                        fields.push(TyField {
                            name: fname,
                            ty: classify(&ftype),
                        });
                    }
                }
            }
            out.push(TyEntity {
                name: nm,
                kind: "struct".to_string(),
                fields,
            });
        }
    }
    let mut c = node.walk();
    for ch in node.children(&mut c) {
        collect_structs(ch, src, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_struct_to_ty_entity() {
        let got = RustExtract.extract_types("pub struct Point { x: i64, y: i64 }\n");
        assert_eq!(
            got,
            vec![TyEntity {
                name: "Point".into(),
                kind: "struct".into(),
                fields: vec![
                    TyField {
                        name: "x".into(),
                        ty: TyRef::Prim("i64".into())
                    },
                    TyField {
                        name: "y".into(),
                        ty: TyRef::Prim("i64".into())
                    },
                ],
            }]
        );
    }

    #[test]
    fn named_type_and_nested_struct() {
        let got = RustExtract.extract_types(
            "mod m { struct Wrap { inner: Point, n: u32 } }\nstruct Point { v: f64 }\n",
        );
        let names: Vec<&str> = got.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"Wrap") && names.contains(&"Point"), "{got:?}");
        let wrap = got.iter().find(|e| e.name == "Wrap").unwrap();
        assert_eq!(wrap.fields[0].ty, TyRef::Named("Point".into()));
        assert_eq!(wrap.fields[1].ty, TyRef::Prim("u32".into()));
    }
}
