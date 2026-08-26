//! `extract move ...`: repair `package.json` targets (`main`, `module`,
//! `types`, `browser`, `bin`, every `exports` leaf) that name a moved file or
//! its compiled image, off the same move batch `0_move.rs` already parsed.
//! Runs after `source_move::run`, from its own arg parse (`bin/extract.rs`'s
//! dispatch region), so it never reaches into `0_move.rs`.
//! @comment-ok: module header, the seam this pass and `2_move_text.rs` share

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use clap::Parser;
use serde::Serialize;
use serde_json::Value;

/// A superset of `0_move.rs`'s `MoveCli` (adds `--text-refs`), so the same
/// argv parses in all three passes without editing `0_move.rs`.
#[derive(Parser)]
#[command(name = "extract move manifests")]
pub(crate) struct MoveArgs {
    pub old: Option<PathBuf>,
    pub new: Option<PathBuf>,
    #[arg(long)]
    pub list: Option<PathBuf>,
    #[arg(long)]
    pub root: Option<PathBuf>,
    #[arg(long)]
    pub state: Option<PathBuf>,
    #[arg(long)]
    pub commit: bool,
    #[arg(long)]
    pub shim: bool,
    #[arg(long = "text-refs")]
    pub text_refs: bool,
}

/// One move, root-relative, forward-slash. `0_move.rs` validates existence and
/// language; this pass and `2_move_text.rs` only ever need the path pair.
pub(crate) struct MoveRow {
    pub old_rel: String,
    pub new_rel: String,
}

pub(crate) struct MoveRun {
    pub root: PathBuf,
    pub commit: bool,
    pub text_refs: bool,
    pub moves: Vec<MoveRow>,
}

/// Parse argv into the corpus root and the move batch, independent of
/// `0_move.rs`'s own parse of the same argv.
pub(crate) fn parse_run<I>(args: I) -> Result<MoveRun, String>
where
    I: IntoIterator,
    I::Item: Into<std::ffi::OsString> + Clone,
{
    let cli = MoveArgs::try_parse_from(args).map_err(|error| error.to_string())?;
    let requested = requested_moves(&cli)?;
    let root = plan_root(&cli, &requested)?;
    let mut moves = Vec::with_capacity(requested.len());
    for (old, new) in requested {
        moves.push(MoveRow {
            old_rel: rel_to_root(&root, &old)?,
            new_rel: rel_to_root(&root, &new)?,
        });
    }
    Ok(MoveRun {
        root,
        commit: cli.commit,
        text_refs: cli.text_refs,
        moves,
    })
}

fn requested_moves(cli: &MoveArgs) -> Result<Vec<(PathBuf, PathBuf)>, String> {
    match (&cli.list, &cli.old, &cli.new) {
        (Some(list), None, None) => read_move_list(list),
        (Some(_), _, _) => Err("--list carries the moves; drop <old> and <new>".to_string()),
        (None, Some(old), Some(new)) => Ok(vec![(old.clone(), new.clone())]),
        (None, _, _) => Err("extract move takes <old> <new>, or --list <tsv>".to_string()),
    }
}

fn read_move_list(path: &Path) -> Result<Vec<(PathBuf, PathBuf)>, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("read move list {}: {error}", path.display()))?;
    let mut rows = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((old, new)) = line.split_once('\t') else {
            continue;
        };
        let (old, new) = (old.trim(), new.trim());
        if old.is_empty() || new.is_empty() {
            continue;
        }
        rows.push((PathBuf::from(old), PathBuf::from(new)));
    }
    Ok(rows)
}

/// `--root` as given, else discovered from the first move's existing side (the
/// old path pre-commit, the new path post-commit).
fn plan_root(cli: &MoveArgs, requested: &[(PathBuf, PathBuf)]) -> Result<PathBuf, String> {
    let root = match cli.root.as_deref() {
        Some(root) => absolute(root)?,
        None => {
            let Some((old, new)) = requested.first() else {
                return Err("extract move takes <old> <new>, or --list <tsv>".to_string());
            };
            let probe = [old, new]
                .into_iter()
                .filter_map(|path| absolute(path).ok())
                .filter_map(|path| path.parent().map(Path::to_path_buf))
                .find(|path| path.is_dir())
                .ok_or_else(|| {
                    "extract move manifests: cannot discover root, pass --root".to_string()
                })?;
            soopy::discover(&probe)
                .map_err(|error| format!("discover root for {}: {error}", probe.display()))?
                .root
        }
    };
    std::fs::canonicalize(&root)
        .map_err(|error| format!("canonicalize root {}: {error}", root.display()))
}

/// Canonicalized when reachable; lexical-only for an `old` side `--commit`
/// already relocated, against a root that `plan_root` always canonicalizes.
fn rel_to_root(root: &Path, path: &Path) -> Result<String, String> {
    let absolute = absolute(path)?;
    let target = absolute.canonicalize().unwrap_or(absolute);
    target
        .strip_prefix(root)
        .map(|rel| rel.to_string_lossy().replace('\\', "/"))
        .map_err(|_| format!("{} is outside root {}", target.display(), root.display()))
}

fn absolute(path: &Path) -> Result<PathBuf, String> {
    if path.is_absolute() {
        return Ok(normalize(path));
    }
    let cwd = std::env::current_dir().map_err(|error| format!("current directory: {error}"))?;
    Ok(normalize(&cwd.join(path)))
}

fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

// ── the corpus walk and package grain, shared with `2_move_text.rs` ────────

/// The move walker's own skip set (`0_move.rs:662`).
const SKIP_DIRS: [&str; 4] = [".git", "target", "node_modules", ".boop-worktrees"];

pub(crate) fn walk_root(root: &Path) -> Vec<PathBuf> {
    let mut queue = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(dir) = queue.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let name = entry.file_name().to_string_lossy().to_string();
            if kind.is_dir() {
                if !SKIP_DIRS.contains(&name.as_str()) {
                    queue.push(path);
                }
            } else if kind.is_file() {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

pub(crate) fn rel_string(root: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(root)
        .ok()
        .map(|rel| rel.to_string_lossy().replace('\\', "/"))
}

pub(crate) fn package_manifests(root: &Path) -> Vec<String> {
    walk_root(root)
        .into_iter()
        .filter_map(|path| rel_string(root, &path))
        .filter(|rel| rel == "package.json" || rel.ends_with("/package.json"))
        .collect()
}

pub(crate) fn dirname(rel: &str) -> String {
    rel.rsplit_once('/')
        .map(|(dir, _)| dir.to_string())
        .unwrap_or_default()
}

/// The deepest `package.json` directory containing `old_rel`; `""` (root) wins
/// only when nothing more specific does.
pub(crate) fn owning_package<'a>(package_dirs: &'a [String], old_rel: &str) -> Option<&'a str> {
    package_dirs
        .iter()
        .filter(|dir| {
            dir.is_empty()
                || (old_rel.starts_with(dir.as_str())
                    && old_rel.as_bytes().get(dir.len()) == Some(&b'/'))
        })
        .max_by_key(|dir| dir.len())
        .map(String::as_str)
}

// ── tsconfig outDir/rootDir, package-relative ───────────────────────────────

pub(crate) struct BuildPaths {
    pub root_dir: String,
    pub out_dir: String,
}

const BUILD_CONFIGS: [&str; 2] = ["tsconfig.build.json", "tsconfig.json"];

/// Read `rootDir`/`outDir` off the package's tsconfig chain, defaulting to
/// `src`/`dist` per field when no config states it.
pub(crate) fn build_paths(package_dir_abs: &Path) -> BuildPaths {
    let mut root_dir = None;
    let mut out_dir = None;
    for name in BUILD_CONFIGS {
        if root_dir.is_some() && out_dir.is_some() {
            break;
        }
        collect_build_paths(package_dir_abs, name, &mut root_dir, &mut out_dir, 0);
    }
    BuildPaths {
        root_dir: root_dir.unwrap_or_else(|| "src".to_string()),
        out_dir: out_dir.unwrap_or_else(|| "dist".to_string()),
    }
}

fn collect_build_paths(
    dir: &Path,
    file_name: &str,
    root_dir: &mut Option<String>,
    out_dir: &mut Option<String>,
    depth: u8,
) {
    if depth > 5 {
        return;
    }
    let Ok(text) = std::fs::read_to_string(dir.join(file_name)) else {
        return;
    };
    let Ok(value) = serde_json::from_str::<Value>(&text) else {
        return;
    };
    if let Some(options) = value.get("compilerOptions") {
        if root_dir.is_none() {
            *root_dir = options
                .get("rootDir")
                .and_then(Value::as_str)
                .map(strip_rel);
        }
        if out_dir.is_none() {
            *out_dir = options.get("outDir").and_then(Value::as_str).map(strip_rel);
        }
    }
    if root_dir.is_none() || out_dir.is_none() {
        if let Some(extends) = value.get("extends").and_then(Value::as_str) {
            let target = dir.join(extends);
            if let (Some(parent), Some(name)) = (target.parent(), target.file_name()) {
                collect_build_paths(
                    parent,
                    &name.to_string_lossy(),
                    root_dir,
                    out_dir,
                    depth + 1,
                );
            }
        }
    }
}

fn strip_rel(raw: &str) -> String {
    let trimmed = raw.trim_start_matches("./");
    if trimmed.is_empty() || trimmed == "." {
        String::new()
    } else {
        trimmed.trim_end_matches('/').to_string()
    }
}

fn join_rel(base: &str, rel: &str) -> String {
    if base.is_empty() {
        rel.to_string()
    } else if rel.is_empty() {
        base.to_string()
    } else {
        format!("{base}/{rel}")
    }
}

fn strip_dir<'a>(dir: &str, rel: &'a str) -> Option<&'a str> {
    if dir.is_empty() {
        return Some(rel);
    }
    rel.strip_prefix(dir)?.strip_prefix('/')
}

fn source_to_within_root(package_dir: &str, root_dir: &str, rel: &str) -> Option<String> {
    let full_root = join_rel(package_dir, root_dir);
    strip_dir(&full_root, rel).map(str::to_string)
}

const SOURCE_EXTS: [&str; 4] = [".ts", ".tsx", ".mts", ".cts"];

/// Emitted extension -> the source extensions it can come from. `.d.ts` loses
/// to nothing here; a declaration and its implementation share one stem.
const EMITTED_EXTS: [(&str, &[&str]); 7] = [
    (".d.ts", &[".ts", ".tsx"]),
    (".js.map", &[".ts", ".tsx"]),
    (".mjs.map", &[".mts"]),
    (".cjs.map", &[".cts"]),
    (".js", &[".ts", ".tsx"]),
    (".mjs", &[".mts"]),
    (".cjs", &[".cts"]),
];

/// `<outDir>/<within-rootDir path><emitted ext>` pairs, one per emitted ext
/// whose source set covers the move's own extension; empty outside `rootDir`.
pub(crate) fn compiled_spellings(
    package_dir: &str,
    build: &BuildPaths,
    old_rel: &str,
    new_rel: &str,
) -> Vec<(String, String)> {
    let Some(old_ext) = SOURCE_EXTS
        .iter()
        .copied()
        .find(|ext| old_rel.ends_with(ext))
    else {
        return Vec::new();
    };
    let Some(new_ext) = SOURCE_EXTS
        .iter()
        .copied()
        .find(|ext| new_rel.ends_with(ext))
    else {
        return Vec::new();
    };
    let Some(old_within) = source_to_within_root(package_dir, &build.root_dir, old_rel) else {
        return Vec::new();
    };
    let Some(new_within) = source_to_within_root(package_dir, &build.root_dir, new_rel) else {
        return Vec::new();
    };
    let Some(old_stem) = old_within.strip_suffix(old_ext) else {
        return Vec::new();
    };
    let Some(new_stem) = new_within.strip_suffix(new_ext) else {
        return Vec::new();
    };
    EMITTED_EXTS
        .iter()
        .filter(|(_, sources)| sources.contains(&old_ext))
        .map(|(emitted, _)| {
            (
                format!("{}/{old_stem}{emitted}", build.out_dir),
                format!("{}/{new_stem}{emitted}", build.out_dir),
            )
        })
        .collect()
}

// ── the package.json rewrite itself ─────────────────────────────────────────

const CANDIDATE_FIELDS: [&str; 6] = ["main", "module", "types", "browser", "bin", "exports"];

pub fn run<I>(args: I) -> Result<(), String>
where
    I: IntoIterator,
    I::Item: Into<std::ffi::OsString> + Clone,
{
    let plan = parse_run(args)?;
    let manifests = package_manifests(&plan.root);
    let package_dirs: Vec<String> = manifests.iter().map(|rel| dirname(rel)).collect();
    let mut grouped: BTreeMap<String, Vec<&MoveRow>> = BTreeMap::new();
    for mv in &plan.moves {
        if let Some(dir) = owning_package(&package_dirs, &mv.old_rel) {
            grouped.entry(dir.to_string()).or_default().push(mv);
        }
    }
    for manifest_rel in &manifests {
        let dir = dirname(manifest_rel);
        let Some(moves) = grouped.get(&dir) else {
            continue;
        };
        rewrite_manifest(&plan.root, manifest_rel, &dir, moves, plan.commit)?;
    }
    Ok(())
}

fn rewrite_manifest(
    root: &Path,
    manifest_rel: &str,
    package_dir: &str,
    moves: &[&MoveRow],
    commit: bool,
) -> Result<(), String> {
    let full_path = root.join(manifest_rel);
    let original = std::fs::read_to_string(&full_path)
        .map_err(|error| format!("read {manifest_rel}: {error}"))?;
    // An unparseable manifest contributes no rewrite, never an error: the same
    // under-report-not-mis-report degradation `manifests.rs` takes.
    let Ok(mut value) = serde_json::from_str::<Value>(&original) else {
        return Ok(());
    };
    let build = build_paths(&full_path.parent().unwrap_or(root).to_path_buf());
    let mut rewrites: Vec<(Vec<String>, String, String)> = Vec::new();
    for field in CANDIDATE_FIELDS {
        let Some(field_value) = value.get(field) else {
            continue;
        };
        let mut leaves = Vec::new();
        collect_string_leaves(field_value, &mut vec![field.to_string()], &mut leaves);
        for (path, raw) in leaves {
            if let Some(proposed) = propose_field_rewrite(package_dir, &build, moves, &raw) {
                if proposed != raw {
                    rewrites.push((path, raw, proposed));
                }
            }
        }
    }
    if rewrites.is_empty() {
        return Ok(());
    }
    for (path, old, new) in &rewrites {
        println!(
            "manifest {manifest_rel}: {} {old} -> {new}",
            field_path_display(path)
        );
    }
    if commit {
        for (path, _, new) in &rewrites {
            if let Some(slot) = value.pointer_mut(&json_pointer(path)) {
                *slot = Value::String(new.clone());
            }
        }
        write_manifest(&full_path, &original, &value)?;
    }
    Ok(())
}

fn propose_field_rewrite(
    package_dir: &str,
    build: &BuildPaths,
    moves: &[&MoveRow],
    raw: &str,
) -> Option<String> {
    let (prefix, bare) = split_prefix(raw);
    for mv in moves {
        if strip_dir(package_dir, &mv.old_rel) == Some(bare) {
            let new_in_pkg = strip_dir(package_dir, &mv.new_rel)?;
            return Some(format!("{prefix}{new_in_pkg}"));
        }
    }
    for mv in moves {
        for (old_spelling, new_spelling) in
            compiled_spellings(package_dir, build, &mv.old_rel, &mv.new_rel)
        {
            if bare == old_spelling {
                return Some(format!("{prefix}{new_spelling}"));
            }
        }
    }
    None
}

fn split_prefix(raw: &str) -> (&str, &str) {
    match raw.strip_prefix("./") {
        Some(rest) => ("./", rest),
        None => ("", raw),
    }
}

fn collect_string_leaves(
    value: &Value,
    path: &mut Vec<String>,
    out: &mut Vec<(Vec<String>, String)>,
) {
    match value {
        Value::String(text) => out.push((path.clone(), text.clone())),
        Value::Object(map) => {
            for (key, inner) in map {
                path.push(key.clone());
                collect_string_leaves(inner, path, out);
                path.pop();
            }
        }
        Value::Array(items) => {
            for (index, inner) in items.iter().enumerate() {
                path.push(index.to_string());
                collect_string_leaves(inner, path, out);
                path.pop();
            }
        }
        _ => {}
    }
}

/// `field["./browser"].types` style: a plain identifier segment reads as
/// `.name`, an all-digit segment (array index) as `[n]`, anything else quoted.
fn field_path_display(path: &[String]) -> String {
    let mut out = String::new();
    for (index, segment) in path.iter().enumerate() {
        if index == 0 {
            out.push_str(segment);
        } else if segment.chars().all(|c| c.is_ascii_digit()) {
            out.push('[');
            out.push_str(segment);
            out.push(']');
        } else if segment
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            out.push('.');
            out.push_str(segment);
        } else {
            out.push_str("[\"");
            out.push_str(segment);
            out.push_str("\"]");
        }
    }
    out
}

fn json_pointer(path: &[String]) -> String {
    let mut out = String::new();
    for segment in path {
        out.push('/');
        out.push_str(&segment.replace('~', "~0").replace('/', "~1"));
    }
    out
}

/// Serialized with the file's own indent width, so an untouched manifest's
/// unedited keys reserialize to their own original bytes.
fn detect_indent(text: &str) -> String {
    for line in text.lines() {
        let spaces = line.chars().take_while(|c| *c == ' ').count();
        if spaces > 0 {
            return " ".repeat(spaces);
        }
        if line.starts_with('\t') {
            return "\t".to_string();
        }
    }
    "  ".to_string()
}

fn write_manifest(path: &Path, original: &str, value: &Value) -> Result<(), String> {
    let indent = detect_indent(original);
    let mut buf = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(indent.as_bytes());
    let mut serializer = serde_json::Serializer::with_formatter(&mut buf, formatter);
    value
        .serialize(&mut serializer)
        .map_err(|error| error.to_string())?;
    if original.ends_with('\n') {
        buf.push(b'\n');
    }
    std::fs::write(path, buf).map_err(|error| format!("write {}: {error}", path.display()))
}
