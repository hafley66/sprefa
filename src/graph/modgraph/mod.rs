//! Cross-language module dependency graph: "the filesystem from language, as
//! statically as possible". A `ModuleResolver` per language turns one file's
//! source into `ModuleRef`s — each an import/mod/use with its resolved target
//! (another project file, an external package, or unresolved). The engine writes
//! these as the built-in `module_import`/`module_edge`/`module_unresolved`
//! relations; `reaches(a,b) <- closure(module_edge).` then gives reach/cycles.
//!
//! Resolution math is Rust (path arithmetic), so `.dl` never needs a `dir()`/
//! `join()` expression layer. Monorepo-aware: Cargo workspaces (per-crate
//! `crate::` namespace + cross-crate `use`), `#[path]` overrides, npm/pnpm
//! workspaces (per-package tsconfig + workspace package.json fallback), Kotlin
//! packages (package declaration is truth, directory layout advisory). Extraction
//! runs over comment/string-stripped content so `use`/`import` in comments or
//! string literals never produce a phantom edge. Validated against `rust-analyzer
//! scip` (tests/oracle_rust.rs). See plans/2026-05-30-module-resolver-trait-plan.md.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;
use std::sync::OnceLock;

use regex::Regex;

mod rust;
mod ts;
mod kotlin;
mod go;
mod python;
pub(crate) use rust::*;
pub(crate) use ts::*;
pub(crate) use kotlin::*;
pub(crate) use go::*;
pub(crate) use python::*;

/// Where a specifier points. `File` is a project-relative path in the file set;
/// `External` is a package/std path we deliberately do not chase; `Unresolved`
/// is a specifier that should have resolved to a file but did not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resolution {
    File(String),
    External(String),
    Unresolved(String),
}

/// One reference out of a file. `kind` is "mod" | "use" | "import".
/// `span` is the byte range, in the file's raw content, of the contiguous
/// source text this ref rewrites: the leaf path of a `use` (or brace leaf), or
/// the specifier-literal text of a TS `import`. `None` when the ref has no
/// contiguous rewrite coordinate (`mod`/`#[path]` decls, dynamic imports).
/// `bindings` is the aliased-import local bindings this one specifier ref
/// carries: (local name, exported/source name) pairs — `use x::y as z` ->
/// `[("z", "y")]`, a TS default import -> `[(ident, "default")]`, a plain
/// named/bare import -> `[]` (local == source already resolves via the
/// name-keyed def bucket, no alias hop needed). Empty for every ref kind that
/// has no local-binding concept (`mod`, `#[path]`, Kotlin wildcard/same-package).
///
/// `module_bindings` is the SUPERSET the `module_binding` relation reads:
/// every local name this ref binds into scope, kind-tagged, for EVERY
/// resolution (`bindings` above only fires for a resolved, non-self `File`
/// target — the alias hop that resolution needs a file to hop to; a library
/// import (the common case a mapping query cares about) resolves `External`
/// and would never appear there). Each entry is `(local_name, imported_name,
/// kind)` with `kind` ∈ `named` | `default` | `namespace` | `side_effect` |
/// `reexport` (Rust `pub use`). A glob/wildcard import (Rust `use a::*`,
/// Kotlin `import a.*`, Go dot-import) binds no single local name and is
/// skipped, same as `bindings`. Populated by Rust/TS/Kotlin/Go/Python
/// resolvers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleRef {
    pub specifier: String,
    pub kind: &'static str,
    pub line: u32,
    pub span: Option<(u32, u32)>,
    pub target: Resolution,
    pub bindings: Vec<(String, String)>,
    pub module_bindings: Vec<(String, String, &'static str)>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CrateEdge {
    pub src: String,
    pub dst: String,
    pub kind: &'static str,
}

/// Per-(repo,rev) context shared across a language's `edges()` calls in one
/// refresh. `files` is the project-relative path set; `manifests` maps each
/// `Cargo.toml`/`package.json`/`go.mod` path to its contents (used to build the
/// crate / package / module registries lazily).
pub struct ProjectCx<'a> {
    pub root: &'a Path,
    pub files: &'a HashSet<String>,
    pub manifests: &'a HashMap<String, String>,
    /// Rev-correct content reader, for resolvers whose index needs file contents
    /// (Kotlin: the `package` declaration is truth, the directory is not). None
    /// (unit tests without one) leaves such indexes empty.
    reader: Option<&'a (dyn Fn(&str) -> Option<String> + Send + Sync)>,
    rust_crates: OnceLock<RustCrates>,
    ts_packages: OnceLock<HashMap<String, String>>,
    kotlin: OnceLock<KotlinIndex>,
    go: OnceLock<GoIndex>,
    python_roots: OnceLock<Vec<String>>,
}

impl<'a> ProjectCx<'a> {
    pub fn new(
        root: &'a Path,
        files: &'a HashSet<String>,
        manifests: &'a HashMap<String, String>,
    ) -> Self {
        ProjectCx {
            root,
            files,
            manifests,
            reader: None,
            rust_crates: OnceLock::new(),
            ts_packages: OnceLock::new(),
            kotlin: OnceLock::new(),
            go: OnceLock::new(),
            python_roots: OnceLock::new(),
        }
    }

    pub fn with_reader(
        mut self,
        reader: &'a (dyn Fn(&str) -> Option<String> + Send + Sync),
    ) -> Self {
        self.reader = Some(reader);
        self
    }

    fn kotlin_index(&self) -> &KotlinIndex {
        self.kotlin
            .get_or_init(|| KotlinIndex::build(self.files, self.reader))
    }

    fn python_roots(&self) -> &Vec<String> {
        self.python_roots.get_or_init(|| py_import_roots(self.files))
    }

    fn rust_crates(&self) -> &RustCrates {
        self.rust_crates
            .get_or_init(|| RustCrates::build(self.files, self.manifests))
    }

    fn go_index(&self) -> &GoIndex {
        self.go.get_or_init(|| GoIndex::build(self.files, self.manifests))
    }

    /// npm/pnpm workspace package name -> the package's directory (manifest parent).
    fn ts_packages(&self) -> &HashMap<String, String> {
        self.ts_packages.get_or_init(|| {
            let mut m = HashMap::new();
            // Duplicate names can occur when repos are projected into the
            // repo-less module graph. Preserve first-wins deterministically.
            let mut manifests: Vec<_> = self.manifests.iter().collect();
            manifests.sort_by(|(ap, _), (bp, _)| ap.cmp(bp));
            for (path, content) in manifests {
                if !path.ends_with("package.json") {
                    continue;
                }
                if let Some(name) = json_name(content) {
                    let dir = parent_dir(path);
                    m.entry(name).or_insert(dir);
                }
            }
            m
        })
    }
}

pub trait ModuleResolver: Send + Sync {
    fn exts(&self) -> &'static [&'static str];
    fn edges(&self, file: &str, content: &str, cx: &ProjectCx) -> Vec<ModuleRef>;
}

// LANG-JUNCTION(module-resolvers): per-language import resolver registration; buys module_edge/module_unresolved/module_binding_resolved plus the name resolver's alias hop and import-scoped ambiguity narrowing
/// Map a file's extension to its resolver. Built per refresh (root-scoped for TS).
pub fn resolvers(root: &Path) -> Vec<Box<dyn ModuleResolver + Send + Sync>> {
    let mut v: Vec<Box<dyn ModuleResolver + Send + Sync>> = vec![Box::new(RustResolver)];
    if let Some(ts) = TsResolver::new(root) {
        v.push(Box::new(ts));
    }
    v.push(Box::new(KotlinResolver));
    v.push(Box::new(GoResolver));
    v.push(Box::new(PyResolver));
    v
}

// ── comment / string stripping ───────────────────────────────────────────────

/// Replace comment bytes (always) and, when `rust`, string/char-literal bytes
/// with spaces — preserving byte offsets and newlines so regex line numbers stay
/// correct and `use`/`import` text inside a comment or string never matches.
/// Rust: `//`, nested `/* */`, `"..."`, `'c'` (lifetime-safe), raw `r#"..."#`.
/// TS (rust=false): keeps string content (specifiers ARE strings) but still
/// tracks `'`/`"`/`` ` `` so a `//` inside a string is not seen as a comment.
fn strip_noise(src: &str, rust: bool) -> String {
    let b = src.as_bytes();
    let n = b.len();
    let mut out = Vec::with_capacity(n);
    let mut i = 0;
    let blank_to = |out: &mut Vec<u8>, b: &[u8], from: usize, to: usize, blank: bool| {
        for k in from..to {
            out.push(if b[k] == b'\n' {
                b'\n'
            } else if blank {
                b' '
            } else {
                b[k]
            });
        }
    };
    while i < n {
        let c = b[i];
        // line comment
        if c == b'/' && i + 1 < n && b[i + 1] == b'/' {
            let s = i;
            while i < n && b[i] != b'\n' {
                i += 1;
            }
            blank_to(&mut out, b, s, i, true);
            continue;
        }
        // block comment (nested when rust)
        if c == b'/' && i + 1 < n && b[i + 1] == b'*' {
            let s = i;
            let mut depth = 1;
            i += 2;
            while i < n && depth > 0 {
                if b[i] == b'/' && i + 1 < n && b[i + 1] == b'*' {
                    if rust {
                        depth += 1;
                    }
                    i += 2;
                } else if b[i] == b'*' && i + 1 < n && b[i + 1] == b'/' {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            blank_to(&mut out, b, s, i, true);
            continue;
        }
        // rust raw string: r"..." / r#"..."# / br#"..."#
        if rust {
            let mut j = i;
            if b[j] == b'b' && j + 1 < n {
                j += 1;
            }
            if b[j] == b'r' {
                let mut k = j + 1;
                let mut hashes = 0;
                while k < n && b[k] == b'#' {
                    hashes += 1;
                    k += 1;
                }
                if k < n && b[k] == b'"' {
                    let s = i;
                    k += 1;
                    // closing = '"' followed by `hashes` '#'
                    loop {
                        if k >= n {
                            break;
                        }
                        if b[k] == b'"' {
                            let mut h = 0;
                            while k + 1 + h < n && b[k + 1 + h] == b'#' {
                                h += 1;
                            }
                            if h >= hashes {
                                k += 1 + hashes;
                                break;
                            }
                        }
                        k += 1;
                    }
                    blank_to(&mut out, b, s, k.min(n), true);
                    i = k.min(n);
                    continue;
                }
            }
        }
        // normal double-quoted string
        if c == b'"' {
            let s = i;
            i += 1;
            while i < n {
                if b[i] == b'\\' {
                    i += 2;
                    continue;
                }
                if b[i] == b'"' {
                    i += 1;
                    break;
                }
                i += 1;
            }
            blank_to(&mut out, b, s, i.min(n), rust);
            continue;
        }
        // backtick template (TS)
        if !rust && c == b'`' {
            let s = i;
            i += 1;
            while i < n {
                if b[i] == b'\\' {
                    i += 2;
                    continue;
                }
                if b[i] == b'`' {
                    i += 1;
                    break;
                }
                i += 1;
            }
            blank_to(&mut out, b, s, i.min(n), false);
            continue;
        }
        // single quote: rust char (lifetime-safe heuristic) vs TS string
        if c == b'\'' {
            if rust {
                // `'x'` or `'\n'` is a char; anything else (`'a`, `'static`) is a lifetime.
                let is_char = (i + 2 < n && b[i + 1] == b'\\')
                    || (i + 2 < n && b[i + 1] != b'\\' && b[i + 2] == b'\'');
                if is_char {
                    let s = i;
                    i += 1;
                    if i < n && b[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                    if i < n && b[i] == b'\'' {
                        i += 1;
                    }
                    blank_to(&mut out, b, s, i.min(n), true);
                    continue;
                }
                out.push(c);
                i += 1;
                continue; // lifetime
            } else {
                let s = i;
                i += 1;
                while i < n {
                    if b[i] == b'\\' {
                        i += 2;
                        continue;
                    }
                    if b[i] == b'\'' {
                        i += 1;
                        break;
                    }
                    i += 1;
                }
                blank_to(&mut out, b, s, i.min(n), false);
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| src.to_string())
}

fn line_of(content: &str, byte: usize) -> u32 {
    content[..byte.min(content.len())]
        .bytes()
        .filter(|&b| b == b'\n')
        .count() as u32
        + 1
}

fn parent_dir(path: &str) -> String {
    Path::new(path)
        .parent()
        .map(|d| d.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default()
}

/// `"name": "x"` from a package.json (first match; diet, no full JSON parse).
fn json_name(content: &str) -> Option<String> {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r#""name"\s*:\s*"([^"]+)""#).unwrap());
    re.captures(content).map(|c| c[1].to_string())
}

// ── Rust ────────────────────────────────────────────────────────────────────

/// Join a `#[path]` value onto a base dir and normalize `.`/`..`/leading `./`.
fn normalize_join(base: &str, rel: &str) -> String {
    let mut segs: Vec<&str> = if base.is_empty() {
        Vec::new()
    } else {
        base.split('/').collect()
    };
    for s in rel.split('/') {
        match s {
            "" | "." => {}
            ".." => {
                segs.pop();
            }
            other => segs.push(other),
        }
    }
    segs.join("/")
}

fn join(prefix: &str, head: &str) -> String {
    let head = head.trim_start_matches(':');
    if prefix.is_empty() {
        head.to_string()
    } else if head.is_empty() {
        prefix.to_string()
    } else {
        format!("{prefix}::{head}")
    }
}

fn matching_brace(s: &str, open: usize) -> Option<usize> {
    let b = s.as_bytes();
    let mut depth = 0;
    for i in open..b.len() {
        match b[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Split on top-level commas, returning each item with its start offset in `s`.
fn split_top_commas(s: &str) -> Vec<(String, usize)> {
    let mut out = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    let b = s.as_bytes();
    for i in 0..b.len() {
        match b[i] {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            b',' if depth == 0 => {
                out.push((s[start..i].to_string(), start));
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push((s[start..].to_string(), start));
    out
}

/// File path -> Rust module path. None if the path has no `src/` segment.
pub fn file_to_mod_path(file_path: &str) -> Option<String> {
    let path = Path::new(file_path);
    let components: Vec<&str> = path
        .components()
        .map(|c| c.as_os_str().to_str().unwrap_or(""))
        .collect();
    let src_idx = components.iter().rposition(|c| *c == "src")?;
    let after_src: Vec<&str> = components[src_idx + 1..].to_vec();
    if after_src.is_empty() {
        return None;
    }
    let last = *after_src.last().unwrap();
    let stem = Path::new(last)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(last);
    if after_src.len() == 1 && (stem == "lib" || stem == "main") {
        return Some("crate".to_string());
    }
    let mut segments = vec!["crate"];
    if stem == "mod" {
        for dir in &after_src[..after_src.len() - 1] {
            segments.push(dir);
        }
    } else {
        for dir in &after_src[..after_src.len() - 1] {
            segments.push(dir);
        }
        segments.push(stem);
    }
    Some(segments.join("::"))
}

/// Resolve a use path to absolute `crate::` form. `crate::` passes through;
/// `self::`/`super::` resolve against `from_mod`; external/other-crate paths None.
pub fn resolve_to_absolute(use_path: &str, from_mod: &str) -> Option<String> {
    if use_path == "crate" || use_path.starts_with("crate::") {
        return Some(use_path.to_string());
    }
    if let Some(rest) = use_path.strip_prefix("self::") {
        return Some(format!("{from_mod}::{rest}"));
    }
    if use_path == "self" {
        return Some(from_mod.to_string());
    }
    if use_path.starts_with("super::") {
        let mut current = from_mod.to_string();
        let mut path = use_path;
        while let Some(rest) = path.strip_prefix("super::") {
            path = rest;
            match current.rfind("::") {
                Some(pos) => current.truncate(pos),
                None => return None,
            }
        }
        return Some(format!("{current}::{path}"));
    }
    None
}

// ── TypeScript / JavaScript (oxc_resolver) ───────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn cx<'a>(
        root: &'a Path,
        files: &'a HashSet<String>,
        manifests: &'a HashMap<String, String>,
    ) -> ProjectCx<'a> {
        ProjectCx::new(root, files, manifests)
    }
    fn set(paths: &[&str]) -> HashSet<String> {
        paths.iter().map(|s| s.to_string()).collect()
    }
    fn no_manifests() -> HashMap<String, String> {
        HashMap::new()
    }

    #[test]
    fn mod_path_basics() {
        assert_eq!(file_to_mod_path("src/lib.rs").as_deref(), Some("crate"));
        assert_eq!(
            file_to_mod_path("src/foo/bar.rs").as_deref(),
            Some("crate::foo::bar")
        );
        assert_eq!(
            file_to_mod_path("src/foo/mod.rs").as_deref(),
            Some("crate::foo")
        );
        assert_eq!(file_to_mod_path("lib/foo.rs"), None);
    }

    #[test]
    fn resolve_absolute() {
        assert_eq!(
            resolve_to_absolute("self::b::C", "crate::a").as_deref(),
            Some("crate::a::b::C")
        );
        assert_eq!(
            resolve_to_absolute("super::b::C", "crate::a::consumer").as_deref(),
            Some("crate::a::b::C")
        );
        assert_eq!(resolve_to_absolute("std::io::Read", "crate::a"), None);
    }

    #[test]
    fn mod_and_use_edges() {
        let files = set(&["src/lib.rs", "src/parser.rs", "src/parser/expr.rs"]);
        let m = no_manifests();
        let c = cx(Path::new("/repo"), &files, &m);
        assert_eq!(
            RustResolver.edges("src/lib.rs", "mod parser;\n", &c)[0].target,
            Resolution::File("src/parser.rs".into())
        );
        assert_eq!(
            RustResolver.edges("src/parser.rs", "mod expr;\n", &c)[0].target,
            Resolution::File("src/parser/expr.rs".into())
        );
        let e = RustResolver.edges("src/lib.rs", "use crate::parser::expr::Foo;\n", &c);
        assert_eq!(
            e.iter().find(|r| r.kind == "use").unwrap().target,
            Resolution::File("src/parser/expr.rs".into())
        );
    }

    #[test]
    fn nested_brace_groups() {
        let body = "crate::a::{b::{c, d}, e as f}";
        let v = expand_use(body);
        let paths: Vec<&str> = v.iter().map(|(p, _, _)| p.as_str()).collect();
        assert!(paths.contains(&"crate::a::b::c"));
        assert!(paths.contains(&"crate::a::b::d"));
        assert!(paths.contains(&"crate::a::e"));
        // Every brace leaf points at the outermost head prefix `crate::a` — the
        // shared, contiguous rewrite coordinate that moves when module `a` moves.
        for (_path, lo, hi) in &v {
            assert_eq!(&body[*lo as usize..*hi as usize], "crate::a");
        }
    }

    #[test]
    fn bare_use_span_covers_whole_path() {
        // No brace -> the span is the full contiguous path (rewrite the lot).
        let body = "crate::parser::expr::Foo";
        let v = expand_use(body);
        assert_eq!(v.len(), 1);
        let (path, lo, hi) = &v[0];
        assert_eq!(path, "crate::parser::expr::Foo");
        assert_eq!(
            &body[*lo as usize..*hi as usize],
            "crate::parser::expr::Foo"
        );
    }

    #[test]
    fn raw_ident_mod_and_use() {
        let files = set(&["src/lib.rs", "src/r#match.rs"]);
        let m = no_manifests();
        let c = cx(Path::new("/repo"), &files, &m);
        // `mod r#match;` resolves; `use crate::r#match::X` strips r# for the path lookup
        let e = RustResolver.edges("src/lib.rs", "mod r#match;\n", &c);
        assert!(e.iter().any(|r| r.kind == "mod"));
        let v = expand_use("crate::r#match::X");
        assert_eq!(
            v.iter().map(|(p, _, _)| p.clone()).collect::<Vec<_>>(),
            vec!["crate::match::X".to_string()]
        );
    }

    #[test]
    fn comment_and_string_use_is_ignored() {
        let files = set(&["src/lib.rs", "src/real.rs"]);
        let m = no_manifests();
        let c = cx(Path::new("/repo"), &files, &m);
        let src = "// use crate::commented::X;\nlet s = \"use crate::instring::Y;\";\nmod real;\n";
        let e = RustResolver.edges("src/lib.rs", src, &c);
        assert!(
            e.iter()
                .all(|r| !r.specifier.contains("commented") && !r.specifier.contains("instring")),
            "comment/string uses must not produce edges: {e:?}"
        );
        assert!(e
            .iter()
            .any(|r| r.kind == "mod" && r.target == Resolution::File("src/real.rs".into())));
    }

    #[test]
    fn rust_use_alias_captured() {
        let files = set(&["src/lib.rs", "src/foo.rs"]);
        let m = no_manifests();
        let c = cx(Path::new("/repo"), &files, &m);

        // bare `use ... as alias;`
        let e = RustResolver.edges("src/lib.rs", "use crate::foo::make as helper;\n", &c);
        let r = e.iter().find(|r| r.kind == "use").unwrap();
        assert_eq!(r.bindings, vec![("helper".to_string(), "make".to_string())]);

        // brace-group alias alongside a non-aliased sibling leaf: only the
        // aliased leaf carries a binding.
        let e2 = RustResolver.edges("src/lib.rs", "use crate::foo::{make as helper, other};\n", &c);
        let aliased = e2.iter().find(|r| r.specifier == "crate::foo::make").unwrap();
        assert_eq!(aliased.bindings, vec![("helper".to_string(), "make".to_string())]);
        let plain = e2.iter().find(|r| r.specifier == "crate::foo::other").unwrap();
        assert!(plain.bindings.is_empty(), "{plain:?}");

        // a collapsed `self` leaf never carries a binding even when written
        // with ` as ` (no meaningful local binding to alias).
        let e3 = RustResolver.edges("src/lib.rs", "use crate::foo::{self as renamed};\n", &c);
        assert!(e3.iter().all(|r| r.bindings.is_empty()), "{e3:?}");
    }

    #[test]
    fn ts_import_clause_parse() {
        assert_eq!(
            parse_ts_import_clause("{ foo as bar }"),
            vec![("bar".to_string(), "foo".to_string())]
        );
        assert_eq!(
            parse_ts_import_clause("Default"),
            vec![("Default".to_string(), "default".to_string())]
        );
        assert_eq!(
            parse_ts_import_clause("Default, { a as b, c }"),
            vec![
                ("Default".to_string(), "default".to_string()),
                ("b".to_string(), "a".to_string()),
            ]
        );
        assert!(parse_ts_import_clause("{ c }").is_empty(), "plain named skips");
        assert!(parse_ts_import_clause("* as ns").is_empty(), "namespace skips");
        assert_eq!(
            parse_ts_import_clause("type { Foo as Bar }"),
            vec![("Bar".to_string(), "Foo".to_string())]
        );
    }

    #[test]
    fn ts_resolves_relative_json_import() {
        // oxc_resolver's extensions list already carries ".json" (modgraph.rs
        // ResolveOptions), so a relative `import x from "./data.json"` resolves
        // to a real on-disk file exactly like a `.ts` sibling would — PROVIDED
        // the target is in the engine's tracked file set (`cx.files`). That
        // second half is not a resolver concern (it lives in
        // `module_rows_for_rev`'s `_file`-backed fileset), but this test pins
        // down the resolver's own half: it must not treat an existing,
        // tracked `.json` target as an unresolvable/external specifier.
        let dir = std::env::temp_dir().join("sprf_modgraph_test_ts_json");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        // Canonicalize (matches main.rs's `--root` handling): on macOS
        // `std::env::temp_dir()` lives under a `/var` -> `/private/var`
        // symlink, and oxc_resolver's `full_path()` comes back canonicalized,
        // so an un-canonicalized root here would spuriously fail the
        // `strip_prefix` in `TsResolver::edges` regardless of the .json fix.
        let dir = dir.canonicalize().unwrap();
        std::fs::write(dir.join("src/app.ts"), "import data from './data.json';\n").unwrap();
        std::fs::write(dir.join("src/data.json"), r#"{"k":1}"#).unwrap();

        let resolver = TsResolver::new(&dir).expect("TsResolver::new");
        let files = set(&["src/app.ts", "src/data.json"]);
        let m = no_manifests();
        let c = cx(&dir, &files, &m);
        let content = std::fs::read_to_string(dir.join("src/app.ts")).unwrap();
        let e = resolver.edges("src/app.ts", &content, &c);
        assert_eq!(
            e.iter().find(|r| r.specifier == "./data.json").map(|r| &r.target),
            Some(&Resolution::File("src/data.json".into())),
            "a tracked .json import target must resolve to Resolution::File: {e:?}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn path_attribute_override() {
        let files = set(&["src/lib.rs", "src/weird/place.rs"]);
        let m = no_manifests();
        let c = cx(Path::new("/repo"), &files, &m);
        let e = RustResolver.edges("src/lib.rs", "#[path = \"weird/place.rs\"]\nmod foo;\n", &c);
        let mods: Vec<&ModuleRef> = e.iter().filter(|r| r.kind == "mod").collect();
        assert_eq!(
            mods.len(),
            1,
            "exactly one mod edge (no double-count): {e:?}"
        );
        assert_eq!(
            mods[0].target,
            Resolution::File("src/weird/place.rs".into())
        );
    }

    #[test]
    fn multi_crate_namespace_and_cross_crate() {
        let files = set(&[
            "crateA/src/lib.rs",
            "crateA/src/foo.rs",
            "crateB/src/lib.rs",
            "crateB/src/foo.rs",
        ]);
        let mut m = HashMap::new();
        m.insert(
            "crateA/Cargo.toml".to_string(),
            "[package]\nname = \"crate_a\"\n".to_string(),
        );
        m.insert(
            "crateB/Cargo.toml".to_string(),
            "[package]\nname = \"crate_b\"\n".to_string(),
        );
        let c = cx(Path::new("/repo"), &files, &m);
        // crateA's `use crate::foo` must resolve to crateA's foo, NOT crateB's.
        let e = RustResolver.edges("crateA/src/lib.rs", "use crate::foo::A;\n", &c);
        assert_eq!(
            e.iter().find(|r| r.kind == "use").unwrap().target,
            Resolution::File("crateA/src/foo.rs".into()),
            "crate:: must stay in-crate: {e:?}"
        );
        // cross-crate `use crate_b::foo::B` resolves into crateB.
        let e2 = RustResolver.edges("crateA/src/lib.rs", "use crate_b::foo::B;\n", &c);
        assert_eq!(
            e2.iter().find(|r| r.kind == "use").unwrap().target,
            Resolution::File("crateB/src/foo.rs".into()),
            "cross-crate use must resolve: {e2:?}"
        );
    }

    fn kt_reader(contents: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> + Send + Sync {
        let m: HashMap<String, String> = contents
            .iter()
            .map(|(p, c)| (p.to_string(), c.to_string()))
            .collect();
        move |p: &str| m.get(p).cloned()
    }

    #[test]
    fn kotlin_package_decl_beats_directory_layout() {
        // B.kt lives in a directory that does NOT match its package; the
        // declaration is truth, so the import still resolves.
        let contents = [
            (
                "weird/spot/B.kt",
                "package com.foo\n\nclass Bar(val n: Int)\nfun topLevel() = 1\n",
            ),
            (
                "app/Main.kt",
                "package com.app\n\nimport com.foo.Bar\nimport com.foo.topLevel\n",
            ),
        ];
        let files = set(&["weird/spot/B.kt", "app/Main.kt"]);
        let m = no_manifests();
        let r = kt_reader(&contents);
        let c = cx(Path::new("/repo"), &files, &m).with_reader(&r);
        let e = KotlinResolver.edges("app/Main.kt", contents[1].1, &c);
        assert_eq!(e.len(), 2, "{e:?}");
        for r in &e {
            assert_eq!(
                r.target,
                Resolution::File("weird/spot/B.kt".into()),
                "{r:?}"
            );
        }
    }

    #[test]
    fn kotlin_wildcard_external_and_unresolved() {
        let contents = [
            ("lib/A.kt", "package com.foo\nclass A\n"),
            ("lib/B.kt", "package com.foo\nclass B\n"),
            ("app/Main.kt", "package com.app\n\nimport com.foo.*\nimport kotlin.collections.List\nimport com.foo.Missing\n"),
        ];
        let files = set(&["lib/A.kt", "lib/B.kt", "app/Main.kt"]);
        let m = no_manifests();
        let r = kt_reader(&contents);
        let c = cx(Path::new("/repo"), &files, &m).with_reader(&r);
        let e = KotlinResolver.edges("app/Main.kt", contents[2].1, &c);
        // wildcard fans to both files of the package
        let wild: Vec<_> = e.iter().filter(|r| r.specifier == "com.foo.*").collect();
        assert_eq!(wild.len(), 2, "{e:?}");
        // unknown package -> External, known package + missing decl -> Unresolved
        assert!(
            e.iter().any(
                |r| matches!(&r.target, Resolution::External(s) if s == "kotlin.collections.List")
            ),
            "{e:?}"
        );
        assert!(
            e.iter()
                .any(|r| matches!(&r.target, Resolution::Unresolved(s) if s.contains("Missing"))),
            "{e:?}"
        );
    }

    #[test]
    fn kotlin_nested_class_and_noise_immunity() {
        let contents = [
            (
                "lib/A.kt",
                "package com.foo\nclass Outer {\n    class Inner\n}\n",
            ),
            (
                "app/Main.kt",
                concat!(
                    "package com.app\n",
                    "// import com.foo.Commented\n",
                    "val s = \"\"\"\nimport com.foo.InString\n\"\"\"\n",
                    "import com.foo.Outer.Inner\n"
                ),
            ),
        ];
        let files = set(&["lib/A.kt", "app/Main.kt"]);
        let m = no_manifests();
        let r = kt_reader(&contents);
        let c = cx(Path::new("/repo"), &files, &m).with_reader(&r);
        let e = KotlinResolver.edges("app/Main.kt", contents[1].1, &c);
        assert_eq!(
            e.len(),
            1,
            "comment/raw-string imports must not match: {e:?}"
        );
        // `Outer.Inner` resolves through the (package, Outer) decl
        assert_eq!(e[0].target, Resolution::File("lib/A.kt".into()), "{e:?}");
        // span is the dotted path text (the rewrite coordinate)
        let (lo, hi) = e[0].span.unwrap();
        assert_eq!(
            &contents[1].1[lo as usize..hi as usize],
            "com.foo.Outer.Inner"
        );
    }

    #[test]
    fn kotlin_expect_actual_fans_to_all_declaring_files() {
        // expect/actual twins: one decl key, two declaring files; an import
        // edges to BOTH (wildcard-style), not first-sorted-wins.
        let contents = [
            ("common/Clock.kt", "package com.lib\nexpect class Clock\n"),
            ("jvm/Clock.kt", "package com.lib\nactual class Clock\n"),
            ("app/Main.kt", "package com.app\n\nimport com.lib.Clock\n"),
        ];
        let files = set(&["common/Clock.kt", "jvm/Clock.kt", "app/Main.kt"]);
        let m = no_manifests();
        let r = kt_reader(&contents);
        let c = cx(Path::new("/repo"), &files, &m).with_reader(&r);
        let e = KotlinResolver.edges("app/Main.kt", contents[2].1, &c);
        let targets: Vec<_> = e.iter().map(|r| &r.target).collect();
        assert_eq!(e.len(), 2, "{e:?}");
        assert!(
            targets.contains(&&Resolution::File("common/Clock.kt".into())),
            "{e:?}"
        );
        assert!(
            targets.contains(&&Resolution::File("jvm/Clock.kt".into())),
            "{e:?}"
        );
    }

    #[test]
    fn kotlin_same_package_implicit_edges() {
        let contents = [
            (
                "lib/Util.kt",
                "package com.app\nclass Util\nfun helper() = 1\n",
            ),
            ("lib/Other.kt", "package com.other\nclass Stray\n"),
            (
                "app/Main.kt",
                "package com.app\n\nfun main() {\n    val u = Util()\n    val s = Strayed()\n}\n",
            ),
        ];
        let files = set(&["lib/Util.kt", "lib/Other.kt", "app/Main.kt"]);
        let m = no_manifests();
        let r = kt_reader(&contents);
        let c = cx(Path::new("/repo"), &files, &m).with_reader(&r);
        let e = KotlinResolver.edges("app/Main.kt", contents[2].1, &c);
        // Util used bare -> same-package edge; helper unused -> none; Stray is
        // another package (and `Strayed` is not a word-boundary hit anyway).
        assert_eq!(e.len(), 1, "{e:?}");
        assert_eq!(e[0].kind, "same-package");
        assert_eq!(e[0].specifier, "Util");
        assert_eq!(e[0].target, Resolution::File("lib/Util.kt".into()));
        assert_eq!(e[0].line, 4, "1-based line of the first use: {e:?}");
    }

    #[test]
    fn kotlin_same_package_skips_locally_declared_names() {
        // Main.kt declares Util itself (expect/actual style twin): a bare
        // `Util` use resolves locally, no same-package edge.
        let contents = [
            ("lib/Util.kt", "package com.app\nactual class Util\n"),
            (
                "app/Main.kt",
                "package com.app\nexpect class Util\nval u = Util()\n",
            ),
        ];
        let files = set(&["lib/Util.kt", "app/Main.kt"]);
        let m = no_manifests();
        let r = kt_reader(&contents);
        let c = cx(Path::new("/repo"), &files, &m).with_reader(&r);
        let e = KotlinResolver.edges("app/Main.kt", contents[1].1, &c);
        assert!(e.is_empty(), "{e:?}");
    }

    #[test]
    fn kotlin_import_alias_captured() {
        let contents = [
            ("lib/A.kt", "package com.foo\nclass Bar\n"),
            ("app/Main.kt", "package com.app\n\nimport com.foo.Bar as Baz\n"),
        ];
        let files = set(&["lib/A.kt", "app/Main.kt"]);
        let m = no_manifests();
        let r = kt_reader(&contents);
        let c = cx(Path::new("/repo"), &files, &m).with_reader(&r);
        let e = KotlinResolver.edges("app/Main.kt", contents[1].1, &c);
        assert_eq!(e.len(), 1, "{e:?}");
        assert_eq!(e[0].target, Resolution::File("lib/A.kt".into()));
        assert_eq!(e[0].bindings, vec![("Baz".to_string(), "Bar".to_string())]);

        // a wildcard import never carries an alias binding.
        let contents_wild = [
            ("lib/A.kt", "package com.foo\nclass Bar\n"),
            ("app/Main.kt", "package com.app\n\nimport com.foo.*\n"),
        ];
        let files_w = set(&["lib/A.kt", "app/Main.kt"]);
        let rw = kt_reader(&contents_wild);
        let cw = cx(Path::new("/repo"), &files_w, &m).with_reader(&rw);
        let ew = KotlinResolver.edges("app/Main.kt", contents_wild[1].1, &cw);
        assert!(ew.iter().all(|r| r.bindings.is_empty()), "{ew:?}");
    }

    #[test]
    fn cargo_dependency_rename() {
        let files = set(&[
            "crateA/src/lib.rs",
            "crateB/src/lib.rs",
            "crateB/src/foo.rs",
        ]);
        let mut m = HashMap::new();
        // crateA depends on crate_b under the alias `renamed`.
        m.insert("crateA/Cargo.toml".to_string(),
            "[package]\nname = \"crate_a\"\n\n[dependencies]\nrenamed = { package = \"crate_b\", path = \"../crateB\" }\n".to_string());
        m.insert(
            "crateB/Cargo.toml".to_string(),
            "[package]\nname = \"crate_b\"\n".to_string(),
        );
        let c = cx(Path::new("/repo"), &files, &m);
        // `use renamed::foo::B` must resolve through the rename to crate_b's foo.
        let e = RustResolver.edges("crateA/src/lib.rs", "use renamed::foo::B;\n", &c);
        assert_eq!(
            e.iter().find(|r| r.kind == "use").unwrap().target,
            Resolution::File("crateB/src/foo.rs".into()),
            "package= rename must resolve: {e:?}"
        );
    }

    fn go_manifest(module: &str) -> HashMap<String, String> {
        let mut m = HashMap::new();
        m.insert("go.mod".to_string(), format!("module {module}\n\ngo 1.22\n"));
        m
    }

    #[test]
    fn go_single_import_resolves_to_package_dir() {
        let files = set(&["main.go", "pkg/store/store.go", "pkg/store/repo.go"]);
        let m = go_manifest("example.com/app");
        let c = cx(Path::new("/repo"), &files, &m);
        let content = "package main\n\nimport \"example.com/app/pkg/store\"\n\nfunc main() {}\n";
        let e = GoResolver.edges("main.go", content, &c);
        // whole-package fan-out: BOTH files in the target dir resolve.
        let mut got: Vec<&str> = e.iter().filter_map(|r| match &r.target {
            Resolution::File(f) => Some(f.as_str()),
            _ => None,
        }).collect();
        got.sort();
        assert_eq!(got, vec!["pkg/store/repo.go", "pkg/store/store.go"], "{e:?}");
    }

    #[test]
    fn go_grouped_import_block_aliased_and_unresolved() {
        let files = set(&["main.go", "pkg/store/store.go"]);
        let m = go_manifest("example.com/app");
        let c = cx(Path::new("/repo"), &files, &m);
        let content = "\
package main

import (
\t\"fmt\"
\ts \"example.com/app/pkg/store\"
\t\"example.com/app/pkg/missing\"
)

func main() {}
";
        let e = GoResolver.edges("main.go", content, &c);
        let fmt_row = e.iter().find(|r| r.specifier == "fmt").expect("fmt row");
        assert_eq!(fmt_row.target, Resolution::External("fmt".into()));
        let store_row = e.iter().find(|r| r.specifier == "example.com/app/pkg/store").expect("store row");
        assert_eq!(store_row.target, Resolution::File("pkg/store/store.go".into()));
        assert_eq!(store_row.bindings, vec![("s".to_string(), "store".to_string())]);
        let missing_row = e.iter().find(|r| r.specifier == "example.com/app/pkg/missing").expect("missing row");
        assert!(matches!(missing_row.target, Resolution::Unresolved(_)), "{missing_row:?}");
    }

    #[test]
    fn go_blank_and_dot_imports_carry_no_binding() {
        let files = set(&["main.go", "pkg/store/store.go"]);
        let m = go_manifest("example.com/app");
        let c = cx(Path::new("/repo"), &files, &m);
        let content = "\
package main

import (
\t_ \"example.com/app/pkg/store\"
\t. \"example.com/app/pkg/store\"
)
";
        let e = GoResolver.edges("main.go", content, &c);
        assert!(e.iter().all(|r| r.bindings.is_empty()), "{e:?}");
        assert_eq!(e.len(), 2, "{e:?}");
    }

    #[test]
    fn go_no_module_leaves_every_import_external() {
        let files = set(&["main.go"]);
        let m = no_manifests();
        let c = cx(Path::new("/repo"), &files, &m);
        let e = GoResolver.edges("main.go", "package main\n\nimport \"example.com/app/pkg/store\"\n", &c);
        assert_eq!(e[0].target, Resolution::External("example.com/app/pkg/store".into()));
    }
    #[test]
    fn python_absolute_import_package_and_module() {
        let files = set(&["pkg/__init__.py", "pkg/sub.py", "app.py"]);
        let m = no_manifests();
        let c = cx(Path::new("/repo"), &files, &m);
        let e = PyResolver.edges("app.py", "import pkg\nimport pkg.sub\n", &c);
        assert!(e.iter().any(|r| r.specifier == "pkg" && r.target == Resolution::File("pkg/__init__.py".into())), "{e:?}");
        assert!(e.iter().any(|r| r.specifier == "pkg.sub" && r.target == Resolution::File("pkg/sub.py".into())), "{e:?}");
    }

    #[test]
    fn python_import_alias_captured() {
        let files = set(&["pkg/sub.py", "app.py"]);
        let m = no_manifests();
        let c = cx(Path::new("/repo"), &files, &m);
        let e = PyResolver.edges("app.py", "import pkg.sub as sub\n", &c);
        let r = e.iter().find(|r| r.specifier == "pkg.sub").unwrap();
        assert_eq!(r.bindings, vec![("sub".to_string(), "sub".to_string())]);
    }

    #[test]
    fn python_from_import_names_and_alias() {
        let files = set(&["pkg/sub.py", "app.py"]);
        let m = no_manifests();
        let c = cx(Path::new("/repo"), &files, &m);
        let e = PyResolver.edges("app.py", "from pkg.sub import make as build, other\n", &c);
        let r = e.iter().find(|r| r.specifier == "pkg.sub").expect("one ModuleRef for the statement");
        assert_eq!(r.target, Resolution::File("pkg/sub.py".into()));
        assert!(r.bindings.contains(&("build".to_string(), "make".to_string())), "{r:?}");
        // even a non-aliased name gets an (local=source) binding row, per spec.
        assert!(r.bindings.contains(&("other".to_string(), "other".to_string())), "{r:?}");
    }

    #[test]
    fn python_relative_import_resolves_off_importing_package() {
        let files = set(&["pkg/__init__.py", "pkg/a.py", "pkg/sub/__init__.py", "pkg/sub/b.py"]);
        let m = no_manifests();
        let c = cx(Path::new("/repo"), &files, &m);
        // one dot = the current package (pkg/sub's own dir).
        let e1 = PyResolver.edges("pkg/sub/b.py", "from . import missing\n", &c);
        // "missing" isn't a submodule of pkg/sub, so this resolves to the
        // package's own __init__.py per the "from module import name targets
        // the module file" simplification.
        assert!(e1.iter().any(|r| r.target == Resolution::File("pkg/sub/__init__.py".into())), "{e1:?}");
        // two dots pop up to pkg/, reaching sibling module `a`.
        let e2 = PyResolver.edges("pkg/sub/b.py", "from .. import a\n", &c);
        assert!(e2.iter().any(|r| r.target == Resolution::File("pkg/__init__.py".into())), "{e2:?}");
    }

    #[test]
    fn python_star_import_is_unresolved_not_silent() {
        let files = set(&["pkg/__init__.py", "app.py"]);
        let m = no_manifests();
        let c = cx(Path::new("/repo"), &files, &m);
        let e = PyResolver.edges("app.py", "from pkg import *\n", &c);
        assert!(matches!(&e[0].target, Resolution::Unresolved(_)), "{e:?}");
    }

    #[test]
    fn python_unknown_absolute_import_is_external() {
        let files = set(&["app.py"]);
        let m = no_manifests();
        let c = cx(Path::new("/repo"), &files, &m);
        let e = PyResolver.edges("app.py", "import os\nimport requests\n", &c);
        assert!(e.iter().all(|r| matches!(r.target, Resolution::External(_))), "{e:?}");
    }

    #[test]
    fn python_src_layout_root_discovery() {
        // src-layout: the package lives under src/, an absolute import from
        // a file OUTSIDE src/ must still resolve via the discovered `src` root.
        let files = set(&["src/pkg/__init__.py", "src/pkg/mod.py", "tests/test_mod.py"]);
        let m = no_manifests();
        let c = cx(Path::new("/repo"), &files, &m);
        let e = PyResolver.edges("tests/test_mod.py", "import pkg.mod\n", &c);
        assert!(
            e.iter().any(|r| r.target == Resolution::File("src/pkg/mod.py".into())),
            "{e:?}"
        );
    }

    #[test]
    fn python_ambiguous_across_two_roots_stays_unresolved() {
        // the same package name reachable under two distinct top-level
        // package roots must NOT guess — stays Unresolved, loud.
        let files = set(&[
            "alpha/pkg/__init__.py",
            "beta/pkg/__init__.py",
            "app.py",
        ]);
        let m = no_manifests();
        let c = cx(Path::new("/repo"), &files, &m);
        let e = PyResolver.edges("app.py", "import pkg\n", &c);
        let r = e.iter().find(|r| r.specifier == "pkg").unwrap();
        assert!(matches!(&r.target, Resolution::Unresolved(_)), "{r:?}");
    }

    #[test]
    fn python_comment_and_string_import_is_ignored() {
        let files = set(&["real.py", "app.py"]);
        let m = no_manifests();
        let c = cx(Path::new("/repo"), &files, &m);
        let src = "# import fake_from_comment\ns = \"import fake_from_string\"\nimport real\n";
        let e = PyResolver.edges("app.py", src, &c);
        assert!(e.iter().all(|r| !r.specifier.contains("fake")), "{e:?}");
        assert!(e.iter().any(|r| r.specifier == "real"), "{e:?}");
    }
}

