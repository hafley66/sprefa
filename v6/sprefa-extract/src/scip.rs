//! S4 SCIP: the Tier-1 resolution wire — the `ScipSource` LOGIC (the seam
//! trait + diet types live in `crate::types`, re-exported through
//! `crate::seams`; this module is the `wire.rs`-style logic half).
//!
//! Commit 4c-i lands `ScipTypescript`:
//! - `build`: subprocess `scip-typescript index` over a directory (v5's argv,
//!   `src/scip_setup.rs` INDEXERS). HERMETIC: the index lands in a fresh temp
//!   dir, and when the root has no tsconfig.json the sources are copied to a
//!   temp workdir first — the indexer's `--infer-tsconfig` WRITES a tsconfig,
//!   and the source dir must never be mutated (fixtures are committed).
//! - `load`: prost decode of index.scip into the diet `ScipIndex` (v5
//!   `src/scip_import.rs::load`, re-runtimed: rust-protobuf -> prost, see the
//!   Cargo.toml dep note).
//!
//! The generated bindings are committed at `scip/scip_proto.rs` (from the
//! vendored `proto/scip.proto`); they stay private — only the diet types in
//! `crate::types` cross the seam.

use std::path::{Path, PathBuf};

use prost::Message;

use crate::shape::Span;
use crate::types::{
    OccurrenceRole, PositionEncoding, ScipDocument, ScipError, ScipIndex, ScipOccurrence,
    ScipSource, ScipSymbolInfo,
};

// doc(hidden): the generated rustdoc carries fenced symbol-grammar examples
// from scip.proto that are not Rust doctests; hide the module so rustdoc
// never tries to compile them.
#[doc(hidden)]
#[path = "scip/scip_proto.rs"]
mod proto;

/// scip-typescript 0.4.0 (the ledger ORACLE entry's version). `build` probes
/// PATH first (v5's `dl index` convention), then falls back to the
/// version-pinned npx form so a machine without the global install still runs
/// the same indexer release.
pub struct ScipTypescript;

/// The source extensions scip-typescript (and `TsSource`) covers; the staging
/// copy preserves these, directory structure included.
const TS_EXTS: &[&str] = &["ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs"];

impl ScipSource for ScipTypescript {
    fn indexer(&self) -> &'static str {
        "scip-typescript"
    }

    fn build(&self, root: &Path) -> Result<PathBuf, ScipError> {
        let stage = fresh_temp_dir("sprefa-scip")?;
        let out = stage.join("index.scip");
        // Hermetic staging: with no tsconfig at the root, the indexer's
        // --infer-tsconfig would WRITE one into the source dir. Copy the
        // sources to a temp workdir and let it write there instead. With a
        // tsconfig present the indexer writes nothing but the (redirected)
        // output, so the root is used in place (no copying real projects).
        let work = if root.join("tsconfig.json").is_file() {
            root.to_path_buf()
        } else {
            let work = stage.join("work");
            copy_sources(root, &work)?;
            work
        };
        let out_str = out.to_string_lossy().into_owned();
        let argv: [&str; 4] = ["index", "--infer-tsconfig", "--output", out_str.as_str()];
        // PATH first (v5's `dl index` convention); a spawn miss falls back to
        // the version-pinned npx form (the ORACLE entry ran 0.4.0). A PATH
        // binary that runs and fails is reported, not retried.
        if let Ok(done) = std::process::Command::new("scip-typescript")
            .args(argv)
            .current_dir(&work)
            .output()
        {
            return if done.status.success() {
                Ok(out)
            } else {
                Err(ScipError::IndexerFailed(tail(&done.stderr)))
            };
        }
        let done = std::process::Command::new("npx")
            .args(["-y", "@sourcegraph/scip-typescript@0.4.0"])
            .args(argv)
            .current_dir(&work)
            .output()
            .map_err(|_| ScipError::IndexerMissing("scip-typescript"))?;
        if done.status.success() {
            Ok(out)
        } else {
            Err(ScipError::IndexerFailed(tail(&done.stderr)))
        }
    }

    fn load(&self, index_path: &Path) -> Result<ScipIndex, ScipError> {
        let bytes = std::fs::read(index_path)
            .map_err(|e| ScipError::Parse(format!("read {}: {e}", index_path.display())))?;
        let index = proto::Index::decode(bytes.as_slice())
            .map_err(|e| ScipError::Parse(format!("protobuf decode: {e}")))?;
        Ok(diet(&index))
    }
}

/// The last nonempty stderr line (the indexer's own error line), trimmed.
fn tail(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    text.lines().filter(|l| !l.trim().is_empty()).last().unwrap_or("").trim().to_string()
}

/// A fresh uniquely-named temp dir (no tempfile dep): base + pid + nanos.
fn fresh_temp_dir(prefix: &str) -> Result<PathBuf, ScipError> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&dir)
        .map_err(|e| ScipError::IndexerFailed(format!("mktemp {}: {e}", dir.display())))?;
    Ok(dir)
}

/// Copy the TS/JS sources under `src_root` to `dst_root`, preserving relative
/// structure; node_modules/.git and friends are not sources. Used only when
/// the root has no tsconfig (see `build`).
fn copy_sources(src_root: &Path, dst_root: &Path) -> Result<(), ScipError> {
    let mut stack = vec![src_root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir)
            .map_err(|e| ScipError::IndexerFailed(format!("read_dir {}: {e}", dir.display())))?;
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if path.is_dir() {
                if !matches!(name.as_ref(), "node_modules" | ".git" | "dist" | "out") {
                    stack.push(path);
                }
                continue;
            }
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if !TS_EXTS.contains(&ext) {
                continue;
            }
            let rel = path.strip_prefix(src_root).unwrap_or(&path);
            let dst = dst_root.join(rel);
            if let Some(parent) = dst.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| ScipError::IndexerFailed(format!("mkdir: {e}")))?;
            }
            std::fs::copy(&path, &dst)
                .map_err(|e| ScipError::IndexerFailed(format!("copy: {e}")))?;
        }
    }
    Ok(())
}

/// proto -> diet: keep symbol + range + role (+ display_name/kind on symbol
/// infos, position_encoding on documents, the tool identity); drop docs,
/// diagnostics, signatures, relationships, syntax kinds.
fn diet(index: &proto::Index) -> ScipIndex {
    let symbol = |si: &proto::SymbolInformation| ScipSymbolInfo {
        symbol: si.symbol.clone(),
        display_name: si.display_name.clone(),
        kind: si.kind,
    };
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
                occurrences: doc
                    .occurrences
                    .iter()
                    .filter_map(|occ| {
                        Some(ScipOccurrence {
                            symbol: occ.symbol.clone(),
                            range: occurrence_range(occ)?,
                            roles: OccurrenceRole(occ.symbol_roles),
                        })
                    })
                    .collect(),
                symbols: doc.symbols.iter().map(symbol).collect(),
            })
            .collect(),
        external_symbols: index.external_symbols.iter().map(symbol).collect(),
        tool: index
            .metadata
            .as_ref()
            .and_then(|m| m.tool_info.as_ref())
            .map(|t| format!("{} {}", t.name, t.version))
            .unwrap_or_default(),
    }
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
        None => match occ.range.as_slice() {
            [sl, sc, el, ec] => Some([*sl, *sc, *el, *ec]),
            [sl, sc, ec] => Some([*sl, *sc, *sl, *ec]),
            _ => None,
        },
    }
}

/// The line/col -> byte bridge. SCIP ranges are 0-based (line, col) with cols
/// in the document's `PositionEncoding`; v6 `Span` is byte offsets. The
/// consumer holds the content, so the conversion lives here as a pure fn.
/// `Unspecified` is UTF-16 per the SCIP spec; a col landing mid-character or
/// past the line end is None (malformed range, never clamped into a lie).
pub fn byte_range(content: &[u8], range: [i32; 4], encoding: PositionEncoding) -> Option<Span> {
    let line_start = |line: i32| -> Option<usize> {
        if line < 0 {
            return None;
        }
        let mut seen = 0i32;
        for (ix, &b) in content.iter().enumerate() {
            if seen == line {
                return Some(ix);
            }
            if b == b'\n' {
                seen += 1;
            }
        }
        if seen == line { Some(content.len()) } else { None }
    };
    let byte_col = |line: i32, col: i32| -> Option<u32> {
        if col < 0 {
            return None;
        }
        let start = line_start(line)?;
        let line_end = content[start..]
            .iter()
            .position(|&b| b == b'\n')
            .map(|p| start + p)
            .unwrap_or(content.len());
        let text = std::str::from_utf8(&content[start..line_end]).ok()?;
        let col = col as usize;
        let within = match encoding {
            PositionEncoding::Utf8 => (col <= text.len()).then_some(col),
            PositionEncoding::Unspecified | PositionEncoding::Utf16 => {
                let mut acc = 0usize;
                let mut hit = None;
                for (i, c) in text.char_indices() {
                    if acc == col {
                        hit = Some(i);
                        break;
                    }
                    acc += c.len_utf16();
                }
                hit.or(if acc == col { Some(text.len()) } else { None })
            }
            PositionEncoding::Utf32 => {
                let mut hit = text.char_indices().nth(col).map(|(i, _)| i);
                if hit.is_none() && col == text.chars().count() {
                    hit = Some(text.len());
                }
                hit
            }
        }?;
        Some((start + within) as u32)
    };
    let start = byte_col(range[0], range[1])?;
    let end = byte_col(range[2], range[3])?;
    if end < start {
        return None;
    }
    Some(Span { start, len: end - start })
}

// ── the resolution joins (4c-ii; shared by the Resolve<CallF> arms and the
//    golden_parity scip ratchet — the arm and the test MUST read the same
//    occurrence the same way, so the conventions live here exactly once) ────

/// The content join for one loaded index: for every document, its blob hash +
/// bytes from the rev-correct reader (None when the reader can't read the
/// document — it is then external to the corpus). Parallel to
/// `index.documents` (same order, same length). Whole-project state built once
/// per refresh; the resolve arms build it per call at fixture scale (the
/// engine caches when this gets hot — the OnceLock discipline of `IndexBag`
/// covers it).
pub fn join_documents(
    index: &ScipIndex,
    reader: &dyn Fn(&str) -> Option<Vec<u8>>,
) -> Vec<Option<(crate::shape::BlobHash, Vec<u8>)>> {
    index
        .documents
        .iter()
        .map(|doc| {
            reader(&doc.relative_path)
                .map(|content| (crate::shape::BlobHash::of(&content), content))
        })
        .collect()
}

/// The scip occurrence answering one call site: contained in the site span
/// (v6's site convention — a call site is the callee expression, a new-
/// expression the whole expression; the callee identifier's occurrence sits
/// inside either) whose source text equals the callee name (the trailing
/// segment; filters the receiver/path occurrences — `Math` in `Math.sqrt` —
/// and the argument occurrences inside a new-expression). Deterministic:
/// first by (start, end).
pub fn site_occurrence<'a>(
    doc: &'a ScipDocument,
    content: &[u8],
    site: Span,
    callee: &str,
) -> Option<&'a ScipOccurrence> {
    let mut hit: Option<(&'a ScipOccurrence, [i32; 4])> = None;
    for occ in &doc.occurrences {
        let Some(span) = byte_range(content, occ.range, doc.position_encoding) else {
            continue;
        };
        if !(site.start <= span.start && span.end() <= site.end()) {
            continue;
        }
        let text = &content[span.start as usize..span.end() as usize];
        if text != callee.as_bytes() {
            continue;
        }
        if hit.map_or(true, |(_, r)| occ.range < r) {
            hit = Some((occ, occ.range));
        }
    }
    hit.map(|(occ, _)| occ)
}

/// The definition occurrence of a symbol: `local ` symbols are DOCUMENT-
/// scoped (scip reuses `local 0` per file — v5's per-document keying), so the
/// search starts and ends at the site's own document; global symbols are
/// corpus-unique, so the first definition-role occurrence across all
/// documents answers. Returns (document_ix, occurrence) — None means the
/// symbol has no definition in the indexed corpus (an EXTERNAL: a library
/// symbol, or an unresolved reference).
pub fn definition_of<'a>(
    index: &'a ScipIndex,
    doc_ix: usize,
    symbol: &str,
) -> Option<(usize, &'a ScipOccurrence)> {
    let is_def = |occ: &'a ScipOccurrence| {
        occ.symbol == symbol && occ.roles.contains(OccurrenceRole::DEFINITION)
    };
    if symbol.starts_with("local ") {
        let doc = &index.documents[doc_ix];
        return doc.occurrences.iter().find(|occ| is_def(occ)).map(|occ| (doc_ix, occ));
    }
    for (ix, doc) in index.documents.iter().enumerate() {
        if let Some(occ) = doc.occurrences.iter().find(|occ| is_def(occ)) {
            return Some((ix, occ));
        }
    }
    None
}
