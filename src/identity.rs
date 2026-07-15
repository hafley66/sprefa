//! Canonical, versioned identities for extraction ownership and public facts.
//!
//! This module is deliberately storage-independent. It defines the Slice A
//! contract only; extraction and SQLite ownership tables are wired later.

use std::fmt;

const MAGIC: &[u8; 6] = b"SPRFID";
const ENCODING_VERSION: u16 = 1;
const DOMAIN_OWNER: u8 = 0x01;
const DOMAIN_FACT: u8 = 0x02;
const DOMAIN_REPO: u8 = 0x03;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OwnerId(pub [u8; 16]);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FactId(pub [u8; 16]);

/// Persistent repository identity stored in root state. This is not the
/// process-local `spine::RepoId(u32)` and is never inferred from a basename.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RepoIdentity(pub [u8; 16]);

impl RepoIdentity {
    pub fn from_bytes(bytes: [u8; 16]) -> Self { Self(bytes) }

    /// Deterministically seed a persistent identity from a registry-controlled
    /// unique key. The key is exact UTF-8; its stability/uniqueness is the
    /// registry's contract, not path or URL guesswork in this module.
    pub fn from_persistent_key(key: &str) -> Result<Self, IdentityError> {
        if key.is_empty() { return Err(IdentityError::EmptyRepoKey); }
        let mut bytes = prefix(DOMAIN_REPO);
        encode_len_bytes(&mut bytes, key.as_bytes())?;
        Ok(Self(Blake3IdHasher.hash(&bytes)))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResolvedCoordinate {
    Work,
    GitSha1([u8; 20]),
    GitSha256([u8; 32]),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NormalizedRepoPath(String);

impl NormalizedRepoPath {
    pub fn new(path: &str) -> Result<Self, IdentityError> {
        if path.as_bytes().contains(&0) { return Err(IdentityError::PathContainsNul); }
        let bytes = path.as_bytes();
        let windows_drive_path = bytes.len() >= 2
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':';
        if path.starts_with('/') || windows_drive_path {
            return Err(IdentityError::AbsolutePath(path.into()));
        }
        if path.contains('\\') { return Err(IdentityError::BackslashPath(path.into())); }

        let mut parts: Vec<&str> = Vec::new();
        for part in path.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    if parts.pop().is_none() {
                        return Err(IdentityError::PathEscapesRoot(path.into()));
                    }
                }
                _ => parts.push(part),
            }
        }
        if parts.is_empty() { return Err(IdentityError::EmptyPath); }
        let normalized = parts.join("/");
        u32::try_from(normalized.len())
            .map_err(|_| IdentityError::ValueTooLarge(normalized.len()))?;
        Ok(Self(normalized))
    }

    pub fn from_utf8(bytes: &[u8]) -> Result<Self, IdentityError> {
        let path = std::str::from_utf8(bytes).map_err(|_| IdentityError::NonUtf8Path)?;
        Self::new(path)
    }

    pub fn as_str(&self) -> &str { &self.0 }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnerKey {
    pub repo: RepoIdentity,
    pub coordinate: ResolvedCoordinate,
    pub path: NormalizedRepoPath,
    pub family_id: u32,
    pub extractor_schema_version: u32,
}

impl OwnerKey {
    pub fn id(&self) -> OwnerId { self.id_with(&Blake3IdHasher) }

    pub fn id_with(&self, hasher: &dyn IdHasher) -> OwnerId {
        OwnerId(hasher.hash(&self.canonical_bytes()))
    }

    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = prefix(DOMAIN_OWNER);
        out.extend_from_slice(&self.repo.0);
        match &self.coordinate {
            ResolvedCoordinate::Work => out.push(0),
            ResolvedCoordinate::GitSha1(oid) => { out.push(1); out.extend_from_slice(oid); }
            ResolvedCoordinate::GitSha256(oid) => { out.push(2); out.extend_from_slice(oid); }
        }
        encode_path(&mut out, &self.path).expect("NormalizedRepoPath enforces the u32 length limit");
        out.extend_from_slice(&self.family_id.to_be_bytes());
        out.extend_from_slice(&self.extractor_schema_version.to_be_bytes());
        out
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CanonicalValue {
    Null,
    Bool(bool),
    Int(i64),
    Text(String),
    Blob(Vec<u8>),
    Path(NormalizedRepoPath),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FactKey {
    pub family_id: u32,
    pub extractor_schema_version: u32,
    pub relation_id: u32,
    pub relation_schema_version: u32,
    /// The relation's explicitly versioned logical key in declared order. It
    /// may be narrower than the public row (for example `df_node.id`); the full
    /// payload is compared separately when validating an existing FactId.
    pub semantic_key: Vec<CanonicalValue>,
}

impl FactKey {
    pub fn new(
        family_id: u32,
        extractor_schema_version: u32,
        relation_id: u32,
        relation_schema_version: u32,
        semantic_key: Vec<CanonicalValue>,
    ) -> Result<Self, IdentityError> {
        if semantic_key.iter().any(|v| matches!(v, CanonicalValue::Null)) {
            return Err(IdentityError::NullIdentityColumn);
        }
        if semantic_key.len() > u16::MAX as usize {
            return Err(IdentityError::TooManyIdentityColumns(semantic_key.len()));
        }
        let key = Self { family_id, extractor_schema_version, relation_id, relation_schema_version, semantic_key };
        // Validate all variable-width cells before admitting the key.
        key.canonical_bytes()?;
        Ok(key)
    }

    pub fn id(&self) -> FactId {
        self.id_with(&Blake3IdHasher).expect("FactKey::new validated canonical encoding")
    }

    pub fn id_with(&self, hasher: &dyn IdHasher) -> Result<FactId, IdentityError> {
        Ok(FactId(hasher.hash(&self.canonical_bytes()?)))
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, IdentityError> {
        if self.semantic_key.iter().any(|v| matches!(v, CanonicalValue::Null)) {
            return Err(IdentityError::NullIdentityColumn);
        }
        let count = u16::try_from(self.semantic_key.len())
            .map_err(|_| IdentityError::TooManyIdentityColumns(self.semantic_key.len()))?;
        let mut out = prefix(DOMAIN_FACT);
        out.extend_from_slice(&self.family_id.to_be_bytes());
        out.extend_from_slice(&self.extractor_schema_version.to_be_bytes());
        out.extend_from_slice(&self.relation_id.to_be_bytes());
        out.extend_from_slice(&self.relation_schema_version.to_be_bytes());
        out.extend_from_slice(&count.to_be_bytes());
        for value in &self.semantic_key { encode_value(&mut out, value)?; }
        Ok(out)
    }
}

pub trait IdHasher: Send + Sync {
    fn hash(&self, canonical: &[u8]) -> [u8; 16];
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Blake3IdHasher;

impl IdHasher for Blake3IdHasher {
    fn hash(&self, canonical: &[u8]) -> [u8; 16] {
        blake3::hash(canonical).as_bytes()[..16].try_into().expect("BLAKE3 is 32 bytes")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CollisionDisposition { Idempotent }

pub fn validate_owner_collision(
    id: OwnerId,
    stored: &OwnerKey,
    incoming: &OwnerKey,
) -> Result<CollisionDisposition, IdentityError> {
    validate_owner_collision_with(&Blake3IdHasher, id, stored, incoming)
}

pub fn validate_owner_collision_with(
    hasher: &dyn IdHasher,
    id: OwnerId,
    stored: &OwnerKey,
    incoming: &OwnerKey,
) -> Result<CollisionDisposition, IdentityError> {
    if stored.id_with(hasher) == id && incoming.id_with(hasher) == id && stored == incoming {
        Ok(CollisionDisposition::Idempotent)
    } else {
        Err(IdentityError::HashCollision { domain: "owner", id: id.0 })
    }
}

pub fn validate_fact_collision(
    id: FactId,
    stored_key: &FactKey,
    stored_payload: &[CanonicalValue],
    incoming_key: &FactKey,
    incoming_payload: &[CanonicalValue],
) -> Result<CollisionDisposition, IdentityError> {
    validate_fact_collision_with(
        &Blake3IdHasher,
        id,
        stored_key,
        stored_payload,
        incoming_key,
        incoming_payload,
    )
}

pub fn validate_fact_collision_with(
    hasher: &dyn IdHasher,
    id: FactId,
    stored_key: &FactKey,
    stored_payload: &[CanonicalValue],
    incoming_key: &FactKey,
    incoming_payload: &[CanonicalValue],
) -> Result<CollisionDisposition, IdentityError> {
    if stored_key.id_with(hasher)? == id
        && incoming_key.id_with(hasher)? == id
        && stored_key == incoming_key
        && stored_payload == incoming_payload
    {
        Ok(CollisionDisposition::Idempotent)
    } else {
        Err(IdentityError::HashCollision { domain: "fact", id: id.0 })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IdentityError {
    EmptyRepoKey,
    EmptyPath,
    NonUtf8Path,
    PathContainsNul,
    AbsolutePath(String),
    BackslashPath(String),
    PathEscapesRoot(String),
    NullIdentityColumn,
    TooManyIdentityColumns(usize),
    ValueTooLarge(usize),
    HashCollision { domain: &'static str, id: [u8; 16] },
}

impl fmt::Display for IdentityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyRepoKey => write!(f, "persistent repository key is empty"),
            Self::EmptyPath => write!(f, "repository-relative path is empty"),
            Self::NonUtf8Path => write!(f, "repository-relative path is not UTF-8"),
            Self::PathContainsNul => write!(f, "repository-relative path contains NUL"),
            Self::AbsolutePath(p) => write!(f, "repository-relative path is absolute: {p:?}"),
            Self::BackslashPath(p) => write!(f, "repository-relative path contains a backslash: {p:?}"),
            Self::PathEscapesRoot(p) => write!(f, "repository-relative path escapes root: {p:?}"),
            Self::NullIdentityColumn => write!(f, "NULL is not allowed in a fact identity column"),
            Self::TooManyIdentityColumns(n) => write!(f, "fact identity has {n} columns; maximum is {}", u16::MAX),
            Self::ValueTooLarge(n) => write!(f, "canonical value is {n} bytes; maximum is {}", u32::MAX),
            Self::HashCollision { domain, id } => write!(f, "forced {domain} identity collision at {}", hex16(id)),
        }
    }
}

impl std::error::Error for IdentityError {}

fn prefix(domain: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(32);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&ENCODING_VERSION.to_be_bytes());
    out.push(domain);
    out.push(0);
    out
}

fn encode_len_bytes(out: &mut Vec<u8>, bytes: &[u8]) -> Result<(), IdentityError> {
    let len = u32::try_from(bytes.len()).map_err(|_| IdentityError::ValueTooLarge(bytes.len()))?;
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(bytes);
    Ok(())
}

fn encode_path(out: &mut Vec<u8>, path: &NormalizedRepoPath) -> Result<(), IdentityError> {
    out.push(0x06);
    encode_len_bytes(out, path.as_str().as_bytes())
}

fn encode_value(out: &mut Vec<u8>, value: &CanonicalValue) -> Result<(), IdentityError> {
    match value {
        CanonicalValue::Null => out.push(0x00),
        CanonicalValue::Bool(false) => out.push(0x01),
        CanonicalValue::Bool(true) => out.push(0x02),
        CanonicalValue::Int(n) => { out.push(0x03); out.extend_from_slice(&n.to_be_bytes()); }
        CanonicalValue::Text(text) => { out.push(0x04); encode_len_bytes(out, text.as_bytes())?; }
        CanonicalValue::Blob(blob) => { out.push(0x05); encode_len_bytes(out, blob)?; }
        CanonicalValue::Path(path) => encode_path(out, path)?,
    }
    Ok(())
}

fn hex16(bytes: &[u8; 16]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(32);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy)]
    struct ConstantHasher([u8; 16]);
    impl IdHasher for ConstantHasher {
        fn hash(&self, _: &[u8]) -> [u8; 16] { self.0 }
    }

    fn owner(path: &str) -> OwnerKey {
        OwnerKey {
            repo: RepoIdentity::from_bytes([0x11; 16]),
            coordinate: ResolvedCoordinate::Work,
            path: NormalizedRepoPath::new(path).unwrap(),
            family_id: 7,
            extractor_schema_version: 3,
        }
    }

    fn fact(relation: u32, schema: u32, name: &str) -> FactKey {
        FactKey::new(7, 3, relation, schema, vec![
            CanonicalValue::Int(-2),
            CanonicalValue::Text(name.into()),
            CanonicalValue::Bool(true),
            CanonicalValue::Blob(vec![0, 255]),
            CanonicalValue::Path(NormalizedRepoPath::new("src/a.rs").unwrap()),
        ]).unwrap()
    }

    #[test]
    fn owner_golden_bytes_are_versioned_and_deterministic() {
        let key = owner("src/./engine//../lib.rs");
        let mut expected = b"SPRFID\0\x01\x01\0".to_vec();
        expected.extend_from_slice(&[0x11; 16]);
        expected.push(0); // WORK
        expected.extend_from_slice(&[0x06, 0, 0, 0, 10]);
        expected.extend_from_slice(b"src/lib.rs");
        expected.extend_from_slice(&7u32.to_be_bytes());
        expected.extend_from_slice(&3u32.to_be_bytes());
        assert_eq!(key.canonical_bytes(), expected);
        assert_eq!(key.id(), key.id());
    }

    #[test]
    fn fact_golden_bytes_cover_every_value_kind() {
        let key = fact(9, 4, "é");
        let bytes = key.canonical_bytes().unwrap();
        assert_eq!(&bytes[..10], b"SPRFID\0\x01\x02\0");
        assert_eq!(&bytes[10..26], &[0,0,0,7, 0,0,0,3, 0,0,0,9, 0,0,0,4]);
        assert_eq!(&bytes[26..28], &[0, 5]);
        assert!(bytes.windows(7).any(|w| w == [0x04, 0, 0, 0, 2, 0xc3, 0xa9]));
        assert_eq!(key.id(), key.id());
    }

    #[test]
    fn owner_identity_excludes_content_and_separates_coordinates() {
        let key = owner("src/a.rs");
        let content_a = blake3::hash(b"old");
        let content_b = blake3::hash(b"new");
        assert_ne!(content_a, content_b);
        assert_eq!(key.id(), key.clone().id());
        let mut committed = key.clone();
        committed.coordinate = ResolvedCoordinate::GitSha1([9; 20]);
        assert_ne!(key.id(), committed.id());
    }

    #[test]
    fn fact_domain_relation_and_schema_are_separate() {
        let a = fact(9, 4, "same");
        assert_ne!(a.id().0, owner("src/a.rs").id().0);
        assert_ne!(a.id(), fact(10, 4, "same").id());
        assert_ne!(a.id(), fact(9, 5, "same").id());
        let mut family = a.clone(); family.family_id += 1;
        assert_ne!(a.id(), family.id());
        let mut extractor = a.clone(); extractor.extractor_schema_version += 1;
        assert_ne!(a.id(), extractor.id());
    }

    #[test]
    fn null_identity_is_rejected() {
        assert_eq!(
            FactKey::new(1, 1, 1, 1, vec![CanonicalValue::Null]).unwrap_err(),
            IdentityError::NullIdentityColumn,
        );
    }

    #[test]
    fn paths_normalize_lexically_and_reject_ambiguous_inputs() {
        assert_eq!(NormalizedRepoPath::new("src//./x/../lib.rs").unwrap().as_str(), "src/lib.rs");
        assert_eq!(NormalizedRepoPath::new("../x").unwrap_err(), IdentityError::PathEscapesRoot("../x".into()));
        assert!(matches!(NormalizedRepoPath::new("/x"), Err(IdentityError::AbsolutePath(_))));
        assert!(matches!(NormalizedRepoPath::new("C:/x"), Err(IdentityError::AbsolutePath(_))));
        assert!(matches!(NormalizedRepoPath::new("C:x"), Err(IdentityError::AbsolutePath(_))));
        assert!(matches!(NormalizedRepoPath::new("c:"), Err(IdentityError::AbsolutePath(_))));
        assert!(matches!(NormalizedRepoPath::new("a\\b"), Err(IdentityError::BackslashPath(_))));
        assert!(matches!(NormalizedRepoPath::new("\\\\server\\share"), Err(IdentityError::BackslashPath(_))));
        assert!(matches!(NormalizedRepoPath::new("\\\\?\\C:\\x"), Err(IdentityError::BackslashPath(_))));
        assert_eq!(NormalizedRepoPath::new("a\0b").unwrap_err(), IdentityError::PathContainsNul);
        assert_eq!(NormalizedRepoPath::from_utf8(&[0xff]).unwrap_err(), IdentityError::NonUtf8Path);
        assert_eq!(NormalizedRepoPath::new("é.rs").unwrap().as_str(), "é.rs");
    }

    #[test]
    fn forced_owner_and_fact_collisions_bail_but_identical_is_idempotent() {
        let hasher = ConstantHasher([0x5a; 16]);
        let owner_a = owner("src/a.rs");
        let owner_b = owner("src/b.rs");
        let oid = owner_a.id_with(&hasher);
        assert_eq!(oid, owner_b.id_with(&hasher));
        assert_eq!(validate_owner_collision_with(&hasher, oid, &owner_a, &owner_a), Ok(CollisionDisposition::Idempotent));
        assert!(matches!(validate_owner_collision_with(&hasher, oid, &owner_a, &owner_b), Err(IdentityError::HashCollision { domain: "owner", .. })));
        assert!(matches!(validate_owner_collision(OwnerId([0; 16]), &owner_a, &owner_a), Err(IdentityError::HashCollision { domain: "owner", .. })));

        let fact_a = fact(1, 1, "a");
        let fact_b = fact(1, 1, "b");
        let fid = fact_a.id_with(&hasher).unwrap();
        assert_eq!(fid, fact_b.id_with(&hasher).unwrap());
        let payload_a = [CanonicalValue::Text("payload-a".into()), CanonicalValue::Null];
        let payload_b = [CanonicalValue::Text("payload-b".into()), CanonicalValue::Null];
        assert_eq!(
            validate_fact_collision_with(&hasher, fid, &fact_a, &payload_a, &fact_a, &payload_a),
            Ok(CollisionDisposition::Idempotent),
        );
        assert!(matches!(
            validate_fact_collision_with(&hasher, fid, &fact_a, &payload_a, &fact_b, &payload_a),
            Err(IdentityError::HashCollision { domain: "fact", .. })
        ));
        assert!(matches!(
            validate_fact_collision_with(&hasher, fid, &fact_a, &payload_a, &fact_a, &payload_b),
            Err(IdentityError::HashCollision { domain: "fact", .. })
        ));
        assert!(matches!(
            validate_fact_collision(FactId([0; 16]), &fact_a, &payload_a, &fact_a, &payload_a),
            Err(IdentityError::HashCollision { domain: "fact", .. })
        ));
    }
}
