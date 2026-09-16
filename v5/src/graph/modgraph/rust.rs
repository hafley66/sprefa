use super::*;

pub struct RustResolver;

fn rust_mod_re() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?m)^[ \t]*(?:pub[ \t]*(?:\([^)]*\)[ \t]*)?)?mod[ \t]+(?:r#)?([A-Za-z_]\w*)[ \t]*;",
        )
        .unwrap()
    })
}

fn rust_use_re() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?s)\buse[ \t\r\n]+([^;]+);").unwrap())
}

/// Whether the `use` statement starting at `use_start` in `clean` is a
/// re-export (`pub use ...` / `pub(crate) use ...` / ...): the `module_binding`
/// `kind` distinguishes a Rust `pub use` from a plain `use` (both are `use`
/// statements to `module_edge`, but only the `pub` one re-exports the name).
/// String-level check on the same line, not a full visibility parse.
fn rust_use_is_reexport(clean: &str, use_start: usize) -> bool {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"pub(\([^)]*\))?[ \t]*$").unwrap());
    let line_start = clean[..use_start].rfind('\n').map(|i| i + 1).unwrap_or(0);
    re.is_match(&clean[line_start..use_start])
}

/// `#[path = "FILE"] mod NAME;` (attribute + decl, possibly across whitespace).
fn rust_path_mod_re() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(
        r#"(?s)#\[\s*path\s*=\s*"([^"]+)"\s*\]\s*(?:pub[^m;]*?)?mod[ \t]+(?:r#)?([A-Za-z_]\w*)[ \t]*;"#).unwrap())
}

impl ModuleResolver for RustResolver {
    fn exts(&self) -> &'static [&'static str] {
        &["rs"]
    }

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
            let target = if cx.files.contains(&cand) {
                Resolution::File(cand)
            } else {
                Resolution::Unresolved(format!("#[path] {rel}: no file"))
            };
            out.push(ModuleRef {
                specifier: format!("#[path] mod {name}"),
                kind: "mod",
                line,
                span: None,
                target,
                bindings: vec![],
                module_bindings: vec![],
            });
        }

        let clean = strip_noise(content, true);

        // `mod foo;` — submodule file by filesystem convention (skip #[path] ones).
        for c in rust_mod_re().captures_iter(&clean) {
            let name = &c[1];
            if path_mods.contains(name) {
                continue;
            }
            let line = line_of(&clean, c.get(0).unwrap().start());
            let target = match mod_child_candidates(file, name)
                .into_iter()
                .find(|cand| cx.files.contains(cand))
            {
                Some(f) => Resolution::File(f),
                None => Resolution::Unresolved(format!("mod {name}: no child file")),
            };
            out.push(ModuleRef {
                specifier: format!("mod {name}"),
                kind: "mod",
                line,
                span: None,
                target,
                bindings: vec![],
                module_bindings: vec![],
            });
        }

        // `use path;` — intra- and cross-crate references. `expand_use_leaves`
        // (not the `expand_use` projection) so each leaf's alias survives;
        // its head/leaf span relative to the captured body plus the body's
        // start in `clean` (offset-preserving vs raw content) gives a file
        // coord, same as before.
        for c in rust_use_re().captures_iter(&clean) {
            let use_start = c.get(0).unwrap().start();
            let line = line_of(&clean, use_start);
            let body_start = c.get(1).unwrap().start() as u32;
            let reexport = rust_use_is_reexport(&clean, use_start);
            for leaf in expand_use_leaves(&c[1]) {
                let (lo, hi) = leaf.head.unwrap_or(leaf.leaf);
                let target = crates.resolve_use(file, &leaf.full);
                let bindings = match &leaf.alias {
                    Some(alias) if !leaf.collapsed => {
                        let source = leaf.full.rsplit("::").next().unwrap_or(&leaf.full);
                        vec![(alias.clone(), source.to_string())]
                    }
                    _ => vec![],
                };
                // `self`/`*` leaves bind no single local name (see the
                // `ModuleRef::module_bindings` doc); a plain leaf's local name
                // is its own last segment unless aliased.
                let module_bindings = if leaf.collapsed {
                    vec![]
                } else {
                    let source = leaf
                        .full
                        .rsplit("::")
                        .next()
                        .unwrap_or(&leaf.full)
                        .to_string();
                    let local = leaf.alias.clone().unwrap_or_else(|| source.clone());
                    let kind = if reexport { "reexport" } else { "named" };
                    vec![(local, source, kind)]
                };
                out.push(ModuleRef {
                    specifier: leaf.full,
                    kind: "use",
                    line,
                    span: Some((body_start + lo, body_start + hi)),
                    target,
                    bindings,
                    module_bindings,
                });
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
    if matches!(stem, "mod" | "lib" | "main") {
        dir
    } else if dir.is_empty() {
        stem.to_string()
    } else {
        format!("{dir}/{stem}")
    }
}

/// Candidate child-module files for `mod <name>;` in `file`.
fn mod_child_candidates(file: &str, name: &str) -> Vec<String> {
    let base = mod_base_dir(file);
    let j = |suffix: &str| {
        if base.is_empty() {
            suffix.to_string()
        } else {
            format!("{base}/{suffix}")
        }
    };
    vec![j(&format!("{name}.rs")), j(&format!("{name}/mod.rs"))]
}

/// One expanded `use` leaf with BOTH coordinates: the shared brace-head span
/// (the located/ref-spine coordinate, `None` for a bare use) and the leaf's own
/// contiguous span. `prefix` is the path accumulated above the leaf ("" for a
/// bare use), `full` the synthesized module path. Spans are body-relative.
pub struct UseLeaf {
    pub full: String,
    pub prefix: String,
    pub leaf: (u32, u32),
    pub head: Option<(u32, u32)>,
    /// Span of the whole `use` statement BODY (between `use ` and `;`), shared
    /// by every leaf of the statement — the splice coordinate when a move
    /// forces a statement-level regroup. Body-relative `(0, body.len())` out of
    /// `expand_use_leaves`; file-absolute out of `rust_use_leaves`.
    pub body: (u32, u32),
    /// `self` / `*` leaf: its own span is the keyword, not a rewritable path.
    pub collapsed: bool,
    /// The ` as alias` binding on this leaf, when present (`r#` stripped);
    /// `None` for a `self`/`*` leaf even if written with one (no meaningful
    /// local binding to alias).
    pub alias: Option<String>,
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
/// won't prefix-match the head; the move sink's leaf-level second pass
/// (`use_leaves`) covers it.
/// Test-only: `RustResolver::edges` now calls `expand_use_leaves` directly
/// (it needs each leaf's `alias`, which this projection drops); kept for the
/// brace/raw-ident/span unit tests below, which exercise the projection
/// itself.
#[cfg(test)]
pub(crate) fn expand_use(body: &str) -> Vec<(String, u32, u32)> {
    expand_use_leaves(body)
        .into_iter()
        .map(|l| {
            let (lo, hi) = l.head.unwrap_or(l.leaf);
            (l.full, lo, hi)
        })
        .collect()
}

/// The full per-leaf expansion behind `expand_use` — same recursion, nothing
/// projected away.
pub fn expand_use_leaves(body: &str) -> Vec<UseLeaf> {
    fn rec(
        prefix: &str,
        raw: &str,
        raw_off: usize,
        head_span: Option<(u32, u32)>,
        out: &mut Vec<UseLeaf>,
    ) {
        let lead = raw.len() - raw.trim_start().len();
        let seg = raw.trim();
        if seg.is_empty() {
            return;
        }
        let seg_off = raw_off + lead; // absolute (body-relative) start of trimmed seg
        if let Some(bi) = seg.find('{') {
            let head = seg[..bi].trim().trim_end_matches(':');
            let np = join(prefix, head);
            // The first (outermost) brace fixes the head coordinate; deeper braces
            // inherit it. `head` starts at seg_off (seg is already left-trimmed).
            let this_head = head_span.or_else(|| {
                (!head.is_empty()).then(|| (seg_off as u32, (seg_off + head.len()) as u32))
            });
            let close = matching_brace(seg, bi).unwrap_or(seg.len());
            for (item, item_off) in split_top_commas(&seg[bi + 1..close]) {
                rec(&np, &item, seg_off + bi + 1 + item_off, this_head, out);
            }
        } else {
            // The contiguous leaf is the path before any ` as ` alias.
            let mut parts = seg.split(" as ");
            let leaf = parts.next().unwrap_or(seg).trim_end();
            let alias = parts
                .next()
                .map(|a| a.trim().trim_start_matches("r#").to_string());
            let collapsed = leaf == "self" || leaf == "*" || leaf.is_empty();
            let full = if collapsed {
                prefix.to_string()
            } else {
                join(prefix, leaf)
            };
            if !full.is_empty() {
                out.push(UseLeaf {
                    full: full.replace("r#", ""),
                    prefix: prefix.replace("r#", ""),
                    leaf: (seg_off as u32, (seg_off + leaf.len()) as u32),
                    head: head_span,
                    body: (0, 0), // filled by the caller (body length unknown here)
                    collapsed,
                    alias: if collapsed { None } else { alias },
                });
            }
        }
    }
    let mut out = Vec::new();
    rec("", body, 0, None, &mut out);
    for l in &mut out {
        l.body = (0, body.len() as u32);
    }
    out
}

/// Every `use` leaf in a file's content with file-absolute spans, comment/
/// string-stripped before matching (offset-preserving, so spans splice into the
/// raw content). The move sink's leaf-level pass.
pub fn rust_use_leaves(content: &str) -> Vec<UseLeaf> {
    let clean = strip_noise(content, true);
    let mut out = Vec::new();
    for c in rust_use_re().captures_iter(&clean) {
        let body_start = c.get(1).unwrap().start() as u32;
        for mut l in expand_use_leaves(&c[1]) {
            l.leaf = (body_start + l.leaf.0, body_start + l.leaf.1);
            l.head = l.head.map(|(lo, hi)| (body_start + lo, body_start + hi));
            l.body = (body_start + l.body.0, body_start + l.body.1);
            out.push(l);
        }
    }
    out
}

/// Cargo workspace registry: per-crate `crate::`-namespaced module index, crate
/// name -> src root, and file -> owning crate. Solves the multi-crate `crate::`
/// collision and enables cross-crate `use othercrate::..`.
pub(crate) struct RustCrates {
    name_to_src: HashMap<String, String>, // crate name -> "<dir>/src"
    roots: Vec<(String, String)>,         // (src root, crate name), longest first
    index: HashMap<(String, String), String>, // (crate name, mod path) -> file
    renames: HashMap<(String, String), String>, // (owner crate, code name) -> real package
}

impl RustCrates {
    pub(crate) fn build(files: &HashSet<String>, manifests: &HashMap<String, String>) -> Self {
        let mut name_to_src = HashMap::new();
        let mut roots: Vec<(String, String)> = Vec::new();
        let mut renames: HashMap<(String, String), String> = HashMap::new();
        let mut sorted_manifests: Vec<_> = manifests.iter().collect();
        sorted_manifests.sort_by_key(|(manifest_path, _)| *manifest_path);
        for (path, content) in sorted_manifests {
            if !path.ends_with("Cargo.toml") {
                continue;
            }
            let (name, rns) = parse_cargo(content);
            let Some(name) = name else { continue };
            let dir = parent_dir(path);
            let src = if dir.is_empty() {
                "src".to_string()
            } else {
                format!("{dir}/src")
            };
            name_to_src
                .entry(name.clone())
                .or_insert_with(|| src.clone());
            roots.push((src, name.clone()));
            // `renamed = { package = "real" }`: code uses `renamed`, crate is `real`.
            for (code, real) in rns {
                renames.entry((name.clone(), code)).or_insert(real);
            }
        }
        roots.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.cmp(b)));

        let owner = |file: &str| -> String {
            for (src, name) in &roots {
                if file == src || file.starts_with(&format!("{src}/")) {
                    return name.clone();
                }
            }
            String::new() // no manifest: single anonymous crate
        };
        let mut index = HashMap::new();
        // `src/lib.rs` and `src/main.rs` both map to `(crate, "crate")`.
        // Flattened repos can collide here too. Since insertion is first-wins,
        // the HashSet input must be ordered or cold processes choose different
        // roots and emit different module edges.
        let mut rust_files: Vec<_> = files.iter().filter(|f| f.ends_with(".rs")).collect();
        rust_files.sort();
        for f in rust_files {
            if let Some(mp) = file_to_mod_path(f) {
                index.entry((owner(f), mp)).or_insert_with(|| f.clone());
            }
        }
        RustCrates {
            name_to_src,
            roots,
            index,
            renames,
        }
    }

    fn owner(&self, file: &str) -> String {
        for (src, name) in &self.roots {
            if file == src || file.starts_with(&format!("{src}/")) {
                return name.clone();
            }
        }
        String::new()
    }

    fn lookup(&self, crate_name: &str, abs: &str) -> Option<String> {
        let segs: Vec<&str> = abs.split("::").collect();
        for len in (1..=segs.len()).rev() {
            let p = segs[..len].join("::");
            if let Some(f) = self.index.get(&(crate_name.to_string(), p)) {
                return Some(f.clone());
            }
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
            self.renames
                .get(&(owner.clone(), first.to_string()))
                .cloned()
        };
        if let Some(pkg) = real {
            let rest: Vec<&str> = segs.collect();
            let abs2 = if rest.is_empty() {
                "crate".to_string()
            } else {
                format!("crate::{}", rest.join("::"))
            };
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
    let Ok(val) = toml::from_str::<toml::Value>(content) else {
        return (None, Vec::new());
    };
    let name = val
        .get("package")
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
        .map(|s| s.to_string());
    let mut renames = Vec::new();
    for sec in ["dependencies", "dev-dependencies", "build-dependencies"] {
        let Some(deps) = val.get(sec).and_then(|d| d.as_table()) else {
            continue;
        };
        for (code, spec) in deps {
            if let Some(pkg) = spec
                .as_table()
                .and_then(|t| t.get("package"))
                .and_then(|p| p.as_str())
            {
                if pkg != code {
                    renames.push((code.clone(), pkg.to_string()));
                }
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
        if !path.ends_with("Cargo.toml") {
            continue;
        }
        let Ok(val) = toml::from_str::<toml::Value>(content) else {
            continue;
        };
        let Some(src) = val
            .get("package")
            .and_then(|p| p.get("name"))
            .and_then(|n| n.as_str())
            .map(|s| s.to_string())
        else {
            continue;
        };
        packages.insert(src.clone());
        for (sec, kind) in [
            ("dependencies", "dependencies"),
            ("dev-dependencies", "dev-dependencies"),
            ("build-dependencies", "build-dependencies"),
        ] {
            let Some(table) = val.get(sec).and_then(|d| d.as_table()) else {
                continue;
            };
            for (code, spec) in table {
                let dst = spec
                    .as_table()
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
        if packages.contains(&dst) && src != dst {
            out.insert(CrateEdge { src, dst, kind });
        }
    }
    out.into_iter().collect()
}
