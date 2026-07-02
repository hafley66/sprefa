//! `dl index` / `dl doctor` — turnkey SCIP generation across languages.
//!
//! Syntactic facts (`match`/`ast`/`sg`/`json`) are embedded, in-process, and
//! git-native (any rev). SCIP facts are compiler-native: an external per-language
//! indexer runs over the WORKING TREE and drops an `index.scip` the engine
//! imports into the `scip_*` relations. That boundary is real and can't be
//! hidden; the job here is to make crossing it one command and to make every
//! silent failure (wrong indexer, wrong dir, stale index, committed 150MB blob)
//! a visible line.
//!
//!   dl index [DIR]            detect language(s), run the right indexer(s),
//!                             place the merged index at <root>/.dl/index.scip
//!   dl index --install        print/run the install command for a missing indexer
//!   dl index --rev <REV>      worktree that rev, index it (SCIP can't read a blob)
//!   dl doctor [DIR]           one screen: langs, indexers, index freshness,
//!                             path-join sanity, scip_* row counts
//!
//! The index is written INTO `.dl/` (gitignored) and run with cwd = root, so its
//! `relative_path` keys always join the paths the scanners see. Both fixes are
//! structural: no naked blob at the repo root, no path-join mismatch.
//!
//! EXPLOSION CONTRACT (matters on a daemon machine watching hundreds of repos):
//! generation is always EXPLICIT and SINGLE-ROOT. `dl index` runs one indexer
//! pass over one project; it never fans over the daemon's watched set, the
//! config repos, or nested repos, and NOTHING (not the daemon, not the reload
//! gate, not a `scan("*")` fan-out) ever generates an index automatically. The
//! daemon only IMPORTS an `index.scip` that already exists. Lazy = load-if-
//! present, never generate-on-demand. `run_index` refuses an aggregation dir
//! (the XDG serving home, or a folder of checked-out repos) unless `--force`, so
//! a stray marker file can't turn one command into 500 rust-analyzer runs.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// One language's SCIP indexer: how to detect the language, the binary to look
/// for on PATH, the one-line install hint, and the argv to run (cwd = root,
/// `{out}` substituted with the absolute output path).
struct Indexer {
    lang: &'static str,
    /// Any-of marker files at the root that identify the language/build.
    markers: &'static [&'static str],
    /// The indexer executable to probe on PATH.
    bin: &'static str,
    /// One-line install command (printed, or run with `--install`).
    install: &'static str,
    /// argv for the indexer; `{out}` -> absolute output path. Run with cwd=root.
    run: &'static [&'static str],
}

const INDEXERS: &[Indexer] = &[
    Indexer {
        lang: "rust",
        markers: &["Cargo.toml"],
        bin: "rust-analyzer",
        install: "rustup component add rust-analyzer",
        run: &["rust-analyzer", "scip", ".", "--output", "{out}"],
    },
    Indexer {
        lang: "typescript",
        markers: &["tsconfig.json", "package.json"],
        bin: "scip-typescript",
        install: "npm install -g @sourcegraph/scip-typescript",
        run: &["scip-typescript", "index", "--output", "{out}"],
    },
    Indexer {
        lang: "python",
        markers: &["pyproject.toml", "setup.py", "requirements.txt"],
        bin: "scip-python",
        install: "npm install -g @sourcegraph/scip-python",
        run: &["scip-python", "index", ".", "--output", "{out}"],
    },
    Indexer {
        lang: "go",
        markers: &["go.mod"],
        bin: "scip-go",
        install: "go install github.com/sourcegraph/scip-go/cmd/scip-go@latest",
        run: &["scip-go", "--output", "{out}"],
    },
    Indexer {
        lang: "kotlin/java",
        markers: &["build.gradle.kts", "build.gradle", "pom.xml"],
        bin: "scip-java",
        install: "coursier install --contrib scip-java  (see sourcegraph/scip-java)",
        run: &["scip-java", "index", "--output", "{out}"],
    },
    Indexer {
        lang: "cpp",
        markers: &["compile_commands.json", "CMakeLists.txt"],
        bin: "scip-clang",
        install: "download from github.com/sourcegraph/scip-clang/releases",
        run: &["scip-clang", "--compdb-path", "compile_commands.json", "-o", "{out}"],
    },
];

/// Child directories under `root` that are their own git repos (contain `.git`).
/// A single project/workspace has none — one `.git` at the root, no nested ones;
/// a Cargo workspace with 500 member crates is still one repo. A directory of
/// checked-out repos has many. Returns None when there are none (the normal,
/// safe-to-index case). Scans one level only — no deep walk.
fn nested_repos(root: &Path) -> Option<Vec<String>> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(root).ok()?.flatten() {
        let p = e.path();
        if p.is_dir() && p.join(".git").exists() {
            out.push(e.file_name().to_string_lossy().into_owned());
        }
    }
    (!out.is_empty()).then_some(out)
}

/// The indexers whose marker files are present at `root`.
fn detect(root: &Path) -> Vec<&'static Indexer> {
    INDEXERS
        .iter()
        .filter(|ix| ix.markers.iter().any(|m| root.join(m).exists()))
        .collect()
}

/// Is the indexer's binary on PATH?
fn installed(bin: &str) -> bool {
    which(bin).is_some()
}

/// First PATH entry containing an executable named `bin` (best-effort `which`).
fn which(bin: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let cand = dir.join(bin);
        if cand.is_file() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(meta) = cand.metadata() {
                    if meta.permissions().mode() & 0o111 != 0 {
                        return Some(cand);
                    }
                }
            }
            #[cfg(not(unix))]
            {
                return Some(cand);
            }
        }
    }
    None
}

fn resolve_dir(args: &[String]) -> Result<(PathBuf, Args)> {
    let mut parsed = Args::default();
    let mut dir: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--install" => parsed.install = true,
            "--force" | "-f" => parsed.force = true,
            "-h" | "--help" => parsed.help = true,
            "--rev" => { i += 1; parsed.rev = args.get(i).cloned(); }
            "--root" => { i += 1; dir = args.get(i).cloned(); }
            other if !other.starts_with('-') => dir = Some(other.to_string()),
            other => { parsed.unknown = Some(other.to_string()); }
        }
        i += 1;
    }
    let dir = dir.unwrap_or_else(|| ".".to_string());
    let root = PathBuf::from(&dir)
        .canonicalize()
        .with_context(|| format!("no such dir: {dir}"))?;
    Ok((root, parsed))
}

#[derive(Default)]
struct Args {
    install: bool,
    force: bool,
    help: bool,
    rev: Option<String>,
    unknown: Option<String>,
}

/// The absolute output path `dl index` writes and the auto-loader reads.
fn index_out(root: &Path) -> PathBuf {
    root.join(".dl").join("index.scip")
}

/// Ensure `<root>/.dl/.gitignore` ignores the generated index, so a turnkey
/// generate never leaves a committable blob in the worktree.
fn gitignore_index(root: &Path) {
    let gi = root.join(".dl").join(".gitignore");
    let existing = std::fs::read_to_string(&gi).unwrap_or_default();
    if existing.lines().any(|l| l.trim() == "index.scip" || l.trim() == "index.scip*") {
        return;
    }
    let mut out = existing;
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str("index.scip*\n");
    let _ = std::fs::create_dir_all(root.join(".dl"));
    let _ = std::fs::write(&gi, out);
}

// ── dl index ──────────────────────────────────────────────────────────────────

pub fn run_index(args: &[String]) -> Result<i32> {
    let (root, opts) = resolve_dir(args)?;
    if opts.help {
        print_index_help();
        return Ok(0);
    }
    if let Some(u) = opts.unknown {
        eprintln!("dl index: unknown arg {u}");
        print_index_help();
        return Ok(2);
    }
    if opts.rev.is_some() {
        // SCIP indexes the working tree, not a git blob. A rev index means:
        // worktree that rev elsewhere, index there, copy the index back. Guide
        // the user rather than silently indexing the wrong (current) tree.
        let rev = opts.rev.unwrap();
        println!("[dl index] SCIP indexes the working tree; it cannot read a git blob directly.");
        println!("[dl index] to index rev {rev}, worktree it and index there:");
        println!("             git worktree add /tmp/dl-{rev} {rev}");
        println!("             dl index /tmp/dl-{rev}");
        println!("             cp /tmp/dl-{rev}/.dl/index.scip {}", index_out(&root).display());
        println!("             git worktree remove /tmp/dl-{rev}");
        return Ok(0);
    }

    // Explosion guard: this launches an external, whole-tree indexer. On a
    // daemon machine that watches hundreds of repos, pointing it at an
    // aggregation dir (the serving home, or a folder of checked-out repos) would
    // fan the indexer over everything. Refuse those unless --force.
    if root == crate::daemon::daemon_home() && !opts.force {
        eprintln!("[dl index] {} is the daemon serving home, not a single project.", root.display());
        eprintln!("[dl index] cd into a specific repo and run `dl index` there (or pass --force).");
        return Ok(2);
    }
    if let Some(repos) = nested_repos(&root) {
        if !opts.force {
            eprintln!("[dl index] {} looks like a collection of {} repos (nested .git dirs):",
                root.display(), repos.len());
            for r in repos.iter().take(5) { eprintln!("             {r}"); }
            if repos.len() > 5 { eprintln!("             … and {} more", repos.len() - 5); }
            eprintln!("[dl index] indexing here would run the indexer over every repo. Run `dl index`");
            eprintln!("[dl index] inside one repo, or pass --force to index this whole tree deliberately.");
            return Ok(2);
        }
    }

    let found = detect(&root);
    if found.is_empty() {
        eprintln!("[dl index] no known language markers under {}", root.display());
        eprintln!("[dl index] looked for: {}", INDEXERS.iter()
            .flat_map(|i| i.markers.iter().copied()).collect::<Vec<_>>().join(", "));
        return Ok(1);
    }
    println!("[dl index] detected: {}", found.iter().map(|i| i.lang).collect::<Vec<_>>().join(", "));

    // Missing indexers: --install runs the install command, otherwise print it.
    let missing: Vec<&&Indexer> = found.iter().filter(|ix| !installed(ix.bin)).collect();
    if !missing.is_empty() {
        for ix in &missing {
            if opts.install {
                println!("[dl index] installing {} for {}: {}", ix.bin, ix.lang, ix.install);
                run_install(ix.install);
            } else {
                println!("[dl index] {} not found (for {}). install with:", ix.bin, ix.lang);
                println!("             {}", ix.install);
            }
        }
        if !opts.install {
            println!("[dl index] re-run with --install to run these, or install by hand, then `dl index`.");
        }
    }

    // Run each present indexer to a per-language temp file, then merge.
    let dl_dir = root.join(".dl");
    std::fs::create_dir_all(&dl_dir)?;
    let out = index_out(&root);
    let mut parts: Vec<PathBuf> = Vec::new();
    let mut ran_any = false;
    for ix in &found {
        if !installed(ix.bin) {
            continue;
        }
        let part = dl_dir.join(format!("index.{}.scip", ix.lang.replace('/', "_")));
        println!("[dl index] {} -> {} (first run can take a minute or two; no progress bar)",
            ix.bin, part.display());
        if run_indexer(ix, &root, &part)? {
            ran_any = true;
            parts.push(part);
        } else {
            eprintln!("[dl index] {} failed; skipping {}", ix.bin, ix.lang);
        }
    }

    if !ran_any {
        eprintln!("[dl index] no indexer ran; nothing written.");
        return Ok(1);
    }

    // Place the index where the auto-loader finds it (single vs merged).
    if parts.len() == 1 {
        std::fs::rename(&parts[0], &out)
            .or_else(|_| std::fs::copy(&parts[0], &out).map(|_| ()))?;
    } else {
        let n = crate::scip_import::merge_files(&parts, &out)?;
        println!("[dl index] merged {} language indexes ({n} documents)", parts.len());
        for p in &parts {
            let _ = std::fs::remove_file(p);
        }
    }
    gitignore_index(&root);

    // Confirm with row counts so the user sees SCIP actually loaded.
    match crate::scip_import::load(&out) {
        Ok(rows) => {
            println!("[dl index] wrote {} ({})", out.display(), human_size(&out));
            println!("[dl index] scip_def={} scip_ref={} scip_edge={} scip_fn_edge={}",
                rows.defs.len(), rows.refs.len(), rows.edges.len(), rows.fn_edges.len());
            if !path_join_ok(&root, &rows) {
                eprintln!("[dl index] WARNING: index paths do not join under {} — \
                    the scip_* rules will return empty. Run `dl index` from the same \
                    dir you pass as --root.", root.display());
            }
        }
        Err(e) => eprintln!("[dl index] wrote {} but failed to read it back: {e}", out.display()),
    }
    println!("[dl index] scip_* relations now load automatically for `dl … --root {}`.", root.display());
    Ok(0)
}

/// Ensure `root` has a loadable SCIP index, for the lazy `scip_want` gate: an
/// existing index (root `index.scip`, `.dl/index.scip`, or $SPREFA_SCIP_INDEX)
/// wins untouched; otherwise every detected+installed indexer runs once and the
/// parts merge to `.dl/index.scip`. Returns `None` (loudly) when no indexer can
/// produce one — a missing toolchain skips the repo, it never fails the tick.
/// No freshness check: like the CLI contract, a stale index is the user's to
/// rebuild (`dl index` / delete it); `dl doctor` reports staleness.
pub fn ensure_index(root: &Path) -> Result<Option<PathBuf>> {
    if let Some(p) = crate::scip_import::index_path(root) {
        return Ok(Some(p));
    }
    let found = detect(root);
    let present: Vec<&&Indexer> = found.iter().filter(|ix| installed(ix.bin)).collect();
    if present.is_empty() {
        eprintln!("[scip_want] {}: no installed indexer for detected markers; skipping",
            root.display());
        return Ok(None);
    }
    let dl_dir = root.join(".dl");
    std::fs::create_dir_all(&dl_dir)?;
    let out = index_out(root);
    let mut parts: Vec<PathBuf> = Vec::new();
    for ix in present {
        let part = dl_dir.join(format!("index.{}.scip", ix.lang.replace('/', "_")));
        eprintln!("[scip_want] indexing {} with {}", root.display(), ix.bin);
        if run_indexer(ix, root, &part)? {
            parts.push(part);
        } else {
            eprintln!("[scip_want] {} failed under {}; skipping {}", ix.bin, root.display(), ix.lang);
        }
    }
    if parts.is_empty() {
        return Ok(None);
    }
    if parts.len() == 1 {
        std::fs::rename(&parts[0], &out)
            .or_else(|_| std::fs::copy(&parts[0], &out).map(|_| ()))?;
    } else {
        crate::scip_import::merge_files(&parts, &out)?;
        for p in &parts {
            let _ = std::fs::remove_file(p);
        }
    }
    gitignore_index(root);
    Ok(Some(out))
}

/// Run one indexer with cwd=root and `{out}` -> abs output path. Inherits stdio
/// so the indexer's own progress is visible. Returns whether it exited 0.
fn run_indexer(ix: &Indexer, root: &Path, out: &Path) -> Result<bool> {
    let out_abs = out.to_string_lossy().into_owned();
    let argv: Vec<String> = ix.run.iter()
        .map(|a| if *a == "{out}" { out_abs.clone() } else { a.to_string() })
        .collect();
    let status = std::process::Command::new(&argv[0])
        .args(&argv[1..])
        .current_dir(root)
        .status();
    match status {
        Ok(s) => Ok(s.success()),
        Err(e) => {
            eprintln!("[dl index] could not launch {}: {e}", ix.bin);
            Ok(false)
        }
    }
}

/// Run an install command via the shell (best effort; prints the result).
fn run_install(cmd: &str) {
    let status = std::process::Command::new("sh").arg("-c").arg(cmd).status();
    match status {
        Ok(s) if s.success() => {}
        Ok(s) => eprintln!("[dl index] install command exited {s}"),
        Err(e) => eprintln!("[dl index] could not run install command: {e}"),
    }
}

fn print_index_help() {
    eprintln!("usage: dl index [DIR] [--install] [--rev REV] [--force]");
    eprintln!("  DIR (or --root DIR)  workspace root to index (default: cwd)");
    eprintln!("  --install            run the install command for any missing indexer");
    eprintln!("  --rev REV            print the worktree-and-index recipe for a git rev");
    eprintln!("  detects rust / typescript / python / go / kotlin-java / cpp by marker files,");
    eprintln!("  runs each indexer with cwd=root, merges to <root>/.dl/index.scip (gitignored).");
    eprintln!("  see `dl doctor` for the health screen.");
}

// ── dl doctor ───────────────────────────────────────────────────────────────

pub fn run_doctor(args: &[String]) -> Result<i32> {
    let (root, opts) = resolve_dir(args)?;
    if opts.help {
        eprintln!("usage: dl doctor [DIR]");
        eprintln!("  reports detected languages, indexer availability, index presence +");
        eprintln!("  freshness, path-join sanity, and scip_* row counts for the workspace.");
        return Ok(0);
    }
    println!("dl doctor — {}", root.display());

    let found = detect(&root);
    if found.is_empty() {
        println!("  languages: none detected (no Cargo.toml / tsconfig / go.mod / …)");
    } else {
        println!("  languages:");
        for ix in &found {
            let status = if installed(ix.bin) {
                format!("indexer `{}` installed", ix.bin)
            } else {
                format!("indexer `{}` MISSING — install: {}", ix.bin, ix.install)
            };
            println!("    - {:<12} {}", ix.lang, status);
        }
    }

    // Index presence + freshness.
    match crate::scip_import::index_path(&root) {
        None => {
            println!("  index:     none — scip_* relations are EMPTY; the syntactic");
            println!("             call/type graph carries. Run `dl index` for compiler precision.");
        }
        Some(path) => {
            println!("  index:     {} ({})", path.display(), human_size(&path));
            match freshness(&root, &path) {
                Freshness::Fresh => println!("             freshness: fresh (newer than HEAD)"),
                Freshness::Stale => println!("             freshness: STALE — older than HEAD; re-run `dl index`"),
                Freshness::Unknown => println!("             freshness: unknown (not a git repo or no HEAD)"),
            }
            match crate::scip_import::load(&path) {
                Ok(rows) => {
                    println!("             scip_def={} scip_ref={} scip_edge={} scip_fn_edge={} scip_impl={}",
                        rows.defs.len(), rows.refs.len(), rows.edges.len(),
                        rows.fn_edges.len(), rows.impls.len());
                    if path_join_ok(&root, &rows) {
                        println!("             path-join: ok (index paths resolve under root)");
                    } else {
                        println!("             path-join: MISMATCH — index generated under a different dir;");
                        println!("                        every scip_* rule will be empty. Re-run `dl index` here.");
                    }
                }
                Err(e) => println!("             (failed to read index: {e})"),
            }
        }
    }
    Ok(0)
}

enum Freshness { Fresh, Stale, Unknown }

/// Compare the index mtime to the HEAD commit time. Older than HEAD = stale
/// (source moved on since the index was cut). Not a git repo / no HEAD = unknown.
fn freshness(root: &Path, index: &Path) -> Freshness {
    let Ok(meta) = index.metadata() else { return Freshness::Unknown };
    let Ok(index_mtime) = meta.modified() else { return Freshness::Unknown };
    let out = std::process::Command::new("git")
        .arg("-C").arg(root)
        .args(["log", "-1", "--format=%ct"])
        .output();
    let Ok(out) = out else { return Freshness::Unknown };
    if !out.status.success() { return Freshness::Unknown; }
    let Ok(secs) = String::from_utf8_lossy(&out.stdout).trim().parse::<u64>() else {
        return Freshness::Unknown;
    };
    let head_time = std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs);
    if index_mtime >= head_time { Freshness::Fresh } else { Freshness::Stale }
}

/// True if a sample of the index's def paths resolve to real files under root —
/// the path-join sanity check that turns friction #3 (silent-empty from a wrong
/// --root) into a visible line.
fn path_join_ok(root: &Path, rows: &crate::scip_import::ScipRows) -> bool {
    if rows.defs.is_empty() {
        return true; // nothing to check; don't warn on an empty index
    }
    rows.defs.iter().take(64).any(|(_, file)| root.join(file).exists())
}

fn human_size(path: &Path) -> String {
    let bytes = path.metadata().map(|m| m.len()).unwrap_or(0);
    if bytes >= 1 << 30 {
        format!("{:.1} GB", bytes as f64 / (1u64 << 30) as f64)
    } else if bytes >= 1 << 20 {
        format!("{:.1} MB", bytes as f64 / (1u64 << 20) as f64)
    } else if bytes >= 1 << 10 {
        format!("{:.1} KB", bytes as f64 / (1u64 << 10) as f64)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("dlindex_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn detect_matches_markers() {
        let d = tmp("detect");
        std::fs::write(d.join("Cargo.toml"), "[package]\nname='x'").unwrap();
        std::fs::write(d.join("go.mod"), "module x").unwrap();
        let langs: Vec<&str> = detect(&d).iter().map(|i| i.lang).collect();
        assert!(langs.contains(&"rust"), "rust detected: {langs:?}");
        assert!(langs.contains(&"go"), "go detected: {langs:?}");
        assert!(!langs.contains(&"python"), "no python marker: {langs:?}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn detect_empty_on_bare_dir() {
        let d = tmp("bare");
        assert!(detect(&d).is_empty());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn gitignore_added_once_and_idempotent() {
        let d = tmp("gi");
        std::fs::create_dir_all(d.join(".dl")).unwrap();
        gitignore_index(&d);
        let gi = d.join(".dl/.gitignore");
        let body = std::fs::read_to_string(&gi).unwrap();
        assert!(body.contains("index.scip*"), "index ignored: {body:?}");
        // Second call must not duplicate.
        gitignore_index(&d);
        let count = std::fs::read_to_string(&gi).unwrap()
            .lines().filter(|l| l.contains("index.scip")).count();
        assert_eq!(count, 1, "idempotent");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn gitignore_preserves_existing_entries() {
        let d = tmp("gikeep");
        std::fs::create_dir_all(d.join(".dl")).unwrap();
        std::fs::write(d.join(".dl/.gitignore"), "cache.db*\n").unwrap();
        gitignore_index(&d);
        let body = std::fs::read_to_string(d.join(".dl/.gitignore")).unwrap();
        assert!(body.contains("cache.db*"), "kept cache entry: {body:?}");
        assert!(body.contains("index.scip*"), "added index entry: {body:?}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn nested_repos_flags_a_collection_not_a_single_repo() {
        // A single repo: `.git` at root, none in children -> None (safe).
        let solo = tmp("solo");
        std::fs::create_dir_all(solo.join(".git")).unwrap();
        std::fs::create_dir_all(solo.join("src")).unwrap();
        assert!(nested_repos(&solo).is_none(), "single repo is not a collection");
        // A collection: children each carry their own `.git` -> Some(list).
        let coll = tmp("coll");
        for name in ["repo_a", "repo_b", "repo_c"] {
            std::fs::create_dir_all(coll.join(name).join(".git")).unwrap();
        }
        let found = nested_repos(&coll).expect("collection flagged");
        assert_eq!(found.len(), 3, "all three nested repos seen: {found:?}");
        let _ = std::fs::remove_dir_all(&solo);
        let _ = std::fs::remove_dir_all(&coll);
    }

    #[test]
    fn every_indexer_run_has_out_placeholder() {
        // Each indexer's argv must carry {out} so the index lands where we place it.
        for ix in INDEXERS {
            assert!(ix.run.iter().any(|a| a.contains("{out}")),
                "{} run args must include {{out}}: {:?}", ix.lang, ix.run);
            assert_eq!(ix.run[0], ix.bin, "{}: argv[0] must be the probed bin", ix.lang);
        }
    }
}
