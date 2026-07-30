//! protobuf -> flat types: the DECODE half of the SCIP wire.
//!
//! Split out of `scip.rs`, which had grown past the file-size law carrying two
//! subjects: that module runs the foreign indexer subprocesses, this one turns
//! their output into `crate::types`. The generated prost bindings are private
//! HERE, which is what keeps them out of every other module.

use std::path::Path;

use prost::Message;

use crate::types::{
    OccurrenceRole, PositionEncoding, ScipDiagnostic, ScipDocument, ScipError, ScipIndex,
    ScipMetadata, ScipOccurrence, ScipRelationship, ScipSignature, ScipSymbolInfo,
};

// doc(hidden): the generated rustdoc carries fenced symbol-grammar examples
// from scip.proto that are not Rust doctests; hide the module so rustdoc
// never tries to compile them.
#[doc(hidden)]
#[path = "scip/scip_proto.rs"]
mod proto;

/// The shared `load` body (one prost decode serves every indexer — the wire is
/// indexer-agnostic by construction).
pub fn load_index(index_path: &Path) -> Result<ScipIndex, ScipError> {
    let bytes = std::fs::read(index_path)
        .map_err(|e| ScipError::Parse(format!("read {}: {e}", index_path.display())))?;
    let index = proto::Index::decode(bytes.as_slice())
        .map_err(|e| ScipError::Parse(format!("protobuf decode: {e}")))?;
    Ok(diet(&index))
}

/// proto -> diet. NO LONGER A DIET IN THE ORIGINAL SENSE: every field the
/// protobuf carries crosses into these types (scip-passthrough lane). The
/// name stays because the target types are still v6's own flat structs, not
/// the generated prost ones, which is what keeps `proto` private.
///
/// The ONE thing not carried is the `Symbol` / `Package` / `Descriptor`
/// message family. Those messages are never serialized into an index.scip:
/// they document the grammar of the symbol STRING, which is passed through
/// verbatim. Splitting a symbol into scheme / package manager / package name /
/// version / descriptors is a string parse over a field the consumer already
/// holds, and string work is the dl layer's, same as the joins.
fn diet(index: &proto::Index) -> ScipIndex {
    let symbol = |si: &proto::SymbolInformation| ScipSymbolInfo {
        symbol: si.symbol.clone(),
        display_name: si.display_name.clone(),
        kind: si.kind,
        relationships: si
            .relationships
            .iter()
            .map(|rel| ScipRelationship {
                symbol: rel.symbol.clone(),
                is_reference: rel.is_reference,
                is_implementation: rel.is_implementation,
                is_type_definition: rel.is_type_definition,
                is_definition: rel.is_definition,
            })
            .collect(),
        documentation: si.documentation.clone(),
        signature: si
            .signature_documentation
            .as_ref()
            .map(|sig| ScipSignature {
                language: sig.language.clone(),
                text: sig.text.clone(),
                occurrences: sig.occurrences.iter().filter_map(occurrence).collect(),
            }),
        enclosing_symbol: si.enclosing_symbol.clone(),
    };
    let metadata = index.metadata.as_ref();
    let tool = metadata.and_then(|m| m.tool_info.as_ref());
    ScipIndex {
        documents: index
            .documents
            .iter()
            .map(|doc| ScipDocument {
                relative_path: doc.relative_path.clone(),
                position_encoding: match doc.position_encoding {
                    1 => PositionEncoding::Utf8,
                    2 => PositionEncoding::Utf16,
                    3 => PositionEncoding::Utf32,
                    _ => PositionEncoding::Unspecified,
                },
                occurrences: doc.occurrences.iter().filter_map(occurrence).collect(),
                symbols: doc.symbols.iter().map(symbol).collect(),
                language: doc.language.clone(),
                text: doc.text.clone(),
            })
            .collect(),
        external_symbols: index.external_symbols.iter().map(symbol).collect(),
        metadata: ScipMetadata {
            version: metadata.map(|m| m.version).unwrap_or_default(),
            tool_name: tool.map(|t| t.name.clone()).unwrap_or_default(),
            tool_version: tool.map(|t| t.version.clone()).unwrap_or_default(),
            tool_arguments: tool.map(|t| t.arguments.clone()).unwrap_or_default(),
            project_root: metadata.map(|m| m.project_root.clone()).unwrap_or_default(),
            text_document_encoding: metadata
                .map(|m| m.text_document_encoding)
                .unwrap_or_default(),
        },
    }
}

/// proto -> diet for one occurrence. An occurrence whose range does not
/// normalize is dropped (the `occurrence_range` law); the enclosing range is
/// optional and a malformed one is `None` rather than a dropped occurrence.
fn occurrence(occ: &proto::Occurrence) -> Option<ScipOccurrence> {
    Some(ScipOccurrence {
        symbol: occ.symbol.clone(),
        range: occurrence_range(occ)?,
        roles: OccurrenceRole(occ.symbol_roles),
        syntax_kind: occ.syntax_kind,
        enclosing_range: enclosing_range(occ),
        override_documentation: occ.override_documentation.clone(),
        diagnostics: occ
            .diagnostics
            .iter()
            .map(|diag| ScipDiagnostic {
                severity: diag.severity,
                code: diag.code.clone(),
                message: diag.message.clone(),
                source: diag.source.clone(),
                tags: diag.tags.clone(),
            })
            .collect(),
    })
}

/// scip.proto's occurrence range comes in two encodings: the typed oneof
/// (`single_line_range` / `multi_line_range`, preferred when present) and the
/// deprecated packed `repeated int32` (`[sl, sc, el, ec]`, or the 3-element
/// short form `[sl, sc, ec]` with end_line == start_line). Normalize both to
/// the quad `[start_line, start_col, end_line, end_col]`. Malformed packed
/// lengths are dropped (v5 `parse_range` parity).
#[allow(deprecated)] // the packed `range` fallback is the backward-compat law
fn occurrence_range(occ: &proto::Occurrence) -> Option<[i32; 4]> {
    match &occ.typed_range {
        Some(proto::occurrence::TypedRange::SingleLineRange(r)) => {
            Some([r.line, r.start_character, r.line, r.end_character])
        }
        Some(proto::occurrence::TypedRange::MultiLineRange(r)) => {
            Some([r.start_line, r.start_character, r.end_line, r.end_character])
        }
        None => packed_range(&occ.range),
    }
}

/// The enclosing range in the same two encodings as `occurrence_range`: the
/// typed oneof first, then the deprecated packed `repeated int32`. `None` is
/// the honest answer for "the indexer emitted none", which is the common case
/// (scip-typescript emits enclosing ranges for definitions only).
#[allow(deprecated)] // the packed `enclosing_range` fallback is the same law
fn enclosing_range(occ: &proto::Occurrence) -> Option<[i32; 4]> {
    match &occ.typed_enclosing_range {
        Some(proto::occurrence::TypedEnclosingRange::SingleLineEnclosingRange(r)) => {
            Some([r.line, r.start_character, r.line, r.end_character])
        }
        Some(proto::occurrence::TypedEnclosingRange::MultiLineEnclosingRange(r)) => {
            Some([r.start_line, r.start_character, r.end_line, r.end_character])
        }
        None => packed_range(&occ.enclosing_range),
    }
}

/// The deprecated packed encoding: `[sl, sc, el, ec]`, or the 3-element short
/// form `[sl, sc, ec]` with end_line == start_line. Any other length is
/// malformed and drops (v5 `parse_range` parity).
fn packed_range(packed: &[i32]) -> Option<[i32; 4]> {
    match packed {
        [sl, sc, el, ec] => Some([*sl, *sc, *el, *ec]),
        [sl, sc, ec] => Some([*sl, *sc, *sl, *ec]),
        _ => None,
    }
}
