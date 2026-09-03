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
    ScipMetadata, ScipOccurrence, ScipRelationship, ScipSignature, ScipSymbolInfo, SymbolInterner,
};

// doc(hidden): the generated rustdoc carries fenced symbol-grammar examples
// from scip.proto that are not Rust doctests; hide the module so rustdoc
// never tries to compile them.
#[doc(hidden)]
#[path = "scip/scip_proto.rs"]
mod proto;

/// The shared `load` body (one prost decode serves every indexer — the wire is
/// indexer-agnostic by construction).
///
/// STREAMING over the top-level fields: only one proto `Document` is ever
/// alive at a time and the raw bytes are the only whole-index copy resident.
/// A whole-`Index` decode holds the protobuf tree AND its flat twin
/// simultaneously and that pair, not the resolved graph, is the run's memory
/// peak.
pub fn load_index(index_path: &Path) -> Result<ScipIndex, ScipError> {
    let bytes = std::fs::read(index_path)
        .map_err(|e| ScipError::Parse(format!("read {}: {e}", index_path.display())))?;
    let mut documents = Vec::new();
    let mut external_symbols: Vec<ScipSymbolInfo> = Vec::new();
    let mut metadata: Option<proto::Metadata> = None;
    let mut symbols = SymbolInterner::default();
    for_each_message(&bytes, &mut |field, buf| match field {
        1 => {
            metadata = Some(
                proto::Metadata::decode(buf)
                    .map_err(|e| ScipError::Parse(format!("protobuf decode: {e}")))?,
            );
            Ok(())
        }
        2 => {
            let doc = proto::Document::decode(buf)
                .map_err(|e| ScipError::Parse(format!("protobuf decode: {e}")))?;
            documents.push(diet_document(&doc, &mut symbols));
            Ok(())
        }
        3 => {
            let info = proto::SymbolInformation::decode(buf)
                .map_err(|e| ScipError::Parse(format!("protobuf decode: {e}")))?;
            external_symbols.push(diet_symbol(&info, &mut symbols));
            Ok(())
        }
        _ => Ok(()),
    })?;
    let metadata = metadata.as_ref();
    let tool = metadata.and_then(|m| m.tool_info.as_ref());
    let index = ScipIndex {
        documents,
        external_symbols,
        metadata: diet_metadata(metadata, tool),
        symbols: symbols.table(),
        defs: std::sync::OnceLock::new(),
    };
    // The symbol->def map is a pure function of the decoded documents, so it
    // is built once here, on the decode path, and never re-derived per call.
    index
        .defs
        .get_or_init(|| crate::scip::build_def_map(&index));
    Ok(index)
}

/// Walk the length-delimited message fields of a top-level index buffer,
/// handing each (field number, encoded message bytes) to `visit`. Unknown
/// fields are skipped by wire type; a varint or fixed field on this level is
/// skipped the same way. `proto::Index` is only ever these three message
/// fields (metadata 1, documents 2, external_symbols 3).
fn for_each_message(
    bytes: &[u8],
    visit: &mut dyn FnMut(u32, &[u8]) -> Result<(), ScipError>,
) -> Result<(), ScipError> {
    use prost::encoding::{decode_key, decode_varint, skip_field, WireType};
    let mut buf = bytes;
    while !buf.is_empty() {
        let (field, wire) =
            decode_key(&mut buf).map_err(|e| ScipError::Parse(format!("protobuf decode: {e}")))?;
        match wire {
            WireType::LengthDelimited => {
                let len = decode_varint(&mut buf)
                    .map_err(|e| ScipError::Parse(format!("protobuf decode: {e}")))?;
                let len = len as usize;
                if buf.len() < len {
                    return Err(ScipError::Parse(
                        "protobuf decode: truncated message".into(),
                    ));
                }
                visit(field, &buf[..len])?;
                buf = &buf[len..];
            }
            _ => skip_field(
                wire,
                field,
                &mut buf,
                prost::encoding::DecodeContext::default(),
            )
            .map_err(|e| ScipError::Parse(format!("protobuf decode: {e}")))?,
        }
    }
    Ok(())
}

/// Merge several per-language SCIP indexes into one on disk (v5
/// `scip_import::merge_files`, re-runtimed onto prost).
///
/// SCIP is document-keyed, so a union is exactly concatenating `documents` and
/// `external_symbols`: each indexer already namespaces its symbols by tool and
/// package, so there is no key collision across languages. `metadata` is
/// carried from the first input, which is the honest answer for a merged index
/// (there is no single tool that produced it, and inventing one would put a lie
/// in the `scip_metadata` row).
///
/// Lives HERE rather than beside the other merge-shaped code because the prost
/// bindings are private to this module, and keeping them private is what stops
/// the generated types leaking into the rest of the crate.
pub fn merge_indexes(inputs: &[std::path::PathBuf], out: &Path) -> Result<usize, ScipError> {
    let mut merged: Option<proto::Index> = None;
    let mut documents = 0usize;
    for path in inputs {
        let bytes = std::fs::read(path)
            .map_err(|e| ScipError::Parse(format!("read {}: {e}", path.display())))?;
        let index = proto::Index::decode(bytes.as_slice())
            .map_err(|e| ScipError::Parse(format!("protobuf decode {}: {e}", path.display())))?;
        documents += index.documents.len();
        match &mut merged {
            None => merged = Some(index),
            Some(into) => {
                into.documents.extend(index.documents);
                into.external_symbols.extend(index.external_symbols);
            }
        }
    }
    let merged = merged.unwrap_or_default();
    std::fs::write(out, merged.encode_to_vec())
        .map_err(|e| ScipError::Parse(format!("write {}: {e}", out.display())))?;
    Ok(documents)
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
fn diet_symbol(si: &proto::SymbolInformation, symbols: &mut SymbolInterner) -> ScipSymbolInfo {
    ScipSymbolInfo {
        symbol: symbols.intern(&si.symbol),
        display_name: si.display_name.clone(),
        kind: si.kind,
        relationships: si
            .relationships
            .iter()
            .map(|rel| ScipRelationship {
                symbol: symbols.intern(&rel.symbol),
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
                occurrences: sig
                    .occurrences
                    .iter()
                    .filter_map(|occ| occurrence(occ, symbols))
                    .collect(),
            }),
        enclosing_symbol: si.enclosing_symbol.clone(),
    }
}

fn diet_document(doc: &proto::Document, symbols: &mut SymbolInterner) -> ScipDocument {
    ScipDocument {
        relative_path: doc.relative_path.clone(),
        position_encoding: match doc.position_encoding {
            1 => PositionEncoding::Utf8,
            2 => PositionEncoding::Utf16,
            3 => PositionEncoding::Utf32,
            _ => PositionEncoding::Unspecified,
        },
        occurrences: doc
            .occurrences
            .iter()
            .filter_map(|occ| occurrence(occ, symbols))
            .collect(),
        symbols: doc
            .symbols
            .iter()
            .map(|si| diet_symbol(si, symbols))
            .collect(),
        language: doc.language.clone(),
        text: doc.text.clone(),
        spans: std::sync::OnceLock::new(),
    }
}

fn diet_metadata(
    metadata: Option<&proto::Metadata>,
    tool: Option<&proto::ToolInfo>,
) -> ScipMetadata {
    ScipMetadata {
        version: metadata.map(|m| m.version).unwrap_or_default(),
        tool_name: tool.map(|t| t.name.clone()).unwrap_or_default(),
        tool_version: tool.map(|t| t.version.clone()).unwrap_or_default(),
        tool_arguments: tool.map(|t| t.arguments.clone()).unwrap_or_default(),
        project_root: metadata.map(|m| m.project_root.clone()).unwrap_or_default(),
        text_document_encoding: metadata
            .map(|m| m.text_document_encoding)
            .unwrap_or_default(),
    }
}

/// proto -> diet for one occurrence. An occurrence whose range does not
/// normalize is dropped (the `occurrence_range` law); the enclosing range is
/// optional and a malformed one is `None` rather than a dropped occurrence.
fn occurrence(occ: &proto::Occurrence, symbols: &mut SymbolInterner) -> Option<ScipOccurrence> {
    Some(ScipOccurrence {
        symbol: symbols.intern(&occ.symbol),
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
