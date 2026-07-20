//! Content-addressed logical types for a compiled engine plan.
//!
//! This module deliberately does not change the runtime `Value` or SQLite
//! representation. It gives the compiler stable logical identities while
//! preserving the current `Col` storage choices.

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::hash::BuildHasher;

use crate::ast::{Col, Type};

const MAGIC: &[u8; 9] = b"SPRFTYPE\0";
const ENCODING_VERSION: u16 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TypeId(pub [u8; 16]);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeNode {
    Base(Type),
    Named {
        name: String,
        parent: TypeId,
    },
    /// Variant order is declaration order and therefore identity-bearing.
    Enum {
        name: String,
        variants: Vec<String>,
    },
    Apply {
        constructor: TypeId,
        args: Vec<TypeId>,
    },
    /// Members are sorted by `TypeId` and deduplicated before hashing.
    Union {
        members: Vec<TypeId>,
    },
    Unknown {
        spelling: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorageClass {
    I64,
    InternedText,
    RawText,
    Blob,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypedColumn {
    pub name: String,
    pub logical: TypeId,
    pub storage: StorageClass,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TypeArena {
    nodes: BTreeMap<TypeId, TypeNode>,
}

impl TypeArena {
    pub fn get(&self, id: TypeId) -> Option<&TypeNode> {
        self.nodes.get(&id)
    }
    pub fn len(&self) -> usize {
        self.nodes.len()
    }
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (TypeId, &TypeNode)> {
        self.nodes.iter().map(|(id, node)| (*id, node))
    }
}

#[derive(Clone, Debug, Default)]
pub struct TypeArenaBuilder {
    nodes: BTreeMap<TypeId, TypeNode>,
}

impl TypeArenaBuilder {
    pub fn intern(&mut self, node: TypeNode) -> Result<TypeId, TypeArenaError> {
        let node = canonicalize(node);
        let encoded = canonical_bytes(&node)?;
        let id = TypeId(hash128(&encoded));
        match self.nodes.get(&id) {
            Some(existing) if existing == &node => Ok(id),
            Some(existing) => Err(TypeArenaError::HashCollision {
                id,
                existing: existing.clone(),
                incoming: node,
            }),
            None => {
                self.nodes.insert(id, node);
                Ok(id)
            }
        }
    }

    pub fn lower_col<R: BrandResolver + ?Sized>(
        &mut self,
        col: &Col,
        brands: &R,
    ) -> Result<TypedColumn, TypeArenaError> {
        let logical = match col.brand.as_deref() {
            Some(brand) => match brands.resolve_brand(brand) {
                Some(id) => id,
                None => self.intern(TypeNode::Unknown {
                    spelling: brand.to_string(),
                })?,
            },
            None => self.intern(TypeNode::Base(col.ty))?,
        };
        Ok(TypedColumn {
            name: col.name.clone(),
            logical,
            storage: storage_class(col),
        })
    }

    pub fn finish(self) -> TypeArena {
        TypeArena { nodes: self.nodes }
    }
}

pub trait BrandResolver {
    fn resolve_brand(&self, name: &str) -> Option<TypeId>;
}

impl<F> BrandResolver for F
where
    F: Fn(&str) -> Option<TypeId>,
{
    fn resolve_brand(&self, name: &str) -> Option<TypeId> {
        self(name)
    }
}

impl<S: BuildHasher> BrandResolver for HashMap<String, TypeId, S> {
    fn resolve_brand(&self, name: &str) -> Option<TypeId> {
        self.get(name).copied()
    }
}

impl BrandResolver for BTreeMap<String, TypeId> {
    fn resolve_brand(&self, name: &str) -> Option<TypeId> {
        self.get(name).copied()
    }
}

pub fn storage_class(col: &Col) -> StorageClass {
    if col.ty == Type::Int {
        StorageClass::I64
    } else if col.interned() {
        StorageClass::InternedText
    } else {
        StorageClass::RawText
    }
}

fn canonicalize(node: TypeNode) -> TypeNode {
    match node {
        TypeNode::Union { mut members } => {
            members.sort_unstable();
            members.dedup();
            TypeNode::Union { members }
        }
        other => other,
    }
}

fn canonical_bytes(node: &TypeNode) -> Result<Vec<u8>, TypeArenaError> {
    let mut out = Vec::with_capacity(64);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&ENCODING_VERSION.to_be_bytes());
    match node {
        TypeNode::Base(ty) => {
            out.push(1);
            out.push(base_tag(*ty));
        }
        TypeNode::Named { name, parent } => {
            out.push(2);
            put_str(&mut out, name)?;
            out.extend_from_slice(&parent.0);
        }
        TypeNode::Enum { name, variants } => {
            out.push(3);
            put_str(&mut out, name)?;
            put_count(&mut out, variants.len())?;
            for variant in variants {
                put_str(&mut out, variant)?;
            }
        }
        TypeNode::Apply { constructor, args } => {
            out.push(4);
            out.extend_from_slice(&constructor.0);
            put_count(&mut out, args.len())?;
            for arg in args {
                out.extend_from_slice(&arg.0);
            }
        }
        TypeNode::Union { members } => {
            out.push(5);
            put_count(&mut out, members.len())?;
            for member in members {
                out.extend_from_slice(&member.0);
            }
        }
        TypeNode::Unknown { spelling } => {
            out.push(6);
            put_str(&mut out, spelling)?;
        }
    }
    Ok(out)
}

fn base_tag(ty: Type) -> u8 {
    match ty {
        Type::Text => 1,
        Type::Int => 2,
        Type::Path => 3,
        Type::File => 4,
        Type::Dir => 5,
        Type::Repo => 6,
        Type::Rev => 7,
    }
}

fn put_count(out: &mut Vec<u8>, count: usize) -> Result<(), TypeArenaError> {
    let count = u32::try_from(count).map_err(|_| TypeArenaError::EncodingTooLarge)?;
    out.extend_from_slice(&count.to_be_bytes());
    Ok(())
}

fn put_str(out: &mut Vec<u8>, value: &str) -> Result<(), TypeArenaError> {
    put_count(out, value.len())?;
    out.extend_from_slice(value.as_bytes());
    Ok(())
}

fn hash128(bytes: &[u8]) -> [u8; 16] {
    let digest = blake3::hash(bytes);
    let mut id = [0; 16];
    id.copy_from_slice(&digest.as_bytes()[..16]);
    id
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeArenaError {
    EncodingTooLarge,
    HashCollision {
        id: TypeId,
        existing: TypeNode,
        incoming: TypeNode,
    },
}

impl fmt::Display for TypeArenaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypeArenaError::EncodingTooLarge => {
                f.write_str("type encoding exceeds the u32 canonical length limit")
            }
            TypeArenaError::HashCollision { id, .. } => write!(f, "type ID collision at {id:?}"),
        }
    }
}

impl std::error::Error for TypeArenaError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn col(name: &str, ty: Type, raw: bool) -> Col {
        Col {
            name: name.into(),
            ty,
            brand: None,
            raw,
            coord: false,
        }
    }

    #[test]
    fn golden_base_encoding_and_determinism() {
        let bytes = canonical_bytes(&TypeNode::Base(Type::Text)).unwrap();
        assert_eq!(bytes, b"SPRFTYPE\0\0\x01\x01\x01");
        let mut a = TypeArenaBuilder::default();
        let mut b = TypeArenaBuilder::default();
        assert_eq!(
            a.intern(TypeNode::Base(Type::Text)).unwrap(),
            b.intern(TypeNode::Base(Type::Text)).unwrap()
        );
    }

    #[test]
    fn structurally_unequal_nodes_have_distinct_ids() {
        let mut types = TypeArenaBuilder::default();
        let text = types.intern(TypeNode::Base(Type::Text)).unwrap();
        let named = types
            .intern(TypeNode::Named {
                name: "Text".into(),
                parent: text,
            })
            .unwrap();
        let unknown = types
            .intern(TypeNode::Unknown {
                spelling: "Text".into(),
            })
            .unwrap();
        assert_ne!(text, named);
        assert_ne!(named, unknown);
        assert_ne!(text, unknown);
    }

    #[test]
    fn interning_deduplicates_equal_nodes() {
        let mut types = TypeArenaBuilder::default();
        let a = types.intern(TypeNode::Base(Type::Path)).unwrap();
        let b = types.intern(TypeNode::Base(Type::Path)).unwrap();
        assert_eq!(a, b);
        let arena = types.finish();
        assert_eq!(arena.len(), 1);
        assert_eq!(arena.get(a), Some(&TypeNode::Base(Type::Path)));
    }

    #[test]
    fn union_order_and_duplicates_are_not_identity_bearing() {
        let mut types = TypeArenaBuilder::default();
        let text = types.intern(TypeNode::Base(Type::Text)).unwrap();
        let int = types.intern(TypeNode::Base(Type::Int)).unwrap();
        let a = types
            .intern(TypeNode::Union {
                members: vec![text, int, text],
            })
            .unwrap();
        let b = types
            .intern(TypeNode::Union {
                members: vec![int, text],
            })
            .unwrap();
        assert_eq!(a, b);
        let arena = types.finish();
        let TypeNode::Union { members } = arena.get(a).unwrap() else {
            panic!("not a union")
        };
        assert_eq!(members.len(), 2);
        assert!(members.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn current_columns_keep_their_storage_contract() {
        for ty in [
            Type::Text,
            Type::Path,
            Type::File,
            Type::Dir,
            Type::Repo,
            Type::Rev,
        ] {
            let interned = col("x", ty, false);
            let raw = col("x", ty, true);
            assert_eq!(storage_class(&interned), StorageClass::InternedText);
            assert_eq!(storage_class(&raw), StorageClass::RawText);
            assert_eq!(interned.sql(), "INTEGER");
            assert_eq!(raw.sql(), "TEXT");
        }
        for raw in [false, true] {
            let int = col("n", Type::Int, raw);
            assert_eq!(storage_class(&int), StorageClass::I64);
            assert_eq!(int.sql(), "INTEGER");
        }
    }

    #[test]
    fn enum_declaration_order_is_preserved_and_identity_bearing() {
        let mut types = TypeArenaBuilder::default();
        let a = types
            .intern(TypeNode::Enum {
                name: "severity".into(),
                variants: vec!["warn".into(), "error".into()],
            })
            .unwrap();
        let b = types
            .intern(TypeNode::Enum {
                name: "severity".into(),
                variants: vec!["error".into(), "warn".into()],
            })
            .unwrap();
        assert_ne!(a, b);
        let arena = types.finish();
        let TypeNode::Enum { variants, .. } = arena.get(a).unwrap() else {
            panic!("not an enum")
        };
        assert_eq!(variants, &["warn", "error"]);
    }

    #[test]
    fn lowering_resolves_known_brands_and_preserves_unknown_spelling() {
        let mut types = TypeArenaBuilder::default();
        let text = types.intern(TypeNode::Base(Type::Text)).unwrap();
        let severity = types
            .intern(TypeNode::Named {
                name: "severity".into(),
                parent: text,
            })
            .unwrap();
        let brands = HashMap::from([("severity".to_string(), severity)]);
        let known = Col {
            name: "level".into(),
            ty: Type::Text,
            brand: Some("severity".into()),
            raw: false,
            coord: false,
        };
        let missing = Col {
            name: "value".into(),
            ty: Type::Text,
            brand: Some("future_type".into()),
            raw: false,
            coord: false,
        };
        assert_eq!(types.lower_col(&known, &brands).unwrap().logical, severity);
        let unknown = types.lower_col(&missing, &brands).unwrap().logical;
        let arena = types.finish();
        assert_eq!(
            arena.get(unknown),
            Some(&TypeNode::Unknown {
                spelling: "future_type".into()
            })
        );
    }
}
