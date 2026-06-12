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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleRef {
    pub specifier: String,
    pub kind: &'static str,
    pub line: u32,
    pub span: Option<(u32, u32)>,
    pub target: Resolution,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CrateEdge {
    pub src: String,
    pub dst: String,
    pub kind: &'static str,
}

/// Per-(repo,rev) context shared across a language's `edges()` calls in one
/// refresh. `files` is the project-relative path set; `manifests` maps each
/// `Cargo.toml`/`package.json` path to its contents (used to build the crate /
/// package registries lazily).
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
}

impl<'a> ProjectCx<'a> {
    pub fn new(root: &'a Path, files: &'a HashSet<String>, manifests: &'a HashMap<String, String>) -> Self {
        ProjectCx { root, files, manifests, reader: None,
            rust_crates: OnceLock::new(), ts_packages: OnceLock::new(), kotlin: OnceLock::new() }
    }

    pub fn with_reader(mut self, reader: &'a (dyn Fn(&str) -> Option<String> + Send + Sync)) -> Self {
        self.reader = Some(reader); self
    }

    fn kotlin_index(&self) -> &KotlinIndex {
        self.kotlin.get_or_init(|| KotlinIndex::build(self.files, self.reader))
    }

    fn rust_crates(&self) -> &RustCrates {
        self.rust_crates.get_or_init(|| RustCrates::build(self.files, self.manifests))
    }

    /// npm/pnpm workspace package name -> the package's directory (manifest parent).
    fn ts_packages(&self) -> &HashMap<String, String> {
        self.ts_packages.get_or_init(|| {
            let mut m = HashMap::new();
            for (path, content) in self.manifests {
                if !path.ends_with("package.json") { continue; }
                if let Some(name) = json_name(content) {
                    let dir = parent_dir(path);
                    m.insert(name, dir);
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

/// Map a file's extension to its resolver. Built per refresh (root-scoped for TS).
pub fn resolvers(root: &Path) -> Vec<Box<dyn ModuleResolver + Send + Sync>> {
    let mut v: Vec<Box<dyn ModuleResolver + Send + Sync>> = vec![Box::new(RustResolver)];
    if let Some(ts) = TsResolver::new(root) { v.push(Box::new(ts)); }
    v.push(Box::new(KotlinResolver));
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
        for k in from..to { out.push(if b[k] == b'\n' { b'\n' } else if blank { b' ' } else { b[k] }); }
    };
    while i < n {
        let c = b[i];
        // line comment
        if c == b'/' && i + 1 < n && b[i + 1] == b'/' {
            let s = i; while i < n && b[i] != b'\n' { i += 1; }
            blank_to(&mut out, b, s, i, true); continue;
        }
        // block comment (nested when rust)
        if c == b'/' && i + 1 < n && b[i + 1] == b'*' {
            let s = i; let mut depth = 1; i += 2;
            while i < n && depth > 0 {
                if b[i] == b'/' && i + 1 < n && b[i + 1] == b'*' { if rust { depth += 1; } i += 2; }
                else if b[i] == b'*' && i + 1 < n && b[i + 1] == b'/' { depth -= 1; i += 2; }
                else { i += 1; }
            }
            blank_to(&mut out, b, s, i, true); continue;
        }
        // rust raw string: r"..." / r#"..."# / br#"..."#
        if rust {
            let mut j = i;
            if b[j] == b'b' && j + 1 < n { j += 1; }
            if b[j] == b'r' {
                let mut k = j + 1; let mut hashes = 0;
                while k < n && b[k] == b'#' { hashes += 1; k += 1; }
                if k < n && b[k] == b'"' {
                    let s = i; k += 1;
                    // closing = '"' followed by `hashes` '#'
                    loop {
                        if k >= n { break; }
                        if b[k] == b'"' {
                            let mut h = 0; while k + 1 + h < n && b[k + 1 + h] == b'#' { h += 1; }
                            if h >= hashes { k += 1 + hashes; break; }
                        }
                        k += 1;
                    }
                    blank_to(&mut out, b, s, k.min(n), true); i = k.min(n); continue;
                }
            }
        }
        // normal double-quoted string
        if c == b'"' {
            let s = i; i += 1;
            while i < n { if b[i] == b'\\' { i += 2; continue; } if b[i] == b'"' { i += 1; break; } i += 1; }
            blank_to(&mut out, b, s, i.min(n), rust); continue;
        }
        // backtick template (TS)
        if !rust && c == b'`' {
            let s = i; i += 1;
            while i < n { if b[i] == b'\\' { i += 2; continue; } if b[i] == b'`' { i += 1; break; } i += 1; }
            blank_to(&mut out, b, s, i.min(n), false); continue;
        }
        // single quote: rust char (lifetime-safe heuristic) vs TS string
        if c == b'\'' {
            if rust {
                // `'x'` or `'\n'` is a char; anything else (`'a`, `'static`) is a lifetime.
                let is_char = (i + 2 < n && b[i + 1] == b'\\')
                    || (i + 2 < n && b[i + 1] != b'\\' && b[i + 2] == b'\'');
                if is_char {
                    let s = i; i += 1;
                    if i < n && b[i] == b'\\' { i += 1; }
                    i += 1; if i < n && b[i] == b'\'' { i += 1; }
                    blank_to(&mut out, b, s, i.min(n), true); continue;
                }
                out.push(c); i += 1; continue; // lifetime
            } else {
                let s = i; i += 1;
                while i < n { if b[i] == b'\\' { i += 2; continue; } if b[i] == b'\'' { i += 1; break; } i += 1; }
                blank_to(&mut out, b, s, i.min(n), false); continue;
            }
        }
        out.push(c); i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| src.to_string())
}

fn line_of(content: &str, byte: usize) -> u32 {
    content[..byte.min(content.len())].bytes().filter(|&b| b == b'\n').count() as u32 + 1
}

fn parent_dir(path: &str) -> String {
    Path::new(path).parent().map(|d| d.to_string_lossy().replace('\\', "/")).unwrap_or_default()
}

/// `"name": "x"` from a package.json (first match; diet, no full JSON parse).
fn json_name(content: &str) -> Option<String> {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r#""name"\s*:\s*"([^"]+)""#).unwrap());
    re.captures(content).map(|c| c[1].to_string())
}

// ── Rust ────────────────────────────────────────────────────────────────────

pub struct RustResolver;

fn rust_mod_re() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?m)^[ \t]*(?:pub[ \t]*(?:\([^)]*\)[ \t]*)?)?mod[ \t]+(?:r#)?([A-Za-z_]\w*)[ \t]*;").unwrap())
}

fn rust_use_re() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?s)\buse[ \t\r\n]+([^;]+);").unwrap())
}

/// `#[path = "FILE"] mod NAME;` (attribute + decl, possibly across whitespace).
fn rust_path_mod_re() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(
        r#"(?s)#\[\s*path\s*=\s*"([^"]+)"\s*\]\s*(?:pub[^m;]*?)?mod[ \t]+(?:r#)?([A-Za-z_]\w*)[ \t]*;"#).unwrap())
}

impl ModuleResolver for RustResolver {
    fn exts(&self) -> &'static [&'static str] { &["rs"] }

    fn edges(&self, file: &str, content: &str, cx: &ProjectCx) -> Vec<ModuleRef> {
        let mut out = Vec::new();
        let mut path_mods: HashSet<String> = HashSet::new();
        let crates = cx.rust_crates();

        // `#[path = "x.rs"] mod foo;` — explicit override (read from RAW content so
        // the attribute string survives; the convention scan runs on stripped text).
        let base = mod_base_dir(file);
        for c in rust_path_mod_re().captures_iter(content) {
            let rel = &c[1];
            let name = c[2].to_string();
            path_mods.insert(name.clone());
            let line = line_of(content, c.get(0).unwrap().start());
            let cand = normalize_join(&base, rel);
            let target = if cx.files.contains(&cand) { Resolution::File(cand) }
                         else { Resolution::Unresolved(format!("#[path] {rel}: no file")) };
            out.push(ModuleRef { specifier: format!("#[path] mod {name}"), kind: "mod", line, span: None, target });
        }

        let clean = strip_noise(content, true);

        // `mod foo;` — submodule file by filesystem convention (skip #[path] ones).
        for c in rust_mod_re().captures_iter(&clean) {
            let name = &c[1];
            if path_mods.contains(name) { continue; }
            let line = line_of(&clean, c.get(0).unwrap().start());
            let target = match mod_child_candidates(file, name).into_iter()
                .find(|cand| cx.files.contains(cand)) {
                Some(f) => Resolution::File(f),
                None => Resolution::Unresolved(format!("mod {name}: no child file")),
            };
            out.push(ModuleRef { specifier: format!("mod {name}"), kind: "mod", line, span: None, target });
        }

        // `use path;` — intra- and cross-crate references. `expand_use` returns
        // each leaf's byte span relative to the captured body; add the body's
        // start in `clean` (offset-preserving vs raw content) for a file coord.
        for c in rust_use_re().captures_iter(&clean) {
            let line = line_of(&clean, c.get(0).unwrap().start());
            let body_start = c.get(1).unwrap().start() as u32;
            for (cand, lo, hi) in expand_use(&c[1]) {
                let target = crates.resolve_use(file, &cand);
                out.push(ModuleRef { specifier: cand, kind: "use", line,
                    span: Some((body_start + lo, body_start + hi)), target });
            }
        }
        out
    }
}

/// The directory `mod foo;` / `#[path]` resolves relative to: a directory-defining
/// file (mod.rs/lib.rs/main.rs) uses its own dir; a `name.rs` file uses `name/`.
fn mod_base_dir(file: &str) -> String {
    let p = Path::new(file);
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let dir = parent_dir(file);
    if matches!(stem, "mod" | "lib" | "main") { dir }
    else if dir.is_empty() { stem.to_string() }
    else { format!("{dir}/{stem}") }
}

/// Candidate child-module files for `mod <name>;` in `file`.
fn mod_child_candidates(file: &str, name: &str) -> Vec<String> {
    let base = mod_base_dir(file);
    let j = |suffix: &str| if base.is_empty() { suffix.to_string() } else { format!("{base}/{suffix}") };
    vec![j(&format!("{name}.rs")), j(&format!("{name}/mod.rs"))]
}

/// Join a `#[path]` value onto a base dir and normalize `.`/`..`/leading `./`.
fn normalize_join(base: &str, rel: &str) -> String {
    let mut segs: Vec<&str> = if base.is_empty() { Vec::new() } else { base.split('/').collect() };
    for s in rel.split('/') {
        match s {
            "" | "." => {}
            ".." => { segs.pop(); }
            other => segs.push(other),
        }
    }
    segs.join("/")
}

/// Expand a `use` body into the module-path candidates to resolve, recursing into
/// brace groups: `a::{b::{c,d}, e as f}` -> [a::b::c, a::b::d, a::e]. `self`/`*`
/// collapse to the enclosing module. `r#` raw-ident prefixes are stripped.
///
/// Each result carries `(lo, hi)`: the byte range, relative to `body`, of the
/// **contiguous rewrite coordinate** for that candidate, i.e. the text a module
/// move splices. For a bare `use a::b::c` that is the whole path. For a brace
/// group `a::{b, c}` the synthesized paths `a::b`/`a::c` are NOT contiguous, so
/// every leaf points at the *outermost head prefix* (`a`) — the part that moves
/// when module `a` moves, shared across siblings (dedups to one located row).
/// A move whose old module is deeper than that head (e.g. `use crate::{old::A}`)
/// simply won't prefix-match the head and is safely left alone.
fn expand_use(body: &str) -> Vec<(String, u32, u32)> {
    fn rec(prefix: &str, raw: &str, raw_off: usize, head_span: Option<(u32, u32)>,
           out: &mut Vec<(String, u32, u32)>) {
        let lead = raw.len() - raw.trim_start().len();
        let seg = raw.trim();
        if seg.is_empty() { return; }
        let seg_off = raw_off + lead; // absolute (body-relative) start of trimmed seg
        if let Some(bi) = seg.find('{') {
            let head = seg[..bi].trim().trim_end_matches(':');
            let np = join(prefix, head);
            // The first (outermost) brace fixes the head coordinate; deeper braces
            // inherit it. `head` starts at seg_off (seg is already left-trimmed).
            let this_head = head_span.or_else(||
                (!head.is_empty()).then(|| (seg_off as u32, (seg_off + head.len()) as u32)));
            let close = matching_brace(seg, bi).unwrap_or(seg.len());
            for (item, item_off) in split_top_commas(&seg[bi + 1..close]) {
                rec(&np, &item, seg_off + bi + 1 + item_off, this_head, out);
            }
        } else {
            // The contiguous leaf is the path before any ` as ` alias.
            let leaf = seg.split(" as ").next().unwrap_or(seg).trim_end();
            // Brace leaf -> the shared head span; bare leaf -> the whole path.
            let (lo, hi) = head_span
                .unwrap_or((seg_off as u32, (seg_off + leaf.len()) as u32));
            let full = if leaf == "self" || leaf == "*" || leaf.is_empty() {
                prefix.to_string()
            } else {
                join(prefix, leaf)
            };
            if !full.is_empty() { out.push((full.replace("r#", ""), lo, hi)); }
        }
    }
    let mut out = Vec::new();
    rec("", body, 0, None, &mut out);
    out
}

fn join(prefix: &str, head: &str) -> String {
    let head = head.trim_start_matches(':');
    if prefix.is_empty() { head.to_string() }
    else if head.is_empty() { prefix.to_string() }
    else { format!("{prefix}::{head}") }
}

fn matching_brace(s: &str, open: usize) -> Option<usize> {
    let b = s.as_bytes();
    let mut depth = 0;
    for i in open..b.len() {
        match b[i] { b'{' => depth += 1, b'}' => { depth -= 1; if depth == 0 { return Some(i); } }, _ => {} }
    }
    None
}

/// Split on top-level commas, returning each item with its start offset in `s`.
fn split_top_commas(s: &str) -> Vec<(String, usize)> {
    let mut out = Vec::new();
    let mut depth = 0; let mut start = 0;
    let b = s.as_bytes();
    for i in 0..b.len() {
        match b[i] {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            b',' if depth == 0 => { out.push((s[start..i].to_string(), start)); start = i + 1; }
            _ => {}
        }
    }
    out.push((s[start..].to_string(), start));
    out
}

/// Cargo workspace registry: per-crate `crate::`-namespaced module index, crate
/// name -> src root, and file -> owning crate. Solves the multi-crate `crate::`
/// collision and enables cross-crate `use othercrate::..`.
struct RustCrates {
    name_to_src: HashMap<String, String>,    // crate name -> "<dir>/src"
    roots: Vec<(String, String)>,            // (src root, crate name), longest first
    index: HashMap<(String, String), String>, // (crate name, mod path) -> file
    renames: HashMap<(String, String), String>, // (owner crate, code name) -> real package
}

impl RustCrates {
    fn build(files: &HashSet<String>, manifests: &HashMap<String, String>) -> Self {
        let mut name_to_src = HashMap::new();
        let mut roots: Vec<(String, String)> = Vec::new();
        let mut renames: HashMap<(String, String), String> = HashMap::new();
        for (path, content) in manifests {
            if !path.ends_with("Cargo.toml") { continue; }
            let (name, rns) = parse_cargo(content);
            let Some(name) = name else { continue };
            let dir = parent_dir(path);
            let src = if dir.is_empty() { "src".to_string() } else { format!("{dir}/src") };
            name_to_src.insert(name.clone(), src.clone());
            roots.push((src, name.clone()));
            // `renamed = { package = "real" }`: code uses `renamed`, crate is `real`.
            for (code, real) in rns { renames.insert((name.clone(), code), real); }
        }
        roots.sort_by(|a, b| b.0.len().cmp(&a.0.len()));

        let owner = |file: &str| -> String {
            for (src, name) in &roots {
                if file == src || file.starts_with(&format!("{src}/")) { return name.clone(); }
            }
            String::new() // no manifest: single anonymous crate
        };
        let mut index = HashMap::new();
        for f in files {
            if !f.ends_with(".rs") { continue; }
            if let Some(mp) = file_to_mod_path(f) {
                index.entry((owner(f), mp)).or_insert_with(|| f.clone());
            }
        }
        RustCrates { name_to_src, roots, index, renames }
    }

    fn owner(&self, file: &str) -> String {
        for (src, name) in &self.roots {
            if file == src || file.starts_with(&format!("{src}/")) { return name.clone(); }
        }
        String::new()
    }

    fn lookup(&self, crate_name: &str, abs: &str) -> Option<String> {
        let segs: Vec<&str> = abs.split("::").collect();
        for len in (1..=segs.len()).rev() {
            let p = segs[..len].join("::");
            if let Some(f) = self.index.get(&(crate_name.to_string(), p)) { return Some(f.clone()); }
        }
        None
    }

    fn resolve_use(&self, from_file: &str, use_path: &str) -> Resolution {
        let owner = self.owner(from_file);
        let from_mod = file_to_mod_path(from_file).unwrap_or_else(|| "crate".to_string());
        if let Some(abs) = resolve_to_absolute(use_path, &from_mod) {
            return match self.lookup(&owner, &abs) {
                Some(f) => Resolution::File(f),
                None => Resolution::Unresolved(format!("{abs}: no file in crate '{owner}'")),
            };
        }
        // cross-crate: first segment names another workspace crate, directly or
        // via a Cargo `package =` rename in the importing crate's manifest.
        let mut segs = use_path.split("::");
        let first = segs.next().unwrap_or("").trim_start_matches("r#");
        let real = if self.name_to_src.contains_key(first) {
            Some(first.to_string())
        } else {
            self.renames.get(&(owner.clone(), first.to_string())).cloned()
        };
        if let Some(pkg) = real {
            let rest: Vec<&str> = segs.collect();
            let abs2 = if rest.is_empty() { "crate".to_string() } else { format!("crate::{}", rest.join("::")) };
            return match self.lookup(&pkg, &abs2) {
                Some(f) => Resolution::File(f),
                None => Resolution::Unresolved(format!("{use_path}: no file in crate '{pkg}'")),
            };
        }
        Resolution::External(use_path.to_string())
    }
}

/// Parse a Cargo.toml into `([package].name, [(code_name, real_package)] renames)`.
/// A rename is a dependency given as `code = { package = "real" }` where the name
/// used in `use code::..` differs from the actual crate. Uses the `toml` crate so
/// inline tables, `[dependencies.x]` sections, and quoted keys all parse correctly.
fn parse_cargo(content: &str) -> (Option<String>, Vec<(String, String)>) {
    let Ok(val) = toml::from_str::<toml::Value>(content) else { return (None, Vec::new()); };
    let name = val.get("package").and_then(|p| p.get("name"))
        .and_then(|n| n.as_str()).map(|s| s.to_string());
    let mut renames = Vec::new();
    for sec in ["dependencies", "dev-dependencies", "build-dependencies"] {
        let Some(deps) = val.get(sec).and_then(|d| d.as_table()) else { continue };
        for (code, spec) in deps {
            if let Some(pkg) = spec.as_table().and_then(|t| t.get("package")).and_then(|p| p.as_str()) {
                if pkg != code { renames.push((code.clone(), pkg.to_string())); }
            }
        }
    }
    (name, renames)
}

/// Workspace-internal crate dependency edges from Cargo.toml manifests.
pub fn crate_edges(manifests: &HashMap<String, String>) -> Vec<CrateEdge> {
    let mut packages = HashSet::new();
    let mut deps: Vec<(String, String, &'static str)> = Vec::new();
    for (path, content) in manifests {
        if !path.ends_with("Cargo.toml") { continue; }
        let Ok(val) = toml::from_str::<toml::Value>(content) else { continue };
        let Some(src) = val.get("package").and_then(|p| p.get("name"))
            .and_then(|n| n.as_str()).map(|s| s.to_string()) else { continue };
        packages.insert(src.clone());
        for (sec, kind) in [
            ("dependencies", "dependencies"),
            ("dev-dependencies", "dev-dependencies"),
            ("build-dependencies", "build-dependencies"),
        ] {
            let Some(table) = val.get(sec).and_then(|d| d.as_table()) else { continue };
            for (code, spec) in table {
                let dst = spec.as_table()
                    .and_then(|t| t.get("package"))
                    .and_then(|p| p.as_str())
                    .unwrap_or(code)
                    .to_string();
                deps.push((src.clone(), dst, kind));
            }
        }
    }
    let mut out = BTreeSet::new();
    for (src, dst, kind) in deps {
        if packages.contains(&dst) && src != dst { out.insert(CrateEdge { src, dst, kind }); }
    }
    out.into_iter().collect()
}

/// File path -> Rust module path. None if the path has no `src/` segment.
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

/// Resolve a use path to absolute `crate::` form. `crate::` passes through;
/// `self::`/`super::` resolve against `from_mod`; external/other-crate paths None.
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
                None => return None,
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
    // import/export ... from "..."  |  bare import "..."  |  import("...")  |  require("...")
    // quote class includes the backtick so a static template literal is caught.
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(
        r#"(?m)(?:\bfrom|\bimport|\brequire)[ \t]*\(?[ \t]*['"`]([^'"`]+)['"`]"#).unwrap())
}

impl TsResolver {
    fn new(root: &Path) -> Option<Self> {
        use oxc_resolver::{ResolveOptions, Resolver, TsconfigDiscovery};
        let options = ResolveOptions {
            extensions: [".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".mts", ".cts", ".json"]
                .iter().map(|s| s.to_string()).collect(),
            main_fields: vec!["module".into(), "main".into()],
            condition_names: vec!["import".into(), "require".into(), "default".into()],
            // Auto = discover the nearest tsconfig.json per importing file (monorepo).
            tsconfig: Some(TsconfigDiscovery::Auto),
            ..ResolveOptions::default()
        };
        Some(TsResolver { resolver: Resolver::new(options), root: root.to_path_buf() })
    }

    /// Workspace package.json fallback for a bare specifier oxc could not find
    /// (no node_modules symlink): map the package name to its directory and probe
    /// the directory's entry within the file set.
    fn workspace_fallback(&self, spec: &str, cx: &ProjectCx) -> Option<String> {
        let pkg = if let Some(rest) = spec.strip_prefix('@') {
            let mut it = rest.splitn(2, '/');
            format!("@{}/{}", it.next()?, it.next()?)
        } else {
            spec.split('/').next()?.to_string()
        };
        let dir = cx.ts_packages().get(&pkg)?.clone();
        let sub = spec[pkg.len()..].trim_start_matches('/');
        let stem = if sub.is_empty() { dir.clone() } else { format!("{dir}/{sub}") };
        for cand in [
            stem.clone(),
            format!("{stem}.ts"), format!("{stem}.tsx"), format!("{stem}.js"),
            format!("{stem}/index.ts"), format!("{stem}/index.tsx"), format!("{stem}/index.js"),
            format!("{dir}/index.ts"), format!("{dir}/index.tsx"), format!("{dir}/index.js"),
        ] {
            if cx.files.contains(&cand) { return Some(cand); }
        }
        None
    }
}

impl ModuleResolver for TsResolver {
    fn exts(&self) -> &'static [&'static str] {
        &["ts", "tsx", "js", "jsx", "mjs", "cjs", "mts", "cts"]
    }

    fn edges(&self, file: &str, content: &str, cx: &ProjectCx) -> Vec<ModuleRef> {
        let clean = strip_noise(content, false);
        let abs = self.root.join(file);
        let mut out = Vec::new();
        for c in ts_spec_re().captures_iter(&clean) {
            let spec = c[1].to_string();
            let line = line_of(&clean, c.get(0).unwrap().start());
            let m = c.get(1).unwrap();
            let span = Some((m.start() as u32, m.end() as u32));
            // a template literal with interpolation cannot be resolved statically
            if spec.contains("${") {
                out.push(ModuleRef { specifier: spec.clone(), kind: "import", line, span: None,
                    target: Resolution::Unresolved(format!("{spec}: dynamic")) });
                continue;
            }
            // resolve_file (not resolve) so TsconfigDiscovery::Auto finds the nearest
            // tsconfig.json walking up from the importing file (per-package monorepo).
            let target = match self.resolver.resolve_file(&abs, &spec) {
                Ok(r) => {
                    let full = r.full_path();
                    match full.strip_prefix(&self.root).ok()
                        .map(|p| p.to_string_lossy().replace('\\', "/"))
                        .filter(|rel| cx.files.contains(rel)) {
                        Some(rel) => Resolution::File(rel),
                        None => Resolution::External(spec.clone()),
                    }
                }
                Err(_) => match self.workspace_fallback(&spec, cx) {
                    Some(f) => Resolution::File(f),
                    None if spec.starts_with('.') => Resolution::Unresolved(format!("{spec}: unresolved")),
                    None => Resolution::External(spec.clone()),
                },
            };
            out.push(ModuleRef { specifier: spec, kind: "import", line, span, target });
        }
        out
    }
}

// ── Kotlin ──────────────────────────────────────────────────────────────────
//
// Static resolution without kotlinc. The compiler's rule is that the `package`
// declaration is the source of truth and the directory layout is advisory, so
// the index maps package -> files and (package, top-level decl name) -> file by
// reading every .kt's header and column-0 declarations. `import a.b.C` then
// resolves by longest package prefix; `import a.b.*` edges to every file of the
// package. Residual (documented, not silently wrong): same-package references
// without an import produce no edge, and expect/actual twins collide on the
// decl key (first sorted file wins). Compiler-exact graphs come from scip-kotlin
// via the SCIP importer instead.

pub struct KotlinResolver;

struct KotlinIndex {
    /// package name -> .kt files declaring it (sorted, deterministic).
    packages: HashMap<String, Vec<String>>,
    /// (package, top-level decl name) -> defining file (first sorted file wins).
    decls: HashMap<(String, String), String>,
}

fn kotlin_package_re() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?m)^[ \t]*package[ \t]+([\w.`]+)").unwrap())
}

fn kotlin_import_re() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?m)^[ \t]*import[ \t]+([\w`]+(?:\.[\w`]+)*(?:\.\*)?)").unwrap())
}

/// Column-0 declarations only — Kotlin convention indents members, so a line
/// that starts at column 0 with modifiers + a declaring keyword is top-level.
/// `fun interface Name` comes before bare `fun` in the alternation; an optional
/// generic param list and extension receiver sit between `fun` and the name.
fn kotlin_decl_re() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(
        r"(?m)^(?:(?:public|internal|private|protected|open|abstract|final|sealed|data|inline|value|annotation|enum|expect|actual|external|suspend|operator|infix|tailrec|const)[ \t]+)*(?:fun[ \t]+interface|class|interface|object|fun|val|var|typealias)[ \t]+(?:<[^>\n]*>[ \t]*)?(?:[\w.<>?]+\.)?(`[^`\n]+`|\w+)").unwrap())
}

/// Blank `"""…"""` bodies (newlines kept) so a raw string containing `import`
/// lines never reaches the import scan; then the shared rust-mode strip handles
/// `//`, nested `/* */`, ordinary strings, and chars — all valid for Kotlin.
fn strip_kotlin(src: &str) -> String {
    let b = src.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'"' && b.get(i + 1) == Some(&b'"') && b.get(i + 2) == Some(&b'"') {
            let mut j = i + 3;
            while j < b.len() && !(b[j] == b'"' && b.get(j + 1) == Some(&b'"') && b.get(j + 2) == Some(&b'"')) { j += 1; }
            let end = (j + 3).min(b.len());
            for k in i..end { out.push(if b[k] == b'\n' { b'\n' } else { b' ' }); }
            i = end;
        } else {
            out.push(b[i]); i += 1;
        }
    }
    strip_noise(&String::from_utf8(out).unwrap_or_else(|_| src.to_string()), true)
}

impl KotlinIndex {
    fn build(files: &HashSet<String>, reader: Option<&(dyn Fn(&str) -> Option<String> + Send + Sync)>) -> Self {
        let mut packages: HashMap<String, Vec<String>> = HashMap::new();
        let mut decls = HashMap::new();
        let Some(read) = reader else { return KotlinIndex { packages, decls } };
        let mut kt: Vec<&String> = files.iter().filter(|f| f.ends_with(".kt") || f.ends_with(".kts")).collect();
        kt.sort();
        for f in kt {
            let Some(content) = read(f) else { continue };
            let clean = strip_kotlin(&content);
            let pkg = kotlin_package_re().captures(&clean)
                .map(|c| c[1].replace('`', "")).unwrap_or_default();
            packages.entry(pkg.clone()).or_default().push(f.clone());
            for c in kotlin_decl_re().captures_iter(&clean) {
                let name = c[1].trim_matches('`').to_string();
                decls.entry((pkg.clone(), name)).or_insert_with(|| f.clone());
            }
        }
        KotlinIndex { packages, decls }
    }

    /// Longest-package-prefix resolution: first prefix split whose (package,
    /// next segment) is a known decl wins (handles `a.b.Outer.Inner`); else a
    /// known package with no such decl is Unresolved; else External (a jar).
    fn resolve(&self, spec: &str) -> Resolution {
        let segs: Vec<&str> = spec.split('.').collect();
        for len in (1..segs.len()).rev() {
            let pkg = segs[..len].join(".");
            if let Some(f) = self.decls.get(&(pkg, segs[len].to_string())) {
                return Resolution::File(f.clone());
            }
        }
        for len in (1..segs.len()).rev() {
            if self.packages.contains_key(&segs[..len].join(".")) {
                return Resolution::Unresolved(
                    format!("{spec}: no column-0 decl `{}` in package {}", segs[len], segs[..len].join(".")));
            }
        }
        Resolution::External(spec.to_string())
    }
}

impl ModuleResolver for KotlinResolver {
    fn exts(&self) -> &'static [&'static str] { &["kt", "kts"] }

    fn edges(&self, file: &str, content: &str, cx: &ProjectCx) -> Vec<ModuleRef> {
        let clean = strip_kotlin(content);
        let idx = cx.kotlin_index();
        let mut out = Vec::new();
        for c in kotlin_import_re().captures_iter(&clean) {
            let m = c.get(1).unwrap();
            let spec = m.as_str().replace('`', "");
            let line = line_of(&clean, c.get(0).unwrap().start());
            let span = Some((m.start() as u32, m.end() as u32));
            if let Some(pkg) = spec.strip_suffix(".*") {
                match idx.packages.get(pkg) {
                    // a wildcard import depends on every file of the package
                    Some(fs) => for f in fs.iter().filter(|f| f.as_str() != file) {
                        out.push(ModuleRef { specifier: spec.clone(), kind: "import", line, span,
                            target: Resolution::File(f.clone()) });
                    },
                    None => out.push(ModuleRef { specifier: spec.clone(), kind: "import", line, span,
                        target: Resolution::External(spec.clone()) }),
                }
                continue;
            }
            out.push(ModuleRef { specifier: spec.clone(), kind: "import", line, span,
                target: idx.resolve(&spec) });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cx<'a>(root: &'a Path, files: &'a HashSet<String>, manifests: &'a HashMap<String, String>) -> ProjectCx<'a> {
        ProjectCx::new(root, files, manifests)
    }
    fn set(paths: &[&str]) -> HashSet<String> { paths.iter().map(|s| s.to_string()).collect() }
    fn no_manifests() -> HashMap<String, String> { HashMap::new() }

    #[test]
    fn mod_path_basics() {
        assert_eq!(file_to_mod_path("src/lib.rs").as_deref(), Some("crate"));
        assert_eq!(file_to_mod_path("src/foo/bar.rs").as_deref(), Some("crate::foo::bar"));
        assert_eq!(file_to_mod_path("src/foo/mod.rs").as_deref(), Some("crate::foo"));
        assert_eq!(file_to_mod_path("lib/foo.rs"), None);
    }

    #[test]
    fn resolve_absolute() {
        assert_eq!(resolve_to_absolute("self::b::C", "crate::a").as_deref(), Some("crate::a::b::C"));
        assert_eq!(resolve_to_absolute("super::b::C", "crate::a::consumer").as_deref(), Some("crate::a::b::C"));
        assert_eq!(resolve_to_absolute("std::io::Read", "crate::a"), None);
    }

    #[test]
    fn mod_and_use_edges() {
        let files = set(&["src/lib.rs", "src/parser.rs", "src/parser/expr.rs"]);
        let m = no_manifests();
        let c = cx(Path::new("/repo"), &files, &m);
        assert_eq!(RustResolver.edges("src/lib.rs", "mod parser;\n", &c)[0].target,
            Resolution::File("src/parser.rs".into()));
        assert_eq!(RustResolver.edges("src/parser.rs", "mod expr;\n", &c)[0].target,
            Resolution::File("src/parser/expr.rs".into()));
        let e = RustResolver.edges("src/lib.rs", "use crate::parser::expr::Foo;\n", &c);
        assert_eq!(e.iter().find(|r| r.kind == "use").unwrap().target,
            Resolution::File("src/parser/expr.rs".into()));
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
        assert_eq!(&body[*lo as usize..*hi as usize], "crate::parser::expr::Foo");
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
        assert_eq!(v.iter().map(|(p, _, _)| p.clone()).collect::<Vec<_>>(),
            vec!["crate::match::X".to_string()]);
    }

    #[test]
    fn comment_and_string_use_is_ignored() {
        let files = set(&["src/lib.rs", "src/real.rs"]);
        let m = no_manifests();
        let c = cx(Path::new("/repo"), &files, &m);
        let src = "// use crate::commented::X;\nlet s = \"use crate::instring::Y;\";\nmod real;\n";
        let e = RustResolver.edges("src/lib.rs", src, &c);
        assert!(e.iter().all(|r| !r.specifier.contains("commented") && !r.specifier.contains("instring")),
            "comment/string uses must not produce edges: {e:?}");
        assert!(e.iter().any(|r| r.kind == "mod" && r.target == Resolution::File("src/real.rs".into())));
    }

    #[test]
    fn path_attribute_override() {
        let files = set(&["src/lib.rs", "src/weird/place.rs"]);
        let m = no_manifests();
        let c = cx(Path::new("/repo"), &files, &m);
        let e = RustResolver.edges("src/lib.rs", "#[path = \"weird/place.rs\"]\nmod foo;\n", &c);
        let mods: Vec<&ModuleRef> = e.iter().filter(|r| r.kind == "mod").collect();
        assert_eq!(mods.len(), 1, "exactly one mod edge (no double-count): {e:?}");
        assert_eq!(mods[0].target, Resolution::File("src/weird/place.rs".into()));
    }

    #[test]
    fn multi_crate_namespace_and_cross_crate() {
        let files = set(&[
            "crateA/src/lib.rs", "crateA/src/foo.rs",
            "crateB/src/lib.rs", "crateB/src/foo.rs",
        ]);
        let mut m = HashMap::new();
        m.insert("crateA/Cargo.toml".to_string(), "[package]\nname = \"crate_a\"\n".to_string());
        m.insert("crateB/Cargo.toml".to_string(), "[package]\nname = \"crate_b\"\n".to_string());
        let c = cx(Path::new("/repo"), &files, &m);
        // crateA's `use crate::foo` must resolve to crateA's foo, NOT crateB's.
        let e = RustResolver.edges("crateA/src/lib.rs", "use crate::foo::A;\n", &c);
        assert_eq!(e.iter().find(|r| r.kind == "use").unwrap().target,
            Resolution::File("crateA/src/foo.rs".into()), "crate:: must stay in-crate: {e:?}");
        // cross-crate `use crate_b::foo::B` resolves into crateB.
        let e2 = RustResolver.edges("crateA/src/lib.rs", "use crate_b::foo::B;\n", &c);
        assert_eq!(e2.iter().find(|r| r.kind == "use").unwrap().target,
            Resolution::File("crateB/src/foo.rs".into()), "cross-crate use must resolve: {e2:?}");
    }

    fn kt_reader(contents: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> + Send + Sync {
        let m: HashMap<String, String> =
            contents.iter().map(|(p, c)| (p.to_string(), c.to_string())).collect();
        move |p: &str| m.get(p).cloned()
    }

    #[test]
    fn kotlin_package_decl_beats_directory_layout() {
        // B.kt lives in a directory that does NOT match its package; the
        // declaration is truth, so the import still resolves.
        let contents = [
            ("weird/spot/B.kt", "package com.foo\n\nclass Bar(val n: Int)\nfun topLevel() = 1\n"),
            ("app/Main.kt", "package com.app\n\nimport com.foo.Bar\nimport com.foo.topLevel\n"),
        ];
        let files = set(&["weird/spot/B.kt", "app/Main.kt"]);
        let m = no_manifests();
        let r = kt_reader(&contents);
        let c = cx(Path::new("/repo"), &files, &m).with_reader(&r);
        let e = KotlinResolver.edges("app/Main.kt", contents[1].1, &c);
        assert_eq!(e.len(), 2, "{e:?}");
        for r in &e {
            assert_eq!(r.target, Resolution::File("weird/spot/B.kt".into()), "{r:?}");
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
        assert!(e.iter().any(|r| matches!(&r.target, Resolution::External(s) if s == "kotlin.collections.List")), "{e:?}");
        assert!(e.iter().any(|r| matches!(&r.target, Resolution::Unresolved(s) if s.contains("Missing"))), "{e:?}");
    }

    #[test]
    fn kotlin_nested_class_and_noise_immunity() {
        let contents = [
            ("lib/A.kt", "package com.foo\nclass Outer {\n    class Inner\n}\n"),
            ("app/Main.kt", concat!(
                "package com.app\n",
                "// import com.foo.Commented\n",
                "val s = \"\"\"\nimport com.foo.InString\n\"\"\"\n",
                "import com.foo.Outer.Inner\n")),
        ];
        let files = set(&["lib/A.kt", "app/Main.kt"]);
        let m = no_manifests();
        let r = kt_reader(&contents);
        let c = cx(Path::new("/repo"), &files, &m).with_reader(&r);
        let e = KotlinResolver.edges("app/Main.kt", contents[1].1, &c);
        assert_eq!(e.len(), 1, "comment/raw-string imports must not match: {e:?}");
        // `Outer.Inner` resolves through the (package, Outer) decl
        assert_eq!(e[0].target, Resolution::File("lib/A.kt".into()), "{e:?}");
        // span is the dotted path text (the rewrite coordinate)
        let (lo, hi) = e[0].span.unwrap();
        assert_eq!(&contents[1].1[lo as usize..hi as usize], "com.foo.Outer.Inner");
    }

    #[test]
    fn cargo_dependency_rename() {
        let files = set(&["crateA/src/lib.rs", "crateB/src/lib.rs", "crateB/src/foo.rs"]);
        let mut m = HashMap::new();
        // crateA depends on crate_b under the alias `renamed`.
        m.insert("crateA/Cargo.toml".to_string(),
            "[package]\nname = \"crate_a\"\n\n[dependencies]\nrenamed = { package = \"crate_b\", path = \"../crateB\" }\n".to_string());
        m.insert("crateB/Cargo.toml".to_string(), "[package]\nname = \"crate_b\"\n".to_string());
        let c = cx(Path::new("/repo"), &files, &m);
        // `use renamed::foo::B` must resolve through the rename to crate_b's foo.
        let e = RustResolver.edges("crateA/src/lib.rs", "use renamed::foo::B;\n", &c);
        assert_eq!(e.iter().find(|r| r.kind == "use").unwrap().target,
            Resolution::File("crateB/src/foo.rs".into()), "package= rename must resolve: {e:?}");
    }
}
