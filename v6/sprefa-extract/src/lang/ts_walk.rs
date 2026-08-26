//! The specifier-carrier corpus walk: every file under a root that can name
//! another file, with the front end that reads its specifiers.
//! @comment-ok: module header, the seam list every lang file opens with
//!
//! BUY NOTE. `ignore` 0.4.33 is already in this lock (an ast-grep transitive)
//! and its `WalkBuilder` is the gitignore-aware walker soopy itself uses
//! (`soopy/src/_4_worktree.rs:6`). It is not taken here because soopy's own
//! directory walk hashes every file it visits (`soopy/src/_3a_files.rs:74`),
//! which a path-only corpus scan must not pay, and this crate's manifest is
//! held to one new dependency for the TS move. `SKIP_DIRS` is the ignore rule
//! until that dep lands; the swap is this one function.

use std::path::{Path, PathBuf};

/// Which front end reads a corpus file's specifiers. It classifies the walk
/// only; the TSX split is oxc's `SourceType`, not a separate grammar.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CorpusLang {
    Prolog,
    Ts,
    Tsx,
}

/// The move's skip list (`0_move.rs:462`) plus `dist`: emitted output whose
/// specifiers mirror the source tree's (10 gitignored ones in hafley-rxjs).
pub const SKIP_DIRS: [&str; 5] = [
    ".git",
    "target",
    "node_modules",
    ".boop-worktrees",
    "dist",
];

/// The TS-family extensions, source and emitted. The emitted ones carry
/// specifiers of their own and an ESM corpus writes them, so they are carriers.
const TS_FAMILY_EXTS: [&str; 8] = ["ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs"];

/// The file's extension, lowercased, or `None` for a name without one. Read off
/// the file name so a dotted directory cannot supply it.
fn extension(path: &str) -> Option<String> {
    let name = Path::new(path).file_name()?.to_str()?;
    let (stem, ext) = name.rsplit_once('.')?;
    (!stem.is_empty()).then(|| ext.to_ascii_lowercase())
}

/// Whether the path names a TS-family file. `.kts` is Kotlin and answers false,
/// which `ends_with(".ts")` cannot say (`lang/mod.rs:54-56`).
pub fn is_ts_family(path: &str) -> bool {
    extension(path).is_some_and(|ext| TS_FAMILY_EXTS.contains(&ext.as_str()))
}

/// The front end for one path, or `None` when the file carries no specifiers
/// this walk reads.
pub fn corpus_lang(path: &str) -> Option<CorpusLang> {
    match extension(path)?.as_str() {
        "pl" | "plt" => Some(CorpusLang::Prolog),
        "tsx" | "jsx" => Some(CorpusLang::Tsx),
        "ts" | "mts" | "cts" | "js" | "mjs" | "cjs" => Some(CorpusLang::Ts),
        _ => None,
    }
}

/// Every specifier carrier under `root`, each with its front end. Path order is
/// the move's merge order, so the action order and the previews do not move.
pub fn specifier_corpus(root: &Path) -> Vec<(PathBuf, CorpusLang)> {
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
                if let Some(lang) = corpus_lang(&name) {
                    files.push((path, lang));
                }
            }
        }
    }
    files.sort();
    files
}

/// The TS-family half of the corpus, in path order.
pub fn ts_corpus(root: &Path) -> Vec<PathBuf> {
    specifier_corpus(root)
        .into_iter()
        .filter(|(_, lang)| matches!(lang, CorpusLang::Ts | CorpusLang::Tsx))
        .map(|(path, _)| path)
        .collect()
}
