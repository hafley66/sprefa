//! protobuf -> flat types: the DECODE half of the CPG importer, shaped after
//! `scip_decode.rs`. The generated prost bindings are private HERE.

use prost::Message;

use crate::cpg_types::{
    edge_kind_or_stop, node_kind_or_stop, CpgEdge, CpgImport, CpgImportError, CpgNode,
    CpgProperty, CpgPropertyValue,
};

#[doc(hidden)]
#[path = "cpg/cpg_proto.rs"]
mod proto;

/// Decode one `CpgStruct` payload into `CpgImport`. An unrecognized
/// NodeType/EdgeType protoId stops the whole decode (house law: named stop).
pub fn decode_cpg_struct(bytes: &[u8]) -> Result<CpgImport, CpgImportError> {
    let parsed = proto::CpgStruct::decode(bytes).map_err(CpgImportError::Decode)?;
    let mut nodes = Vec::with_capacity(parsed.node.len());
    for node in &parsed.node {
        let kind = node_kind_or_stop(node.key, node.r#type)?;
        nodes.push(CpgNode {
            key: node.key,
            kind,
            properties: node.property.iter().map(node_property).collect(),
        });
    }
    let mut edges = Vec::with_capacity(parsed.edge.len());
    for edge in &parsed.edge {
        let kind = edge_kind_or_stop(edge.src, edge.dst, edge.r#type)?;
        edges.push(CpgEdge {
            src: edge.src,
            dst: edge.dst,
            kind,
            properties: edge.property.iter().map(edge_property).collect(),
        });
    }
    Ok(CpgImport { nodes, edges })
}

fn node_property(prop: &proto::cpg_struct::Property) -> CpgProperty {
    CpgProperty {
        name_id: prop.name,
        value: property_value(prop.value.as_ref()),
    }
}

fn edge_property(prop: &proto::cpg_struct::edge::Property) -> CpgProperty {
    CpgProperty {
        name_id: prop.name,
        value: property_value(prop.value.as_ref()),
    }
}

/// The 13-variant oneof, flattened 1:1. `None` (proto3 absent oneof) is
/// `CpgPropertyValue::Absent`, the honest answer for "no value on the wire".
fn property_value(value: Option<&proto::PropertyValue>) -> CpgPropertyValue {
    use proto::property_value::Value;
    match value.and_then(|pv| pv.value.as_ref()) {
        None => CpgPropertyValue::Absent,
        Some(Value::StringValue(v)) => CpgPropertyValue::Str(v.clone()),
        Some(Value::BoolValue(v)) => CpgPropertyValue::Bool(*v),
        Some(Value::IntValue(v)) => CpgPropertyValue::Int(*v),
        Some(Value::LongValue(v)) => CpgPropertyValue::Long(*v),
        Some(Value::FloatValue(v)) => CpgPropertyValue::Float(*v),
        Some(Value::DoubleValue(v)) => CpgPropertyValue::Double(*v),
        Some(Value::StringList(v)) => CpgPropertyValue::StringList(v.values.clone()),
        Some(Value::BoolList(v)) => CpgPropertyValue::BoolList(v.values.clone()),
        Some(Value::IntList(v)) => CpgPropertyValue::IntList(v.values.clone()),
        Some(Value::LongList(v)) => CpgPropertyValue::LongList(v.values.clone()),
        Some(Value::FloatList(v)) => CpgPropertyValue::FloatList(v.values.clone()),
        Some(Value::DoubleList(v)) => CpgPropertyValue::DoubleList(v.values.clone()),
        Some(Value::ContainedRefs(v)) => CpgPropertyValue::ContainedRefs {
            local_name: v.local_name.clone(),
            refs: v.refs.clone(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpg_types::{CpgEdgeKind, CpgNodeKind};

    /// Builds one METHOD node, one CALL node, one AST edge between them, each
    /// carrying a property, plus one node with an unmapped NodeType protoId.
    fn fixture(bad_node_type: i32) -> proto::CpgStruct {
        proto::CpgStruct {
            node: vec![
                proto::cpg_struct::Node {
                    key: 1,
                    r#type: 1, // METHOD
                    property: vec![proto::cpg_struct::Property {
                        name: 5, // NAME protoId, not resolved by this card
                        value: Some(proto::PropertyValue {
                            value: Some(proto::property_value::Value::StringValue(
                                "main".to_string(),
                            )),
                        }),
                    }],
                },
                proto::cpg_struct::Node {
                    key: 2,
                    r#type: 15, // CALL
                    property: vec![],
                },
                proto::cpg_struct::Node {
                    key: 3,
                    r#type: bad_node_type,
                    property: vec![],
                },
            ],
            edge: vec![proto::cpg_struct::Edge {
                src: 1,
                dst: 2,
                r#type: 3, // AST
                property: vec![proto::cpg_struct::edge::Property {
                    name: 1,
                    value: Some(proto::PropertyValue {
                        value: Some(proto::property_value::Value::IntValue(7)),
                    }),
                }],
            }],
        }
    }

    #[test]
    fn maps_known_node_and_edge_kinds_and_carries_properties() {
        let mut built = fixture(1 /* METHOD, so no third-node stop fires */);
        built.node.truncate(2);
        let bytes = built.encode_to_vec();

        let import = decode_cpg_struct(&bytes).expect("known ids decode");
        assert_eq!(import.nodes.len(), 2);
        assert_eq!(import.edges.len(), 1);

        assert_eq!(import.nodes[0].key, 1);
        assert_eq!(import.nodes[0].kind, CpgNodeKind::Method);
        assert_eq!(
            import.nodes[0].properties[0].value,
            CpgPropertyValue::Str("main".to_string())
        );

        assert_eq!(import.nodes[1].kind, CpgNodeKind::Call);

        assert_eq!(import.edges[0].kind, CpgEdgeKind::Ast);
        assert_eq!(import.edges[0].src, 1);
        assert_eq!(import.edges[0].dst, 2);
        assert_eq!(
            import.edges[0].properties[0].value,
            CpgPropertyValue::Int(7)
        );
    }

    #[test]
    fn unknown_node_type_is_a_named_stop_not_a_silent_skip() {
        let built = fixture(999_999);
        let bytes = built.encode_to_vec();

        let err = decode_cpg_struct(&bytes).expect_err("unmapped protoId must stop the decode");
        match err {
            CpgImportError::UnknownNodeType { key, type_id } => {
                assert_eq!(key, 3);
                assert_eq!(type_id, 999_999);
            }
            other => panic!("expected UnknownNodeType, got {other:?}"),
        }
        assert!(err_mentions(&err, "999999"));
    }

    #[test]
    fn unknown_edge_type_is_a_named_stop() {
        let mut built = fixture(1);
        built.node.truncate(2);
        built.edge[0].r#type = 424_242;
        let bytes = built.encode_to_vec();

        let err = decode_cpg_struct(&bytes).expect_err("unmapped edge protoId must stop");
        match err {
            CpgImportError::UnknownEdgeType { src, dst, type_id } => {
                assert_eq!((src, dst, type_id), (1, 2, 424_242));
            }
            other => panic!("expected UnknownEdgeType, got {other:?}"),
        }
    }

    fn err_mentions(err: &CpgImportError, needle: &str) -> bool {
        format!("{err}").contains(needle)
    }
}
