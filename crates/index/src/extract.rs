use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::Result;
use memmap2::Mmap;
use rayon::prelude::*;
use xxhash_rust::xxh3::xxh3_128;

use sprefa_config::CompiledFilter;
use sprefa_extract::{ExtractContext, Extractor, RawRef};

use crate::files::list_files;

pub struct ExtractedFile {
    pub rel_path: String,
    pub content_hash: String,
    pub stem: Option<String>,
    pub ext: Option<String>,
    pub refs: Vec<RawRef>,
    /// True when (rel_path, content_hash) matched the skip set -- refs are empty
    /// because they are already in the DB from a prior scan with the same binary.
    pub was_skipped: bool,
}

/// Walk `repo_path`, run extractors in parallel, return (total_files_found, extracted).
///
/// Files in `skip_set` (keyed on `(rel_path, content_hash)`) are returned with
/// `was_skipped = true` and empty refs -- the caller must not re-insert refs for
/// them but should still update branch membership.
///
/// Files with no matching extractor AND not in the skip set are omitted entirely.
/// Shared parallel extraction over an explicit file list.
fn extract_from_list(
    repo_path: &Path,
    files: &[PathBuf],
    extractors: &[Box<dyn Extractor>],
    skip_set: &HashSet<(String, String)>,
    ctx: &ExtractContext,
) -> Vec<ExtractedFile> {
    files
        .par_iter()
        .filter_map(|abs_path| {
            let rel = abs_path.strip_prefix(repo_path).ok()?.to_str()?;
            let file = std::fs::File::open(abs_path).ok()?;
            let mmap = unsafe { Mmap::map(&file).ok()? };
            let hash = format!("{:x}", xxh3_128(&mmap));

            if skip_set.contains(&(rel.to_string(), hash.clone())) {
                return Some(ExtractedFile {
                    rel_path: rel.to_string(),
                    content_hash: hash,
                    stem: abs_path.file_stem().and_then(|s| s.to_str()).map(String::from),
                    ext: abs_path.extension().and_then(|e| e.to_str()).map(String::from),
                    refs: vec![],
                    was_skipped: true,
                });
            }

            let ext = abs_path.extension().and_then(|e| e.to_str());
            let refs: Vec<RawRef> = match ext {
                Some(e) => extractors
                    .iter()
                    .filter(|ex| ex.extensions().contains(&e))
                    .flat_map(|ex| ex.extract(&mmap, rel, ctx))
                    .collect(),
                None => return None,
            };
            if refs.is_empty() {
                return None;
            }
            Some(ExtractedFile {
                rel_path: rel.to_string(),
                content_hash: hash,
                stem: abs_path.file_stem().and_then(|s| s.to_str()).map(String::from),
                ext: ext.map(String::from),
                refs,
                was_skipped: false,
            })
        })
        .collect()
}

pub fn extract(
    repo_path: &Path,
    filter: Option<&CompiledFilter>,
    extractors: &[Box<dyn Extractor>],
    skip_set: &HashSet<(String, String)>,
    ctx: &ExtractContext,
) -> Result<(usize, Vec<ExtractedFile>)> {
    let files = list_files(repo_path, filter)?;
    let total = files.len();
    let extracted = extract_from_list(repo_path, &files, extractors, skip_set, ctx);
    Ok((total, extracted))
}

/// Extract refs from in-memory blobs (git object store content).
/// Same shape as `extract_from_list` but reads bytes directly instead of mmap.
fn extract_from_blobs(
    blobs: &[(String, Vec<u8>)],
    extractors: &[Box<dyn Extractor>],
    skip_set: &HashSet<(String, String)>,
    ctx: &ExtractContext,
) -> Vec<ExtractedFile> {
    blobs
        .par_iter()
        .filter_map(|(rel_path, content)| {
            let hash = format!("{:x}", xxh3_128(content));

            if skip_set.contains(&(rel_path.clone(), hash.clone())) {
                let p = Path::new(rel_path);
                return Some(ExtractedFile {
                    rel_path: rel_path.clone(),
                    content_hash: hash,
                    stem: p.file_stem().and_then(|s| s.to_str()).map(String::from),
                    ext: p.extension().and_then(|e| e.to_str()).map(String::from),
                    refs: vec![],
                    was_skipped: true,
                });
            }

            let p = Path::new(rel_path);
            let ext = p.extension().and_then(|e| e.to_str());
            let refs: Vec<RawRef> = match ext {
                Some(e) => extractors
                    .iter()
                    .filter(|ex| ex.extensions().contains(&e))
                    .flat_map(|ex| ex.extract(content, rel_path, ctx))
                    .collect(),
                None => return None,
            };
            if refs.is_empty() {
                return None;
            }
            Some(ExtractedFile {
                rel_path: rel_path.clone(),
                content_hash: hash,
                stem: p.file_stem().and_then(|s| s.to_str()).map(String::from),
                ext: ext.map(String::from),
                refs,
                was_skipped: false,
            })
        })
        .collect()
}

/// Extract refs from files at a specific git revision (tag, branch, sha).
/// Uses `list_blobs_at_rev` to read file content from the git object store.
pub fn extract_rev(
    repo_path: &Path,
    rev: &str,
    filter: Option<&CompiledFilter>,
    extractors: &[Box<dyn Extractor>],
    skip_set: &HashSet<(String, String)>,
    ctx: &ExtractContext,
) -> Result<(usize, Vec<ExtractedFile>)> {
    let blobs = crate::files::list_blobs_at_rev(repo_path, rev, filter)?;
    let total = blobs.len();
    let extracted = extract_from_blobs(&blobs, extractors, skip_set, ctx);
    Ok((total, extracted))
}

/// Extract refs from a specific set of files (absolute paths).
/// Same logic as `extract()` but skips the tree walk -- only processes
/// the provided file list.
pub fn extract_files(
    repo_path: &Path,
    files: Vec<PathBuf>,
    extractors: &[Box<dyn Extractor>],
    skip_set: &HashSet<(String, String)>,
    ctx: &ExtractContext,
) -> Result<(usize, Vec<ExtractedFile>)> {
    let total = files.len();
    let extracted = extract_from_list(repo_path, &files, extractors, skip_set, ctx);
    Ok((total, extracted))
}
