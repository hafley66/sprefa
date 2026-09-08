//! `--verify-scip <index.scip>`: a second opinion on a rename plan, read off a
//! SCIP index. It REPORTS. It never changes the plan, the stages, or the exit
//! code (user decision, 2026-08-27). No language is named here; the index and
//! the plan decide everything.
//! @comment-ok: module header, the seam list every rename file opens with

use std::collections::BTreeSet;
use std::path::Path;

use sprefa_extract::scip::{byte_range_at, LineTable};
use sprefa_extract::scip_decode::load_index;
use sprefa_extract::{OccurrenceRole, RefRole, RenameCx, RenameRequest, ScipIndex, SymbolRef};

/// One span in the corpus, as both sides of the diff spell it.
type Site = (String, u32, u32);

/// Which side of the diff a span sits on.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DisagreementSide {
    /// A plan span no occurrence of the anchor symbol covers.
    PlanOnly,
    /// An occurrence of the anchor symbol the plan missed.
    ScipOnly,
}

impl DisagreementSide {
    fn as_str(self) -> &'static str {
        match self {
            DisagreementSide::PlanOnly => "plan-only",
            DisagreementSide::ScipOnly => "scip-only",
        }
    }
}

/// One span the plan and the index disagree about.
pub struct ScipDisagreement {
    pub file: String,
    pub start: u32,
    pub end: u32,
    pub side: DisagreementSide,
}

/// Every disagreement one loaded index has with a whole batch's plan, in
/// (file, offset, side) order. ONE load per run; the batch shares it.
pub fn verify_plan(
    cx: &RenameCx,
    refs: &[Vec<SymbolRef>],
    index_path: &Path,
) -> Result<Vec<ScipDisagreement>, String> {
    let index = load_index(index_path)
        .map_err(|error| format!("verify-scip {}: {error}", index_path.display()))?;
    let mut out = Vec::new();
    for (request, found) in cx.batch().iter().zip(refs) {
        out.extend(verify_against_scip(cx, &index, request, found)?);
    }
    out.sort_by(|left, right| {
        (&left.file, left.start, left.side.as_str()).cmp(&(
            &right.file,
            right.start,
            right.side.as_str(),
        ))
    });
    Ok(out)
}

/// Where `index` and one request's plan disagree. An index answers about ONE
/// symbol, so a plan whose declaration cannot be anchored is the whole report.
pub fn verify_against_scip(
    cx: &RenameCx,
    index: &ScipIndex,
    request: &RenameRequest,
    refs: &[SymbolRef],
) -> Result<Vec<ScipDisagreement>, String> {
    let declaration = refs
        .iter()
        .find(|reference| reference.file == request.anchor && reference.role == RefRole::Definition)
        .ok_or_else(|| {
            format!(
                "verify-scip: the plan declares no {} in {}",
                request.old, request.anchor
            )
        })?;
    let declared = (
        declaration.file.clone(),
        declaration.span.start,
        declaration.span.end(),
    );

    let Some(symbol) = anchor_symbol(cx, index, request, &declared) else {
        return Ok(vec![row(declared, DisagreementSide::PlanOnly)]);
    };

    let sited = anchor_sites(cx, index, request, &symbol);
    let planned: BTreeSet<Site> = refs
        .iter()
        .map(|reference| {
            (
                reference.file.clone(),
                reference.span.start,
                reference.span.end(),
            )
        })
        .collect();

    let mut out: Vec<ScipDisagreement> = planned
        .difference(&sited)
        .map(|site| row(site.clone(), DisagreementSide::PlanOnly))
        .collect();
    out.extend(
        sited
            .difference(&planned)
            .map(|site| row(site.clone(), DisagreementSide::ScipOnly)),
    );
    tracing::debug!(
        anchor = %request.anchor,
        symbol = %symbol,
        planned = planned.len(),
        sited = sited.len(),
        disagreements = out.len(),
        "rename scip verify"
    );
    Ok(out)
}

/// One line per disagreement, then the count. The count is unconditional: a leg
/// that agreed and a leg that never ran would otherwise read the same.
pub fn report(disagreements: &[ScipDisagreement]) {
    for disagreement in disagreements {
        println!(
            "scip-verify {}:{}-{} {}",
            disagreement.file,
            disagreement.start,
            disagreement.end,
            disagreement.side.as_str()
        );
    }
    println!("scip-verify disagreements={}", disagreements.len());
}

fn row(site: Site, side: DisagreementSide) -> ScipDisagreement {
    ScipDisagreement {
        file: site.0,
        start: site.1,
        end: site.2,
        side,
    }
}

/// The DEFINITION occurrence whose bytes are exactly the plan's declaration
/// span. The name alone is not enough: one file may declare `old` twice.
/// @comment-ok: the span-not-name rule is the whole correctness of this leg
fn anchor_symbol(
    cx: &RenameCx,
    index: &ScipIndex,
    request: &RenameRequest,
    declared: &Site,
) -> Option<String> {
    let document = index
        .documents
        .iter()
        .find(|document| document.relative_path == request.anchor)?;
    let content = cx.read(&request.anchor)?;
    let lines = LineTable::build(&content);
    document
        .occurrences
        .iter()
        .filter(|occurrence| occurrence.roles.contains(OccurrenceRole::DEFINITION))
        .find(|occurrence| {
            byte_range_at(
                &content,
                &lines,
                occurrence.range,
                document.position_encoding,
            )
            .is_some_and(|span| span.start == declared.1 && span.end() == declared.2)
        })
        .map(|occurrence| index.symbol(occurrence.symbol).to_string())
}

/// Every occurrence of `symbol` whose bytes spell the old name. A document the
/// corpus does not hold never compares, so a vendored or stale path is out of
/// scope rather than a disagreement.
///
/// The spelling gate is the alias law: scip-typescript folds
/// `import {OLD as local}`'s local binding into the imported symbol, and the
/// rename leaves `local` alone by design (`lang/ts_rename.rs:277`). A seat
/// spelling a name this run never writes is not a seat.
/// @comment-ok: the gate encodes an indexer fact the signature cannot show
fn anchor_sites(
    cx: &RenameCx,
    index: &ScipIndex,
    request: &RenameRequest,
    symbol: &str,
) -> BTreeSet<Site> {
    let mut sites = BTreeSet::new();
    for document in &index.documents {
        // The read and the line table cost a document each; a document that
        // never names the symbol pays neither.
        if !document
            .occurrences
            .iter()
            .any(|occurrence| index.symbol(occurrence.symbol) == symbol)
        {
            continue;
        }
        let Some(content) = cx.read(&document.relative_path) else {
            continue;
        };
        let lines = LineTable::build(&content);
        for occurrence in &document.occurrences {
            if index.symbol(occurrence.symbol) != symbol || !is_seat_role(occurrence.roles) {
                continue;
            }
            let Some(span) = byte_range_at(
                &content,
                &lines,
                occurrence.range,
                document.position_encoding,
            ) else {
                continue;
            };
            let Some(bytes) = content.get(span.start as usize..span.end() as usize) else {
                continue;
            };
            if bytes != request.old.as_bytes() {
                continue;
            }
            sites.insert((document.relative_path.clone(), span.start, span.end()));
        }
    }
    sites
}

/// Whether an occurrence's roles let it be a seat a rename respells.
///
/// MEASURED, scip-typescript 0.4.0: `symbol_roles` is written at exactly two
/// sites, `dist/src/FileIndexer.js:80` and `:214`, and both write DEFINITION.
/// Every reference occurrence therefore carries NO bits, IMPORT included, so an
/// import-clause occurrence counts by symbol alone and a bits-only rule would
/// read one seat per corpus.
/// @comment-ok: the empty-roles arm is a measured indexer fact, not a preference
fn is_seat_role(roles: OccurrenceRole) -> bool {
    roles.0 == 0
        || roles.contains(OccurrenceRole::DEFINITION)
        || roles.contains(OccurrenceRole::READ_ACCESS)
        || roles.contains(OccurrenceRole::WRITE_ACCESS)
}
