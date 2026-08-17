//! Flat import-side types for the CPG protobuf importer (issue
//! cpg-proto-importer). NOT extract's own node/edge families.

use std::fmt;

/// The report section 3 mapping subset, node half (17 of the schema's 44
/// `NodeType` protoIds); names match `cpg_edge_vocab.pl` `cpg_node/3` (docs-only there).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum CpgNodeKind {
    Method,
    MethodReturn,
    Call,
    Identifier,
    Literal,
    Local,
    Block,
    ControlStructure,
    Return,
    JumpTarget,
    MethodRef,
    TypeRef,
    FieldIdentifier,
    File,
    Type,
    TypeDecl,
    Member,
}

/// The report section 3 mapping subset, edge half (9 of the schema's 26
/// `EdgeType` protoIds). Variant names match `cpg_edge_vocab.pl` `cpg_edge/4`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum CpgEdgeKind {
    Ast,
    Cfg,
    Call,
    Argument,
    Receiver,
    Ref,
    ReachingDef,
    EvalType,
    Contains,
}

/// `NodeType` protoId -> our kind, io.shiftleft:codepropertygraph-protos_3:1.7.36.
/// Ids outside this table are a named decode stop, never a silent skip.
fn node_kind(id: i32) -> Option<CpgNodeKind> {
    Some(match id {
        1 => CpgNodeKind::Method,
        3 => CpgNodeKind::MethodReturn,
        15 => CpgNodeKind::Call,
        27 => CpgNodeKind::Identifier,
        8 => CpgNodeKind::Literal,
        23 => CpgNodeKind::Local,
        31 => CpgNodeKind::Block,
        339 => CpgNodeKind::ControlStructure,
        30 => CpgNodeKind::Return,
        340 => CpgNodeKind::JumpTarget,
        333 => CpgNodeKind::MethodRef,
        335 => CpgNodeKind::TypeRef,
        2_001_081 => CpgNodeKind::FieldIdentifier,
        38 => CpgNodeKind::File,
        45 => CpgNodeKind::Type,
        46 => CpgNodeKind::TypeDecl,
        9 => CpgNodeKind::Member,
        _ => return None,
    })
}

/// `EdgeType` protoId -> our kind, same release as `node_kind`.
fn edge_kind(id: i32) -> Option<CpgEdgeKind> {
    Some(match id {
        3 => CpgEdgeKind::Ast,
        19 => CpgEdgeKind::Cfg,
        6 => CpgEdgeKind::Call,
        156 => CpgEdgeKind::Argument,
        55 => CpgEdgeKind::Receiver,
        10 => CpgEdgeKind::Ref,
        137 => CpgEdgeKind::ReachingDef,
        21 => CpgEdgeKind::EvalType,
        28 => CpgEdgeKind::Contains,
        _ => return None,
    })
}

/// `PropertyValue`'s 13-variant oneof, flattened (report `ProtoGen.scala:56-101`).
/// `CpgProperty::name_id` stays the raw protoId; symbolic names are out of scope.
#[derive(Clone, Debug, PartialEq)]
pub enum CpgPropertyValue {
    Str(String),
    Bool(bool),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    StringList(Vec<String>),
    BoolList(Vec<bool>),
    IntList(Vec<i32>),
    LongList(Vec<i64>),
    FloatList(Vec<f32>),
    DoubleList(Vec<f64>),
    ContainedRefs {
        local_name: String,
        refs: Vec<i64>,
    },
    /// The oneof was absent on the wire (proto3 allows this).
    Absent,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CpgProperty {
    pub name_id: i32,
    pub value: CpgPropertyValue,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CpgNode {
    pub key: i64,
    pub kind: CpgNodeKind,
    pub properties: Vec<CpgProperty>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CpgEdge {
    pub src: i64,
    pub dst: i64,
    pub kind: CpgEdgeKind,
    pub properties: Vec<CpgProperty>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CpgImport {
    pub nodes: Vec<CpgNode>,
    pub edges: Vec<CpgEdge>,
}

/// Why a CpgStruct import failed. Both unknown-id variants carry the raw
/// protoId and the node key / edge endpoints so the stop is traceable.
#[derive(Debug)]
pub enum CpgImportError {
    Decode(prost::DecodeError),
    UnknownNodeType { key: i64, type_id: i32 },
    UnknownEdgeType { src: i64, dst: i64, type_id: i32 },
}

impl fmt::Display for CpgImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CpgImportError::Decode(err) => write!(f, "cpg protobuf decode failed: {err}"),
            CpgImportError::UnknownNodeType { key, type_id } => write!(
                f,
                "cpg import: unknown NodeType protoId {type_id} on node key {key}"
            ),
            CpgImportError::UnknownEdgeType { src, dst, type_id } => write!(
                f,
                "cpg import: unknown EdgeType protoId {type_id} on edge {src} -> {dst}"
            ),
        }
    }
}

impl std::error::Error for CpgImportError {}

pub(crate) fn node_kind_or_stop(key: i64, type_id: i32) -> Result<CpgNodeKind, CpgImportError> {
    node_kind(type_id).ok_or(CpgImportError::UnknownNodeType { key, type_id })
}

pub(crate) fn edge_kind_or_stop(
    src: i64,
    dst: i64,
    type_id: i32,
) -> Result<CpgEdgeKind, CpgImportError> {
    edge_kind(type_id).ok_or(CpgImportError::UnknownEdgeType { src, dst, type_id })
}
