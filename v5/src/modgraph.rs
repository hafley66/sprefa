//! Cross-language module dependency graph: "the filesystem from language, as
//! statically as possible". A `ModuleResolver` per language turns one file's
//! source into `ModuleRef`s — each an import/mod/use with its resolved target
//! (another project file, an external package, or unresolved). The engine writes
//! these as the built-in `module_import`/`module_edge`/`module_unresolved`
//! relations; `reaches(a,b) <- closure(module_edge).` then gives reach/cycles.
//!
//! Resolution math is Rust (path arithmetic, `Path::parent` for free), so `.dl`
//! never needs a `dir()`/`join()` expression layer. Diet: ~90% correct, no
//! compiler. See plans/2026-05-30-module-resolver-trait-plan.md.

use std::cell::OnceCell;
use std::collections::{HashMap, HashSet};
use std::path::Path;

use regex::Regex;

/// Where a specifier points. `File` is a project-relative path in the file set;
/// `External` is a package/std path we deliberately do not chase; `Unresolved`
/// is a specifier that should have resolved to a file but did not (deleted /
/// typo / unsupported convention) — the reason string is for the diag surface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resolution {
    File(String),
    External(String),
    Unresolved(String),
}

/// One reference out of a file. `kind` is "mod" | "use" | "import".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleRef {
    pub specifier: String,
    pub kind: &'static str,
    pub line: u32,
    pub target: Resolution,
}

/// Per-(repo,rev) context shared across a language's `edges()` calls in one
/// refresh. `files` is the project-relative path set at this rev; `rust_index`
/// is the lazily-built reverse map (module path -> file) Rust use-resolution needs.
pub struct ProjectCx<'a> {
    pub root: &'a Path,
    pub files: &'a HashSet<String>,
    rust_index: OnceCell<HashMap<String, String>>,
}

impl<'a> ProjectCx<'a> {
    pub fn new(root: &'a Path, files: &'a HashSet<String>) -> Self {
        ProjectCx { root, files, rust_index: OnceCell::new() }
    }

    /// module path ("crate::a::b") -> the file that defines it, over every Rust
    /// file in the set. Built once per context.
    pub fn rust_index(&self) -> &HashMap<String, String> {
        self.rust_index.get_or_init(|| build_rust_index(self.files))
    }
}

pub trait ModuleResolver {
    /// File extensions (no dot) this resolver claims.
    fn exts(&self) -> &'static [&'static str];
    /// References out of `file` (project-relative), given its `content` and the
    /// project context.
    fn edges(&self, file: &str, content: &str, cx: &ProjectCx) -> Vec<ModuleRef>;
}

/// Map a file's extension to its resolver. Built per refresh (root-scoped for TS).
pub fn resolvers(root: &Path) -> Vec<Box<dyn ModuleResolver>> {
    let mut v: Vec<Box<dyn ModuleResolver>> = vec![Box::new(RustResolver)];
    if let Some(ts) = TsResolver::new(root) { v.push(Box::new(ts)); }
    v
}

// ── Rust ────────────────────────────────────────────────────────────────────

pub struct RustResolver;

fn rust_mod_re() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?m)^[ \t]*(?:pub[ \t]*(?:\([^)]*\)[ \t]*)?)?mod[ \t]+([A-Za-z_]\w*)[ \t]*;").unwrap())
}

fn rust_use_re() -> &'static Regex {
    // DOTALL body capture so rustfmt-wrapped `use a::{\n b,\n c\n};` is one match.
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?s)\buse[ \t\r\n]+([^;]+);").unwrap())
}

fn line_of(content: &str, byte: usize) -> u32 {
    content[..byte.min(content.len())].bytes().filter(|&b| b == b'\n').count() as u32 + 1
}

impl ModuleResolver for RustResolver {
    fn exts(&self) -> &'static [&'static str] { &["rs"] }

    fn edges(&self, file: &str, content: &str, cx: &ProjectCx) -> Vec<ModuleRef> {
        let mut out = Vec::new();

        // `mod foo;` — submodule file by filesystem convention from this file's path.
        for c in rust_mod_re().captures_iter(content) {
            let name = &c[1];
            let line = line_of(content, c.get(0).unwrap().start());
            let target = match mod_child_candidates(file, name).into_iter()
                .find(|cand| cx.files.contains(cand)) {
                Some(f) => Resolution::File(f),
                None => Resolution::Unresolved(format!("mod {name}: no child file")),
            };
            out.push(ModuleRef { specifier: format!("mod {name}"), kind: "mod", line, target });
        }

        // `use path;` — intra-crate references; resolve crate/self/super to a file.
        let from_mod = file_to_mod_path(file);
        for c in rust_use_re().captures_iter(content) {
            let line = line_of(content, c.get(0).unwrap().start());
            for cand in use_candidates(&c[1]) {
                let target = match from_mod.as_deref() {
                    Some(fm) => resolve_use(&cand, fm, cx.rust_index()),
                    None => Resolution::External(cand.clone()),
                };
                out.push(ModuleRef { specifier: cand, kind: "use", line, target });
            }
        }
        out
    }
}

/// `crate::a::b::C` / `crate::a::{b, c as d}` / `crate::a as x` -> the module-path
/// candidates to resolve. Leaf symbols are kept (resolution takes the longest
/// prefix that is a file, so a leaf vs its module collapse to the same file).
fn use_candidates(body: &str) -> Vec<String> {
    let body = body.trim();
    let strip_alias = |s: &str| s.split(" as ").next().unwrap_or(s).trim().to_string();
    match body.find('{') {
        None => vec![strip_alias(body)],
        Some(i) => {
            let prefix = body[..i].trim_end_matches(':').trim().trim_end_matches(':');
            let inner = &body[i + 1..body.rfind('}').unwrap_or(body.len())];
            let mut out = Vec::new();
            for item in inner.split(',') {
                let item = strip_alias(item);
                if item.is_empty() || item.contains('{') { continue; }
                if item == "self" {
                    out.push(prefix.to_string());
                } else {
                    out.push(format!("{prefix}::{item}"));
                }
            }
            if out.is_empty() { out.push(prefix.to_string()); }
            out
        }
    }
}

/// Resolve an absolute-or-relative use path to a project file via the longest
/// module-path prefix that is a known file. External crates -> External; an
/// intra-crate path with no file -> Unresolved.
fn resolve_use(use_path: &str, from_mod: &str, index: &HashMap<String, String>) -> Resolution {
    let Some(abs) = resolve_to_absolute(use_path, from_mod) else {
        return Resolution::External(use_path.to_string());
    };
    let segs: Vec<&str> = abs.split("::").collect();
    for len in (1..=segs.len()).rev() {
        let prefix = segs[..len].join("::");
        if let Some(f) = index.get(&prefix) {
            return Resolution::File(f.clone());
        }
    }
    Resolution::Unresolved(format!("{abs}: no file for any module-path prefix"))
}

/// Candidate child-module files for `mod <name>;` in `file`, by Rust's filesystem
/// convention. A directory-defining file (mod.rs / lib.rs / main.rs) holds its
/// children as siblings; a `name.rs` file holds them under `name/`.
fn mod_child_candidates(file: &str, name: &str) -> Vec<String> {
    let p = Path::new(file);
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let dir = p.parent().map(|d| d.to_string_lossy().replace('\\', "/")).unwrap_or_default();
    let base = if matches!(stem, "mod" | "lib" | "main") {
        dir
    } else if dir.is_empty() {
        stem.to_string()
    } else {
        format!("{dir}/{stem}")
    };
    let j = |suffix: &str| if base.is_empty() { suffix.to_string() } else { format!("{base}/{suffix}") };
    vec![j(&format!("{name}.rs")), j(&format!("{name}/mod.rs"))]
}

/// Reverse index: module path -> file, over the Rust files in the set. A file
/// whose path has no `src/` segment has no module path and is skipped.
fn build_rust_index(files: &HashSet<String>) -> HashMap<String, String> {
    let mut m = HashMap::new();
    for f in files {
        if !f.ends_with(".rs") { continue; }
        if let Some(mp) = file_to_mod_path(f) {
            m.entry(mp).or_insert_with(|| f.clone());
        }
    }
    m
}

/// Convert a file path to a Rust module path (ported from v1/v2 watch::rs_path).
///   src/lib.rs | src/main.rs -> "crate"
///   src/foo.rs               -> "crate::foo"
///   src/foo/mod.rs           -> "crate::foo"
///   src/foo/bar.rs           -> "crate::foo::bar"
/// None if the path has no `src/` segment.
pub fn file_to_mod_path(file_path: &str) -> Option<String> {
    let path = Path::new(file_path);
    let components: Vec<&str> = path.components()
        .map(|c| c.as_os_str().to_str().unwrap_or("")).collect();
    let src_idx = components.iter().rposition(|c| *c == "src")?;
    let after_src: Vec<&str> = components[src_idx + 1..].to_vec();
    if after_src.is_empty() { return None; }
    let last = *after_src.last().unwrap();
    let stem = Path::new(last).file_stem().and_then(|s| s.to_str()).unwrap_or(last);
    if after_src.len() == 1 && (stem == "lib" || stem == "main") {
        return Some("crate".to_string());
    }
    let mut segments = vec!["crate"];
    if stem == "mod" {
        for dir in &after_src[..after_src.len() - 1] { segments.push(dir); }
    } else {
        for dir in &after_src[..after_src.len() - 1] { segments.push(dir); }
        segments.push(stem);
    }
    Some(segments.join("::"))
}

/// Resolve a use path to absolute `crate::` form (ported from v1/v2). `crate::`
/// passes through; `self::`/`super::` resolve against `from_mod`; external crate
/// paths (std::, serde::, ...) return None.
pub fn resolve_to_absolute(use_path: &str, from_mod: &str) -> Option<String> {
    if use_path == "crate" || use_path.starts_with("crate::") {
        return Some(use_path.to_string());
    }
    if let Some(rest) = use_path.strip_prefix("self::") {
        return Some(format!("{from_mod}::{rest}"));
    }
    if use_path == "self" { return Some(from_mod.to_string()); }
    if use_path.starts_with("super::") {
        let mut current = from_mod.to_string();
        let mut path = use_path;
        while let Some(rest) = path.strip_prefix("super::") {
            path = rest;
            match current.rfind("::") {
                Some(pos) => current.truncate(pos),
                None => return None, // super:: past crate root
            }
        }
        return Some(format!("{current}::{path}"));
    }
    None
}

// ── TypeScript / JavaScript (oxc_resolver) ───────────────────────────────────

pub struct TsResolver {
    resolver: oxc_resolver::Resolver,
    root: std::path::PathBuf,
}

fn ts_spec_re() -> &'static Regex {
    // import/export ... from '...'  |  bare import '...'  |  import('...')  |  require('...')
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(
        r#"(?m)(?:\bfrom|\bimport|\brequire)[ \t]*\(?[ \t]*['"]([^'"]+)['"]"#).unwrap())
}

impl TsResolver {
    fn new(root: &Path) -> Option<Self> {
        use oxc_resolver::{ResolveOptions, Resolver, TsconfigDiscovery, TsconfigOptions, TsconfigReferences};
        let tsconfig = {
            let p = root.join("tsconfig.json");
            if p.exists() {
                Some(TsconfigDiscovery::Manual(TsconfigOptions { config_file: p, references: TsconfigReferences::Auto }))
            } else { None }
        };
        let options = ResolveOptions {
            extensions: [".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".mts", ".cts", ".json"]
                .iter().map(|s| s.to_string()).collect(),
            main_fields: vec!["module".into(), "main".into()],
            condition_names: vec!["import".into(), "require".into(), "default".into()],
            tsconfig,
            ..ResolveOptions::default()
        };
        Some(TsResolver { resolver: Resolver::new(options), root: root.to_path_buf() })
    }
}

impl ModuleResolver for TsResolver {
    fn exts(&self) -> &'static [&'static str] {
        &["ts", "tsx", "js", "jsx", "mjs", "cjs", "mts", "cts"]
    }

    fn edges(&self, file: &str, content: &str, cx: &ProjectCx) -> Vec<ModuleRef> {
        let abs = self.root.join(file);
        let dir = abs.parent().unwrap_or(&self.root);
        let mut out = Vec::new();
        for c in ts_spec_re().captures_iter(content) {
            let spec = c[1].to_string();
            let line = line_of(content, c.get(0).unwrap().start());
            let target = match self.resolver.resolve(dir, &spec) {
                Ok(r) => {
                    let full = r.full_path();
                    match full.strip_prefix(&self.root).ok()
                        .map(|p| p.to_string_lossy().replace('\\', "/"))
                        .filter(|rel| cx.files.contains(rel)) {
                        Some(rel) => Resolution::File(rel),
                        None => Resolution::External(spec.clone()),
                    }
                }
                Err(_) => Resolution::Unresolved(format!("{spec}: unresolved")),
            };
            out.push(ModuleRef { specifier: spec, kind: "import", line, target });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(paths: &[&str]) -> HashSet<String> {
        paths.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn mod_path_basics() {
        assert_eq!(file_to_mod_path("src/lib.rs").as_deref(), Some("crate"));
        assert_eq!(file_to_mod_path("src/main.rs").as_deref(), Some("crate"));
        assert_eq!(file_to_mod_path("src/foo.rs").as_deref(), Some("crate::foo"));
        assert_eq!(file_to_mod_path("src/foo/mod.rs").as_deref(), Some("crate::foo"));
        assert_eq!(file_to_mod_path("src/foo/bar.rs").as_deref(), Some("crate::foo::bar"));
        assert_eq!(file_to_mod_path("v5/src/a/b.rs").as_deref(), Some("crate::a::b"));
        assert_eq!(file_to_mod_path("lib/foo.rs"), None);
    }

    #[test]
    fn resolve_absolute() {
        assert_eq!(resolve_to_absolute("crate::a::B", "crate::x").as_deref(), Some("crate::a::B"));
        assert_eq!(resolve_to_absolute("self::b::C", "crate::a").as_deref(), Some("crate::a::b::C"));
        assert_eq!(resolve_to_absolute("super::b::C", "crate::a::consumer").as_deref(), Some("crate::a::b::C"));
        assert_eq!(resolve_to_absolute("super::super::t::X", "crate::a::b::c").as_deref(), Some("crate::a::t::X"));
        assert_eq!(resolve_to_absolute("std::io::Read", "crate::a"), None);
        assert_eq!(resolve_to_absolute("super::super::x", "crate::a"), None);
    }

    #[test]
    fn mod_decl_edge() {
        let files = set(&["src/lib.rs", "src/parser.rs", "src/parser/expr.rs"]);
        let cx = ProjectCx::new(Path::new("/repo"), &files);
        // lib.rs declares `mod parser;` -> src/parser.rs (sibling of lib.rs)
        let e = RustResolver.edges("src/lib.rs", "mod parser;\n", &cx);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].target, Resolution::File("src/parser.rs".into()));
        // parser.rs declares `mod expr;` -> src/parser/expr.rs (under parser/)
        let e2 = RustResolver.edges("src/parser.rs", "mod expr;\n", &cx);
        assert_eq!(e2[0].target, Resolution::File("src/parser/expr.rs".into()));
    }

    #[test]
    fn use_edge_resolves_to_file() {
        let files = set(&["src/lib.rs", "src/parser.rs", "src/parser/expr.rs"]);
        let cx = ProjectCx::new(Path::new("/repo"), &files);
        // use crate::parser::expr::Foo -> longest file prefix = crate::parser::expr
        let e = RustResolver.edges("src/lib.rs", "use crate::parser::expr::Foo;\n", &cx);
        let uses: Vec<&ModuleRef> = e.iter().filter(|r| r.kind == "use").collect();
        assert_eq!(uses[0].target, Resolution::File("src/parser/expr.rs".into()));
        // external crate -> External
        let e2 = RustResolver.edges("src/lib.rs", "use std::collections::HashMap;\n", &cx);
        let u2: Vec<&ModuleRef> = e2.iter().filter(|r| r.kind == "use").collect();
        assert!(matches!(u2[0].target, Resolution::External(_)));
    }

    #[test]
    fn use_brace_group_expands() {
        let files = set(&["src/lib.rs", "src/a.rs", "src/a/b.rs", "src/a/c.rs"]);
        let cx = ProjectCx::new(Path::new("/repo"), &files);
        let e = RustResolver.edges("src/lib.rs", "use crate::a::{b, c};\n", &cx);
        let targets: Vec<&Resolution> = e.iter().filter(|r| r.kind == "use").map(|r| &r.target).collect();
        assert!(targets.contains(&&Resolution::File("src/a/b.rs".into())));
        assert!(targets.contains(&&Resolution::File("src/a/c.rs".into())));
    }

    #[test]
    fn inline_mod_is_not_a_file_edge() {
        let files = set(&["src/lib.rs"]);
        let cx = ProjectCx::new(Path::new("/repo"), &files);
        // `mod foo { .. }` (no semicolon) must not produce a mod edge
        let e = RustResolver.edges("src/lib.rs", "mod foo {\n  fn x() {}\n}\n", &cx);
        assert!(e.iter().all(|r| r.kind != "mod"));
    }
}
