//! Cross-check the `ImportRef`s a `Rehome` impl answered with against a loaded
//! SCIP index: what the index knows and the impl missed, what the impl produced
//! and the index does not know. No language is named here; the index and the
//! batch decide everything. The index arrives LOADED — `ScipSource::build`
//! shells out to a foreign indexer and nothing in this module does.
//! @comment-ok: module header, the seam list every move file opens with

use std::collections::{BTreeMap, BTreeSet};

use crate::move_cx::MoveCx;
use crate::scip::{byte_range_at, LineTable};
use crate::types::{ImportRef, OccurrenceRole, ScipIndex, Span};

/// A ref the index carries and the impl did not answer with.
pub const MISSED_BY_IMPL: &str = "missed_by_impl";
/// A ref the impl answered with that no occurrence in the index covers.
pub const UNKNOWN_TO_SCIP: &str = "unknown_to_scip";

/// One import-shaped occurrence in the coordinates a move works in: byte span
/// in the importer, corpus path of the document defining the symbol.
pub struct ScipSite {
    /// Project-relative path of the document writing the occurrence.
    pub importer: String,
    pub span: Span,
    /// The bytes `span` covers.
    pub text: String,
    pub symbol: String,
    /// `None` when `symbol` is defined outside the indexed corpus.
    pub target: Option<String>,
}

/// One place the index and a `Rehome` impl disagree about the corpus.
pub struct ScipDisagreement {
    pub importer: String,
    pub span: Span,
    /// `MISSED_BY_IMPL` | `UNKNOWN_TO_SCIP`.
    pub kind: &'static str,
    pub detail: String,
}

/// Every occurrence in `index` that names an imported module. A document the
/// corpus does not hold is skipped, so vendored documents never compare.
///
/// IMPORT-shaped is `roles` carrying IMPORT **or** a range whose bytes are a
/// quoted string literal. The role bit alone answers NOTHING for the TypeScript
/// door: scip-typescript 0.4.0 writes `symbol_roles` at exactly two sites,
/// `dist/src/FileIndexer.js:80` and `:214`, and both write
/// `SymbolRole.Definition`. What it does emit is an occurrence over the
/// specifier literal whose symbol is the target document's module symbol.
/// @comment-ok: the fallback rule is a measured indexer fact, not a preference
pub fn scip_import_sites(cx: &MoveCx, index: &ScipIndex) -> Vec<ScipSite> {
    // ONE pass builds the symbol table; `scip::definition_of` rescans every
    // document per lookup.
    let mut defining: BTreeMap<&str, &str> = BTreeMap::new();
    for document in &index.documents {
        for occurrence in &document.occurrences {
            if occurrence.roles.contains(OccurrenceRole::DEFINITION) {
                defining
                    .entry(occurrence.symbol.as_str())
                    .or_insert(document.relative_path.as_str());
            }
        }
    }
    let mut sites = Vec::new();
    for document in &index.documents {
        let importer = document.relative_path.as_str();
        if !cx.contains(importer) {
            continue;
        }
        let Some(content) = cx.read(importer) else {
            continue;
        };
        let lines = LineTable::build(&content);
        for occurrence in &document.occurrences {
            let Some(span) = byte_range_at(
                &content,
                &lines,
                occurrence.range,
                document.position_encoding,
            ) else {
                continue;
            };
            if !occurrence.roles.contains(OccurrenceRole::IMPORT) && !quoted_literal(&content, span)
            {
                continue;
            }
            let Ok(text) = std::str::from_utf8(&content[span.start as usize..span.end() as usize])
            else {
                continue;
            };
            let target = defining
                .get(occurrence.symbol.as_str())
                .filter(|path| **path != importer && cx.contains(path))
                .map(|path| path.to_string());
            sites.push(ScipSite {
                importer: importer.to_string(),
                span,
                text: text.to_string(),
                symbol: occurrence.symbol.clone(),
                target,
            });
        }
    }
    sites.sort_by(|left, right| {
        (&left.importer, left.span.start, left.span.len).cmp(&(
            &right.importer,
            right.span.start,
            right.span.len,
        ))
    });
    tracing::debug!(
        documents = index.documents.len(),
        sites = sites.len(),
        "move scip sites"
    );
    sites
}

/// Where `index` and `refs` disagree, in `(importer, span)` order. A ref matches
/// a site on importer plus span overlap, one-to-one; leftovers are the report.
pub fn verify_import_refs(
    cx: &MoveCx,
    index: &ScipIndex,
    refs: &[ImportRef],
) -> Vec<ScipDisagreement> {
    let indexed: BTreeSet<&str> = index
        .documents
        .iter()
        .map(|document| document.relative_path.as_str())
        .collect();
    let sites: Vec<ScipSite> = scip_import_sites(cx, index)
        .into_iter()
        .filter(|site| in_batch(cx, &site.importer, site.target.as_deref()))
        .collect();
    let mut claimed = vec![false; sites.len()];
    let mut by_importer: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (at, site) in sites.iter().enumerate() {
        by_importer
            .entry(site.importer.as_str())
            .or_default()
            .push(at);
    }

    // An index knows a module reference and nothing else, so a ref naming a
    // target no document defines is out of scope rather than a disagreement.
    let mut mine: Vec<&ImportRef> = refs
        .iter()
        .filter(|reference| {
            indexed.contains(reference.importer.as_str())
                && indexed.contains(reference.target.as_str())
                && in_batch(cx, &reference.importer, Some(&reference.target))
        })
        .collect();
    mine.sort_by_key(|reference| {
        (
            reference.importer.as_str(),
            reference.literal.start,
            reference.literal.len,
        )
    });

    let mut out = Vec::new();
    for reference in mine {
        let sited = by_importer
            .get(reference.importer.as_str())
            .into_iter()
            .flatten()
            .find(|at| !claimed[**at] && overlaps(sites[**at].span, reference.literal));
        match sited {
            Some(at) => claimed[*at] = true,
            None => out.push(ScipDisagreement {
                importer: reference.importer.clone(),
                span: reference.literal,
                kind: UNKNOWN_TO_SCIP,
                detail: format!(
                    "{} {} -> {}",
                    reference.kind, reference.text, reference.target
                ),
            }),
        }
    }
    for (at, site) in sites.iter().enumerate() {
        if claimed[at] {
            continue;
        }
        out.push(ScipDisagreement {
            importer: site.importer.clone(),
            span: site.span,
            kind: MISSED_BY_IMPL,
            detail: format!(
                "{} -> {} ({})",
                site.text,
                site.target.as_deref().unwrap_or("?"),
                site.symbol
            ),
        });
    }
    out.sort_by(|left, right| {
        (&left.importer, left.span.start, left.span.len, left.kind).cmp(&(
            &right.importer,
            right.span.start,
            right.span.len,
            right.kind,
        ))
    });
    tracing::debug!(
        refs = refs.len(),
        sites = sites.len(),
        disagreements = out.len(),
        "move scip verify"
    );
    out
}

/// Both sides carry the SAME scope or the report is noise: a `Rehome` impl
/// answers for the batch only (`lang/ts_rehome.rs:176` drops a relative
/// specifier that cannot name a moved file) while an index carries every import
/// in the corpus.
/// @comment-ok: the symmetry is why this exists and no signature shows it
fn in_batch(cx: &MoveCx, importer: &str, target: Option<&str>) -> bool {
    let Some(target) = target else {
        return false;
    };
    cx.contains(importer)
        && cx.contains(target)
        && (cx.destination(importer).is_some() || cx.destination(target).is_some())
}

/// Whether `span`'s bytes open and close with the same quote: what separates a
/// module specifier from the identifier occurrences around it, with no parse.
fn quoted_literal(content: &[u8], span: Span) -> bool {
    if span.len < 2 || span.end() as usize > content.len() {
        return false;
    }
    let bytes = &content[span.start as usize..span.end() as usize];
    matches!(bytes.first(), Some(b'"' | b'\'' | b'`')) && bytes.first() == bytes.last()
}

fn overlaps(left: Span, right: Span) -> bool {
    left.start < right.end() && right.start < left.end()
}
