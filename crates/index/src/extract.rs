use std::path::Path;

use anyhow::Result;
use memmap2::Mmap;
use rayon::prelude::*;
use xxhash_rust::xxh3::xxh3_128;

use sprefa_config::CompiledFilter;
use sprefa_extract::{Extractor, RawRef};

use crate::files::list_files;

pub struct ExtractedFile {
    pub rel_path: String,
    pub content_hash: String,
    pub stem: Option<String>,
    pub ext: Option<String>,
    pub refs: Vec<RawRef>,
}

/// Walk `repo_path`, run extractors in parallel, return (total_files_found, extracted).
/// Files with no refs are excluded from the returned vec.
pub fn extract(
    repo_path: &Path,
    filter: Option<&CompiledFilter>,
    extractors: &[Box<dyn Extractor>],
) -> Result<(usize, Vec<ExtractedFile>)> {
    let files = list_files(repo_path, filter)?;
    let total = files.len();

    let extracted: Vec<ExtractedFile> = files
        .par_iter()
        .filter_map(|abs_path| {
            let rel = abs_path.strip_prefix(repo_path).ok()?.to_str()?;
            let ext = abs_path.extension().and_then(|e| e.to_str());
            let extractor = ext.and_then(|e| {
                extractors
                    .iter()
                    .find(|ex| ex.extensions().contains(&e))
                    .map(|ex| ex.as_ref())
            })?;
            let file = std::fs::File::open(abs_path).ok()?;
            let mmap = unsafe { Mmap::map(&file).ok()? };
            let hash = format!("{:x}", xxh3_128(&mmap));
            let refs = extractor.extract(&mmap, rel);
            if refs.is_empty() {
                return None;
            }
            Some(ExtractedFile {
                rel_path: rel.to_string(),
                content_hash: hash,
                stem: abs_path.file_stem().and_then(|s| s.to_str()).map(String::from),
                ext: ext.map(String::from),
                refs,
            })
        })
        .collect();

    Ok((total, extracted))
}
