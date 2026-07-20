use super::*;

/// The module name a file path answers to in an import specifier: the file
/// stem, except `mod.rs`/`index.*`/`lib.rs`/`main.rs` answer to their parent
/// directory's name. Used by `definition_targets` to pair a specifier segment
/// with a `module_edge` dst.
pub(crate) fn module_stem(path: &str) -> &str {
    let file = path.rsplit('/').next().unwrap_or(path);
    let stem = file.split('.').next().unwrap_or(file);
    if matches!(stem, "mod" | "index" | "lib" | "main") {
        path.rsplit('/').nth(1).unwrap_or(stem)
    } else {
        stem
    }
}

/// Enumerate (path, hash, mtime, size) for one repo×rev against the UNION of
/// that group's rule globs — one walk / one `ls-tree` per repo×rev per tick,
/// however many rules scan it. Free function (no `&self`) so groups enumerate
/// in parallel across repos. For WORK, stat each file and reuse the stored hash
/// when mtime+size are unchanged (the fast-path), reading+hashing only changed
/// files.
///
/// Racy-write guard: mtime here is whole-second resolution (`mtime_secs`), so
/// an edit that lands in the same filesystem-timestamp second as a prior write
/// — same content length too — is invisible to the mtime+size compare alone.
/// `walk_ref_secs` is the wall-clock second as of the walk that produced
/// `prev` (persisted by the caller via `save_walk_ref_secs`, mirroring git's
/// racy-index check and watchman's cookie sync): a cached row whose mtime is
/// `>= walk_ref_secs` was captured in or after the same tick its own walk
/// completed in, so a same-tick rewrite immediately after that capture cannot
/// be ruled out — the fast path is skipped and the file is rehashed. This is
/// deliberately whole-second, not nanosecond: `st_mtime` nanosecond fields
/// (APFS's `st_mtimespec`) are real where the filesystem supports them, but a
/// zero or coarse sub-second component elsewhere doesn't mean "exactly on the
/// second," so the guard can't assume finer-than-second precision is
/// meaningful and must stay conservative at the resolution `mtime_secs`
/// actually stores. Self-healing: once wall-clock time advances past a row's
/// mtime second, `mtime < walk_ref_secs` holds again and the fast path
/// resumes for that file. A git rev uses the blob OID from `ls-tree`, so
/// unchanged blobs are detected without fetching content; the guard does not
/// apply there (mtime is always 0 for that arm). The walk skips `.git` explicitly:
/// `hidden(false)` un-hides it, and crawling the object store made big-repo
/// scans pathological. A directory below the root that itself owns a `.git`
/// entry (dir or file — a submodule worktree's is a file) is a foreign repo
/// and is pruned the same way: the `git ls-tree` arm below already excludes
/// submodules for free (gitlink entries are type `commit`, not `blob`), so
/// this closes the WORK-arm asymmetry. Depth 0 is `repo_root` itself and is
/// never pruned by this check (it owns the `.git` we're walking FROM).
/// Once-per-full-scan corpus sanity: total files/bytes, the top-3 dirs by
/// file count, and a loud line if any single dir carries more than
/// `DIR_SHARE_WARN_PCT`% of the corpus (e.g. a vendored/generated tree the
/// scan glob should have excluded). Called once from the WORK arm of
/// `enumerate_with_hash` per repo, never per-file — corpus-sanity is a
/// scan-level verdict, not a hot-loop one.
pub(crate) fn emit_corpus_scan_verdict(repo: &str, files: &[(String, String, i64, i64, i64)]) {
    if files.is_empty() {
        return;
    }
    let total_files = files.len();
    let total_bytes: i64 = files.iter().map(|(_, _, _, sz, _)| *sz).sum();
    let mut per_dir: HashMap<&str, usize> = HashMap::new();
    for (rel, _, _, _, _) in files {
        let dir = rel.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
        *per_dir.entry(dir).or_insert(0) += 1;
    }
    let mut dirs: Vec<(&str, usize)> = per_dir.into_iter().collect();
    dirs.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    let top3: Vec<String> = dirs
        .iter()
        .take(3)
        .map(|(dir, n)| format!("{}:{n}", if dir.is_empty() { "." } else { dir }))
        .collect();
    let msg = format!(
        "[corpus] {repo}: {total_files} files, {total_bytes} bytes, top dirs: {}",
        top3.join(", ")
    );
    crate::verdict::verdict(
        "corpus-scan",
        &msg,
        &[
            ("repo", repo),
            ("files", &total_files.to_string()),
            ("bytes", &total_bytes.to_string()),
            ("top_dirs", &top3.join(",")),
        ],
    );
    if let Some((dir, n)) = dirs.first() {
        let pct = (*n as u64 * 100) / total_files as u64;
        if pct as u32 > crate::verdict::DIR_SHARE_WARN_PCT {
            let dir_label = if dir.is_empty() { "." } else { dir };
            let warn_msg = format!(
                "[corpus] {repo}: WARNING dir {dir_label} carries {pct}% of {total_files} files (over {}%)",
                crate::verdict::DIR_SHARE_WARN_PCT
            );
            crate::verdict::verdict(
                "corpus-scan",
                &warn_msg,
                &[
                    ("repo", repo),
                    ("dir", dir_label),
                    ("pct", &pct.to_string()),
                    ("outcome", "dir-share-warn"),
                ],
            );
        }
    }
}

/// Count lines the way `wc -l` semantics-adjacent editors expect: an empty
/// file is 0 lines; a file with content but no trailing newline still counts
/// its last (unterminated) line. Counts `\n` bytes and adds one more unless
/// the file already ends on a newline — no lossy String allocation, works on
/// raw bytes so binary-ish files don't panic on invalid UTF-8.
pub(crate) fn count_lines(bytes: &[u8]) -> i64 {
    if bytes.is_empty() {
        return 0;
    }
    let newlines = bytes.iter().filter(|&&b| b == b'\n').count() as i64;
    if bytes.last() == Some(&b'\n') {
        newlines
    } else {
        newlines + 1
    }
}

// ARCH {"url":"engine/10-scan","role":"file-discovery"}
pub(crate) fn enumerate_with_hash(
    repo: &str,
    repo_root: &Path,
    rev: &RevId,
    union: &globset::GlobSet,
    prev: &FileMeta,
    walk_ref_secs: i64,
) -> Result<(Vec<(String, String, i64, i64, i64)>, usize)> {
    let max_size = max_filesize();
    // The cache probe below keys on the STORED rev text, which is now an oid,
    // never the alias. Probing a literal here would miss every `_file` row and
    // re-read + re-hash the whole corpus on every tick, silently.
    let rev_text = rev.text();
    if rev.is_worktree() {
        let mut files: Vec<(PathBuf, String, i64, i64)> = Vec::new();
        let mut walk = ignore::WalkBuilder::new(repo_root);
        walk.hidden(false).filter_entry(|e| {
            if e.file_name() == ".git" {
                return false;
            }
            // One extra stat per walked DIRECTORY; file entries skip the check.
            if e.depth() >= 1
                && e.file_type().is_some_and(|ft| ft.is_dir())
                && e.path().join(".git").exists()
            {
                return false;
            }
            true
        });
        // The walker crate caps oversized files itself (skips them before we ever
        // hash), so a single minified/vendored blob can't blow RSS. Opt-in via
        // `DL_MAX_FILESIZE` (bytes); unset = no cap (legacy behavior).
        if let Some(cap) = max_size {
            walk.max_filesize(Some(cap));
        }
        let walk = walk.build();
        for entry in walk.flatten() {
            if !entry.path().is_file() {
                continue;
            }
            let rel = match entry.path().strip_prefix(repo_root) {
                Ok(r) => r,
                Err(_) => continue,
            };
            let rel = rel.to_string_lossy().replace('\\', "/");
            if !union.is_match(&rel) {
                continue;
            }
            let (mt, sz) = entry
                .metadata()
                .ok()
                .map(|m| (mtime_secs(&m), m.len() as i64))
                .unwrap_or((0, 0));
            files.push((entry.path().to_path_buf(), rel, mt, sz));
        }
        // reuse stored hash + line count when mtime+size match; otherwise
        // read+hash+count (parallel). A stored line count of -1 (unknown: an
        // old row from before this column existed) still forces one read on
        // an otherwise-unchanged file, purely to count lines — the hash is
        // NOT recomputed, so this is a one-time cost per file, not a repeat.
        // Files this walk actually read and re-hashed, i.e. the `(repo, path,
        // rev)` probes that MISSED the prior `_file` set. Instrumentation, not
        // policy: the probe key carries the RESOLVED rev, and a key naming the
        // `WORK` alias instead would miss every oid-bearing row and re-read the
        // whole corpus every tick, silently, since a re-hash of unchanged bytes
        // yields the identical hash and moves no digest. Counted per walk rather
        // than in a process global so parallel tests do not read each other's.
        let rehashed = std::sync::atomic::AtomicUsize::new(0);
        let mut out: Vec<(String, String, i64, i64, i64)> = files
            .par_iter()
            .map(|(abs, rel, mt, sz)| {
                if let Some((h, pmt, psz, plines)) =
                    prev.get(&(repo.to_string(), rel.clone(), rev_text.clone()))
                {
                    if pmt == mt && psz == sz && *pmt < walk_ref_secs {
                        if *plines >= 0 {
                            return (rel.clone(), h.clone(), *mt, *sz, *plines);
                        }
                        let bytes = std::fs::read(abs).unwrap_or_default();
                        return (rel.clone(), h.clone(), *mt, *sz, count_lines(&bytes));
                    }
                }
                rehashed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let bytes = std::fs::read(abs).unwrap_or_default();
                (
                    rel.clone(),
                    blake3::hash(&bytes).to_hex().to_string(),
                    *mt,
                    *sz,
                    count_lines(&bytes),
                )
            })
            .collect();
        out.sort();
        emit_corpus_scan_verdict(repo, &out);
        Ok((out, rehashed.load(std::sync::atomic::Ordering::Relaxed)))
    } else {
        // `git ls-tree -r -l <rev>` lines:
        // "<mode> <type> <oid> <size>\t<path>"
        let output = Command::new("git")
            .arg("-C")
            .arg(repo_root)
            .args(["ls-tree", "-r", "-l", rev.git_oid().as_str()])
            .output()?;
        if !output.status.success() {
            return Ok((Vec::new(), 0));
        }
        let text = String::from_utf8_lossy(&output.stdout);
        let mut out = Vec::new();
        for line in text.lines() {
            let Some((meta, path)) = line.split_once('\t') else {
                continue;
            };
            let parts: Vec<&str> = meta.split_whitespace().collect();
            if parts.get(1) != Some(&"blob") {
                continue;
            }
            let oid = parts.get(2).copied().unwrap_or_default();
            let size = parts
                .get(3)
                .and_then(|s| s.parse::<i64>().ok())
                .unwrap_or(0);
            // Same size cap as the WORK walker, applied to blob sizes from ls-tree.
            if let Some(cap) = max_size {
                if size as u64 > cap {
                    continue;
                }
            }
            // Line count is left unknown (-1) for git-rev blobs: counting them
            // would spawn a read per blob, and the file-size rail only needs
            // WORK. See `file_lines`'s doc string.
            if union.is_match(path) {
                out.push((path.to_string(), oid.to_string(), 0, size, -1));
            }
        }
        // A git rev reads blob oids from `ls-tree`; nothing is hashed here.
        Ok((out, 0))
    }
}

/// Resolve a gen write/splice target against the first candidate root where it
/// already lives. Candidates are `self.root` plus each rule-origin's `.git`
/// ancestor (collected by `run_gens`), so a gen rule splicing a file scanned
/// from a loaded script's repo writes back to that repo. A new file (no candidate
/// contains it) falls back to `self.root` (the first candidate), preserving the
/// original behavior for foreground file-emits.
pub(crate) fn resolve_write_full(write_roots: &[PathBuf], p: &str) -> PathBuf {
    for r in write_roots {
        let f = r.join(p);
        if f.exists() {
            return f;
        }
    }
    write_roots
        .first()
        .map(|r| r.join(p))
        .unwrap_or_else(|| PathBuf::from(p))
}
