//! TS module specifier -> file path, on the real filesystem, bought.
//! @comment-ok: module header, the seam list every lang file opens with
//!
//! `oxc_resolver` is the ESM/CJS + tsconfig algorithm: extensionless names,
//! `index`, the `.js` written for a `.ts`, package.json `exports`/`main`,
//! tsconfig `paths`/`baseUrl`/`extends`/`references`. It re-uses the oxc family
//! this crate already links.
//!
//! WHY NOT `deps::resolve_specifier`. That one answers a different question and
//! keeps answering it: it resolves against a SUPPLIED file universe with no
//! syscall per specifier (`deps.rs:43-47`), which is what a corpus-wide dep fold
//! needs and what its madge grading measures. A move needs on-disk truth for a
//! handful of specifiers: package `exports`, a tsconfig `extends` chain, and a
//! monorepo's sibling packages are all outside the supplied universe. The two
//! stay separate; neither replaces the other.

use std::path::{Path, PathBuf};

use oxc_resolver::{ResolveOptions, Resolver, TsconfigDiscovery};

/// The candidate extensions, TS sources before their JS twins and `.d.ts` after
/// `.ts` (a declaration file loses to an implementation): `deps.rs:58-60`.
const EXTENSIONS: [&str; 9] = [
    ".ts", ".tsx", ".d.ts", ".mts", ".cts", ".js", ".jsx", ".mjs", ".cjs",
];

/// `./x.js` names what the compiler emits from `x.ts` (`deps.rs:65-70`). A list
/// REPLACES the written extension, so each ends with its own: a real `.js`.
const EXTENSION_ALIAS: [(&str, &[&str]); 4] = [
    (".js", &[".ts", ".tsx", ".d.ts", ".js"]),
    (".mjs", &[".mts", ".mjs"]),
    (".cjs", &[".cts", ".cjs"]),
    (".jsx", &[".tsx", ".jsx"]),
];

/// One resolver for a whole run, holding the options and the library's own
/// filesystem cache. `Resolver` is `Send + Sync`, so a rayon pool shares one.
pub struct TsResolver {
    inner: Resolver,
    root: PathBuf,
}

impl TsResolver {
    /// `root` bounds what counts as a move target; it is canonicalized so the
    /// resolver's own canonical paths compare against it.
    pub fn new(root: &Path) -> Result<Self, String> {
        let root = root
            .canonicalize()
            .map_err(|error| format!("canonicalize root {}: {error}", root.display()))?;
        Ok(Self {
            inner: Resolver::new(options()),
            root,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The file `module` names when written by `from`. `None` when it resolves
    /// to nothing: a missing file, an uninstalled package, a builtin.
    pub fn resolve(&self, from: &Path, module: &str) -> Option<PathBuf> {
        // `resolve_file` takes the IMPORTING FILE, not its directory: the
        // TsconfigDiscovery::Auto branch only runs there (oxc_resolver lib.rs:250).
        let resolution = self.inner.resolve_file(from, module).ok()?;
        Some(resolution.path().to_path_buf())
    }

    /// The same answer, kept only when it lands inside the root: a package in
    /// `node_modules` is no move target, a tsconfig alias into the tree is.
    pub fn resolve_in_root(&self, from: &Path, module: &str) -> Option<PathBuf> {
        let path = self.resolve(from, module)?;
        path.starts_with(&self.root).then_some(path)
    }
}

/// The ESM-style TS options. Every value is a stated policy; the defaults this
/// leaves alone are `symlinks` (true) and `exports_fields` (`[["exports"]]`).
fn options() -> ResolveOptions {
    ResolveOptions {
        extensions: EXTENSIONS.iter().map(|ext| (*ext).to_string()).collect(),
        extension_alias: EXTENSION_ALIAS
            .iter()
            .map(|(written, sources)| {
                (
                    (*written).to_string(),
                    sources.iter().map(|ext| (*ext).to_string()).collect(),
                )
            })
            .collect(),
        main_files: vec!["index".to_string()],
        main_fields: vec!["module".to_string(), "main".to_string()],
        condition_names: vec!["node".to_string(), "import".to_string()],
        tsconfig: Some(TsconfigDiscovery::Auto),
        ..ResolveOptions::default()
    }
}

/// The replacement for a relative specifier now aiming at `relative`, keeping
/// `original`'s extension style and quote. `./` leads, else TS reads a package.
pub fn respell(relative: &str, original: &str, quote: char) -> String {
    let text = match written_extension(original) {
        // The spec named the emitted twin of a source file; keep that spelling.
        Some(written) if backs(written, extension_of(relative)) => {
            format!("{}{written}", strip_extension(relative))
        }
        Some(_) => relative.to_string(),
        None => directory_form(strip_extension(relative), original),
    };
    let text = if text.starts_with("..") {
        text
    } else {
        format!("./{}", text.trim_start_matches("./"))
    };
    format!("{quote}{text}{quote}")
}

/// An extensionless spec that resolved through `index` keeps naming the
/// directory, unless the spec itself spelled `index`.
fn directory_form(stripped: &str, original: &str) -> String {
    let named_index = original
        .rsplit('/')
        .next()
        .is_some_and(|last| last == "index");
    match stripped.strip_suffix("/index") {
        Some(directory) if !named_index => directory.to_string(),
        _ => stripped.to_string(),
    }
}

/// The extension `original` wrote, when it is one this resolver knows. A spec
/// ending in an unknown suffix (`./v1.2`) wrote no extension.
fn written_extension(original: &str) -> Option<&'static str> {
    EXTENSIONS
        .iter()
        .chain(EXTENSION_ALIAS.iter().map(|(written, _)| written))
        .find(|ext| original.ends_with(**ext))
        .copied()
}

/// Whether a file with extension `source` is what `written` names on disk.
fn backs(written: &str, source: Option<&str>) -> bool {
    let Some(source) = source else {
        return false;
    };
    EXTENSION_ALIAS
        .iter()
        .find(|(candidate, _)| *candidate == written)
        .is_some_and(|(_, sources)| sources.contains(&source))
}

fn extension_of(path: &str) -> Option<&str> {
    EXTENSIONS.iter().find(|ext| path.ends_with(**ext)).copied()
}

fn strip_extension(path: &str) -> &str {
    match extension_of(path) {
        Some(ext) => &path[..path.len() - ext.len()],
        None => path,
    }
}
