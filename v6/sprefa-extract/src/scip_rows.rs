//! SCIP index -> flat rows. The projection half of the SCIP wire.
//!
//! Split out of `wire.rs` on size and on subject: that module flattens the four
//! extraction families, this one flattens a foreign tool's index, and they
//! change for different reasons. `wire` re-exports both public functions so no
//! import path moved.
//!
//! PASSTHROUGH IS THE LAW HERE (the scip-passthrough lane). Every field
//! scip.proto serializes reaches the wire. The rows stay UNJOINED: v5 spells
//! ten relations over this data (`scip_occurrence`, `scip_def`, `scip_ref`,
//! `scip_name`, `scip_local`, `scip_binding`, `scip_impl`, `scip_edge`,
//! `scip_fn_edge`, `scip_callee_type`) and every one of them is a filter or a
//! join over these rows. Those machines live in the dl layer by standing law,
//! and projecting them here would weld v5's relation vocabulary into a crate
//! that must survive the ts-to-rust port unchanged.
//!
//! THE ONE EXCEPTION is `scip_file_edges`, and its reason is measured, not
//! stylistic. See that function's own doc.

use std::collections::{BTreeMap, BTreeSet};

use crate::scip::{byte_range_at, LineTable};
use crate::shape::Span;
use crate::types::{
    FlatFact, OccurrenceRole, PositionEncoding, ScipDocument, ScipIndex, ScipOccurrence,
    ScipSymbolInfo,
};

/// The record kinds `flatten_scip` may produce, in wire order. Named here
/// because this module owns the projection; the CLI only parses names.
pub const SCIP_RECORD_KINDS: &[&str] = &[
    "scip_metadata",
    "scip_document",
    "scip_occurrence",
    "scip_occurrence_doc",
    "scip_diagnostic",
    "scip_symbol",
    "scip_relationship",
    "scip_documentation",
    "scip_signature",
    "scip_signature_occurrence",
];

/// WHICH record kinds a caller wants. The demand-side lever for the measured
/// cost of full passthrough: over v6/tsv2 the whole stream is 177,967 rows and
/// 59.4MB, of which the occurrence rows alone are 48.5MB. A consumer that
/// wants only relationships should not pay for the occurrences, and filtering
/// HERE rather than at the wire keeps the rows out of memory too.
///
/// The default is every kind, so passthrough stays the default and narrowing
/// is the explicit act.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScipRecords {
    wanted: Option<BTreeSet<String>>,
}

impl Default for ScipRecords {
    fn default() -> Self {
        Self::all()
    }
}

impl ScipRecords {
    /// Every kind: full passthrough.
    pub fn all() -> Self {
        Self { wanted: None }
    }

    /// Parse a comma-separated kind list. An unknown name is an ERROR, never a
    /// silent drop: a typo would otherwise produce an empty stream that reads
    /// as "this index has no such facts".
    pub fn parse(spec: &str) -> Result<Self, String> {
        let mut wanted = BTreeSet::new();
        for name in spec.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            if !SCIP_RECORD_KINDS.contains(&name) {
                return Err(format!(
                    "'{name}' is not a scip record kind; known kinds are {}",
                    SCIP_RECORD_KINDS.join(", ")
                ));
            }
            wanted.insert(name.to_string());
        }
        if wanted.is_empty() {
            return Err("--scip-record selected no record kind".to_string());
        }
        Ok(Self {
            wanted: Some(wanted),
        })
    }

    fn wants(&self, kind: &str) -> bool {
        self.wanted
            .as_ref()
            .is_none_or(|wanted| wanted.contains(kind))
    }
}

/// Flatten a loaded SCIP index to raw flat facts: the index metadata, every
/// document header, every occurrence with its roles / syntax kind / enclosing
/// range / range docs / diagnostics, every symbol information row with its
/// docs / signature / relationships, and the external symbols.
///
/// `reader` supplies each document's bytes, because SCIP ranges are (line, col)
/// in the document's own position encoding and this wire is byte offsets. A
/// document the reader cannot read contributes its header, symbols and
/// relationships but no occurrences, and an occurrence whose range does not
/// convert is dropped rather than clamped into a lie (the `byte_range` law).
pub fn flatten_scip(index: &ScipIndex, reader: &dyn Fn(&str) -> Option<Vec<u8>>) -> Vec<FlatFact> {
    flatten_scip_records(index, reader, &ScipRecords::all(), false)
}

/// `flatten_scip` narrowed to the requested record kinds. See `ScipRecords`.
/// `occurrence_text` asks the `scip_occurrence` row to also carry the source
/// slice at its span (lossy-utf8), the v6 answer to v5 scip_binding's
/// `local_name`; off, the row is byte-identical to before the field existed.
pub fn flatten_scip_records(
    index: &ScipIndex,
    reader: &dyn Fn(&str) -> Option<Vec<u8>>,
    records: &ScipRecords,
    occurrence_text: bool,
) -> Vec<FlatFact> {
    let mut out = Vec::new();
    if records.wants("scip_metadata") {
        out.push(FlatFact::ScipMetadataRow {
            version: index.metadata.version,
            tool_name: index.metadata.tool_name.clone(),
            tool_version: index.metadata.tool_version.clone(),
            tool_arguments: index.metadata.tool_arguments.clone(),
            project_root: index.metadata.project_root.clone(),
            text_document_encoding: index.metadata.text_document_encoding,
        });
    }
    for document in &index.documents {
        if records.wants("scip_document") {
            out.push(FlatFact::ScipDocumentRow {
                path: document.relative_path.clone(),
                language: document.language.clone(),
                position_encoding: document.position_encoding.ordinal(),
                // Empty means "read the file yourself", which is not the same
                // fact as an inlined empty document; null says which one.
                text: (!document.text.is_empty()).then(|| document.text.clone()),
            });
        }
        // The content read is skipped entirely when no occurrence-side kind is
        // wanted: a narrowed stream must not pay to read the corpus.
        if wants_occurrences(records) {
            if let Some(content) = reader(&document.relative_path) {
                let lines = LineTable::build(&content);
                for occurrence in &document.occurrences {
                    out.extend(occurrence_rows(
                        index,
                        document,
                        occurrence,
                        &content,
                        &lines,
                        records,
                        occurrence_text,
                    ));
                }
            }
        }
        for info in &document.symbols {
            out.extend(symbol_rows(index, info, Some(&document.relative_path), records));
        }
    }
    // External symbols belong to no document: they are what the corpus
    // references and does not define, which is exactly how a consumer tells a
    // library call from a corpus call.
    for info in &index.external_symbols {
        out.extend(symbol_rows(index, info, None, records));
    }
    out
}

fn wants_occurrences(records: &ScipRecords) -> bool {
    records.wants("scip_occurrence")
        || records.wants("scip_occurrence_doc")
        || records.wants("scip_diagnostic")
}

/// One occurrence's rows: the occurrence itself, then its range-specific
/// documentation and its diagnostics as child rows keyed by the same
/// (path, start, end) coordinate.
fn occurrence_rows(
    index: &ScipIndex,
    document: &ScipDocument,
    occurrence: &ScipOccurrence,
    content: &[u8],
    lines: &LineTable,
    records: &ScipRecords,
    occurrence_text: bool,
) -> Vec<FlatFact> {
    let encoding = document.position_encoding;
    let Some(span) = byte_range_at(content, lines, occurrence.range, encoding) else {
        return Vec::new();
    };
    let (start, end) = (span.start, span.end());
    let path = document.relative_path.clone();
    let enclosing = occurrence
        .enclosing_range
        .and_then(|range| byte_range_at(content, lines, range, encoding));
    let roles = occurrence.roles;
    let text = occurrence_text
        .then(|| {
            let s = start as usize;
            let e = end as usize;
            (e <= content.len()).then(|| String::from_utf8_lossy(&content[s..e]).into_owned())
        })
        .flatten();
    let mut out = Vec::new();
    if records.wants("scip_occurrence") {
        out.push(FlatFact::ScipOccurrenceRow {
            path: path.clone(),
            symbol: index.symbol(occurrence.symbol).to_string(),
            start,
            end,
            roles: roles.0,
            definition: roles.contains(OccurrenceRole::DEFINITION),
            import: roles.contains(OccurrenceRole::IMPORT),
            write_access: roles.contains(OccurrenceRole::WRITE_ACCESS),
            read_access: roles.contains(OccurrenceRole::READ_ACCESS),
            generated: roles.contains(OccurrenceRole::GENERATED),
            test: roles.contains(OccurrenceRole::TEST),
            forward_definition: roles.contains(OccurrenceRole::FORWARD_DEF),
            syntax_kind: occurrence.syntax_kind,
            enclosing_start: enclosing.map(|span| span.start),
            enclosing_end: enclosing.map(Span::end),
            text,
        });
    }
    for (pos, text) in occurrence
        .override_documentation
        .iter()
        .enumerate()
        .filter(|_| records.wants("scip_occurrence_doc"))
    {
        out.push(FlatFact::ScipOccurrenceDocRow {
            path: path.clone(),
            start,
            end,
            pos: pos as u32,
            text: text.clone(),
        });
    }
    for diagnostic in occurrence
        .diagnostics
        .iter()
        .filter(|_| records.wants("scip_diagnostic"))
    {
        out.push(FlatFact::ScipDiagnosticRow {
            path: path.clone(),
            start,
            end,
            severity: diagnostic.severity,
            code: diagnostic.code.clone(),
            message: diagnostic.message.clone(),
            source: diagnostic.source.clone(),
            tags: diagnostic.tags.clone(),
        });
    }
    out
}

/// One symbol information's rows: the symbol itself, its relationships, its
/// docstrings, and its signature with the references inside the signature text.
fn symbol_rows(
    index: &ScipIndex,
    info: &ScipSymbolInfo,
    path: Option<&str>,
    records: &ScipRecords,
) -> Vec<FlatFact> {
    let mut out = Vec::new();
    if records.wants("scip_symbol") {
        out.push(FlatFact::ScipSymbolRow {
            path: path.map(str::to_string),
            symbol: index.symbol(info.symbol).to_string(),
            display_name: info.display_name.clone(),
            kind: info.kind,
            enclosing_symbol: info.enclosing_symbol.clone(),
        });
    }
    for related in info
        .relationships
        .iter()
        .filter(|_| records.wants("scip_relationship"))
    {
        out.push(FlatFact::ScipRelationshipRow {
            symbol: index.symbol(info.symbol).to_string(),
            related_symbol: index.symbol(related.symbol).to_string(),
            is_reference: related.is_reference,
            is_implementation: related.is_implementation,
            is_type_definition: related.is_type_definition,
            is_definition: related.is_definition,
        });
    }
    for (pos, text) in info
        .documentation
        .iter()
        .enumerate()
        .filter(|_| records.wants("scip_documentation"))
    {
        out.push(FlatFact::ScipDocumentationRow {
            symbol: index.symbol(info.symbol).to_string(),
            pos: pos as u32,
            text: text.clone(),
        });
    }
    if let Some(signature) = &info.signature {
        if records.wants("scip_signature") {
            out.push(FlatFact::ScipSignatureRow {
                symbol: index.symbol(info.symbol).to_string(),
                language: signature.language.clone(),
                text: signature.text.clone(),
            });
        }
        if !records.wants("scip_signature_occurrence") {
            return out;
        }
        // Signature occurrence ranges are relative to the signature TEXT. The
        // proto gives that text no position encoding of its own, and the
        // spec's default reading for an unset encoding is UTF-16.
        let bytes = signature.text.as_bytes();
        let lines = LineTable::build(bytes);
        for occurrence in &signature.occurrences {
            let Some(span) = byte_range_at(
                bytes,
                &lines,
                occurrence.range,
                PositionEncoding::Unspecified,
            ) else {
                continue;
            };
            out.push(FlatFact::ScipSignatureOccurrenceRow {
                symbol: index.symbol(info.symbol).to_string(),
                ref_symbol: index.symbol(occurrence.symbol).to_string(),
                start: span.start,
                end: span.end(),
                roles: occurrence.roles.0,
            });
        }
    }
    out
}

/// Fold a loaded SCIP index into file-to-file dependency edges: `src` holds a
/// non-definition occurrence of a symbol defined in `dst`.
///
/// WHY THIS IS THE ONE DERIVED RELATION THE EXTRACTOR PROJECTS. The fold is
/// exactly the join a dl rule would write over `flatten_scip`'s occurrence rows,
/// and by the standing law those machines live in prolog. It is here anyway
/// because the amplification is measured: over v6/tsv2, 212 TypeScript files
/// produce 123,655 occurrence rows and 764 edges. Sending 123,655 rows across
/// the wire to compute 764 is the shape the N+1 law exists to stop. The raw rows
/// remain available for every join that is not this one.
///
/// GRADED AGAINST MADGE over that same corpus: 755 of madge's 761 edges agree,
/// recall 0.992, precision 0.988. Both divergence classes are understood and
/// neither is a resolution error.
///  - 6 madge-only edges are all from `labs/`, which the corpus tsconfig's
///    `include` omits. scip-typescript indexes the tsconfig program; madge walks
///    the filesystem. The two tools disagree about the corpus, not the graph.
///  - 9 scip-only edges are all references to declarations in one types module
///    from files with NO import of it. SCIP sees an inferred type reaching them
///    through another module's signature; madge scans import syntax and cannot.
///    SCIP is right and strictly sees more, the same shape as the flagship
///    callgraph result.
///
/// `kind` IS `unknown` ON EVERY ROW THIS FOLD PRODUCES, and that is a property
/// of an index rather than a gap here: the edge is a symbol crossing, and the
/// occurrence that carries it says nothing about the import form that bound the
/// name. `--deps` fills the column from the specifier it read.
///
/// LOCAL SYMBOLS ARE EXCLUDED. scip reuses `local N` per document, so a `local`
/// symbol in two files is two different things and joining on it would mint
/// edges between unrelated files.
pub fn scip_file_edges(index: &ScipIndex) -> Vec<FlatFact> {
    let mut defining_document: BTreeMap<&str, &str> = BTreeMap::new();
    for document in &index.documents {
        for occurrence in &document.occurrences {
            if !occurrence.roles.contains(OccurrenceRole::DEFINITION)
                || index.symbol(occurrence.symbol).starts_with("local ")
            {
                continue;
            }
            defining_document
                .entry(index.symbol(occurrence.symbol))
                .or_insert(&document.relative_path);
        }
    }

    // (src, dst) -> the distinct symbols crossing it. A BTreeSet both dedups a
    // symbol referenced many times in one file and keeps the output ordered.
    let mut crossings: BTreeMap<(&str, &str), BTreeSet<&str>> = BTreeMap::new();
    for document in &index.documents {
        for occurrence in &document.occurrences {
            if occurrence.roles.contains(OccurrenceRole::DEFINITION)
                || index.symbol(occurrence.symbol).starts_with("local ")
            {
                continue;
            }
            let Some(target) = defining_document.get(index.symbol(occurrence.symbol)) else {
                continue;
            };
            if *target == document.relative_path {
                continue;
            }
            crossings
                .entry((&document.relative_path, target))
                .or_default()
                .insert(index.symbol(occurrence.symbol));
        }
    }

    crossings
        .into_iter()
        .map(|((src, dst), symbols)| FlatFact::FileEdgeRow {
            src_path: src.to_string(),
            dst_path: dst.to_string(),
            // An index records resolved occurrences, never the import statement
            // that bound the name, so the specifier kind is not in it to read.
            kind: "unknown".to_string(),
            symbols: symbols.len() as u32,
        })
        .collect()
}
