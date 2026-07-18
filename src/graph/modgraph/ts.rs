use super::*;

pub struct TsResolver {
    resolver: oxc_resolver::Resolver,
    root: std::path::PathBuf,
}

fn ts_spec_re() -> &'static Regex {
    // import/export ... from "..."  |  bare import "..."  |  import("...")  |  require("...")
    // quote class includes the backtick so a static template literal is caught.
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?m)(?:\bfrom|\bimport|\brequire)[ \t]*\(?[ \t]*['"`]([^'"`]+)['"`]"#)
            .unwrap()
    })
}

/// `import <clause> from "spec"` only — NOT a bare `import "spec"` (no clause,
/// nothing to alias), NOT `export ... from` (a re-export, not a local
/// binding), NOT `require(...)`. Group 1 is the clause text (everything
/// between `import` and `from`), group 2 the specifier text; group 2's span
/// lines up byte-for-byte with `ts_spec_re`'s group 1 span for the same
/// statement (both anchor on the `from '...'` tail), so the two regexes'
/// captures can be joined by that span without a second parse.
fn ts_import_clause_re() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?ms)\bimport\s+([^;'"`]*?)\s+from\s+['"`]([^'"`]+)['"`]"#).unwrap()
    })
}

/// Parse a TS/JS import clause (the text between `import` and `from`) into
/// (local, source) alias pairs: `import Default from ...` -> `[(Default,
/// "default")]`; `import { a as b } from ...` -> `[(b, a)]`; a plain named
/// import (`{ c }`) is skipped — its local name already equals the source, so
/// the name-keyed def bucket resolves it with no alias hop; a namespace
/// import (`* as ns`) is skipped (no member-level resolution). Leading
/// `type ` (type-only import) tokens are stripped. String-level, not oxc-grade
/// (Non-goal): an exotic clause this can't parse just yields fewer/no pairs,
/// never a wrong one.
pub(crate) fn parse_ts_import_clause(clause: &str) -> Vec<(String, String)> {
    let clause = clause.trim();
    let clause = clause.strip_prefix("type ").map(str::trim).unwrap_or(clause);
    let mut out = Vec::new();
    let (default_seg, named_part) = match (clause.find('{'), clause.find('}')) {
        (Some(open), Some(close)) if close > open => (&clause[..open], &clause[open + 1..close]),
        _ => (clause, ""),
    };
    let default_ident = default_seg.split(',').next().unwrap_or("").trim();
    if !default_ident.is_empty() && !default_ident.starts_with('*') {
        out.push((default_ident.to_string(), "default".to_string()));
    }
    for item in named_part.split(',') {
        let item = item.trim();
        if item.is_empty() {
            continue;
        }
        let item = item.strip_prefix("type ").map(str::trim).unwrap_or(item);
        if let Some((source, local)) = item.split_once(" as ") {
            out.push((local.trim().to_string(), source.trim().to_string()));
        }
        // plain named import: local == source, no alias hop needed, skip.
    }
    out
}

/// The `module_binding` superset of `parse_ts_import_clause`: EVERY local
/// name a clause introduces, kind-tagged, including the plain named import
/// and the namespace import that the alias-hop-only parser above skips (a
/// plain named local already equals its source name; a namespace import has
/// no member-level target) — both are real local bindings `module_binding`
/// needs to answer "which library does this local name come from". Same
/// string-level, non-goal-documented parse as `parse_ts_import_clause`.
pub(crate) fn parse_ts_module_bindings(clause: &str) -> Vec<(String, String, &'static str)> {
    let clause = clause.trim();
    let clause = clause.strip_prefix("type ").map(str::trim).unwrap_or(clause);
    let mut out = Vec::new();
    let (head, named_part) = match (clause.find('{'), clause.find('}')) {
        (Some(open), Some(close)) if close > open => (&clause[..open], &clause[open + 1..close]),
        _ => (clause, ""),
    };
    for seg in head.split(',') {
        let seg = seg.trim();
        if seg.is_empty() {
            continue;
        }
        if let Some(rest) = seg.strip_prefix('*') {
            if let Some(local) = rest.trim().strip_prefix("as ") {
                out.push((local.trim().to_string(), "*".to_string(), "namespace"));
            }
        } else {
            out.push((seg.to_string(), "default".to_string(), "default"));
        }
    }
    for item in named_part.split(',') {
        let item = item.trim();
        if item.is_empty() {
            continue;
        }
        let item = item.strip_prefix("type ").map(str::trim).unwrap_or(item);
        if let Some((source, local)) = item.split_once(" as ") {
            out.push((local.trim().to_string(), source.trim().to_string(), "named"));
        } else {
            out.push((item.to_string(), item.to_string(), "named"));
        }
    }
    out
}

impl TsResolver {
    pub(crate) fn new(root: &Path) -> Option<Self> {
        use oxc_resolver::{ResolveOptions, Resolver, TsconfigDiscovery};
        let options = ResolveOptions {
            extensions: [
                ".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".mts", ".cts", ".json",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
            main_fields: vec!["module".into(), "main".into()],
            condition_names: vec!["import".into(), "require".into(), "default".into()],
            // Auto = discover the nearest tsconfig.json per importing file (monorepo).
            tsconfig: Some(TsconfigDiscovery::Auto),
            ..ResolveOptions::default()
        };
        Some(TsResolver {
            resolver: Resolver::new(options),
            root: root.to_path_buf(),
        })
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
        let stem = if sub.is_empty() {
            dir.clone()
        } else {
            format!("{dir}/{sub}")
        };
        for cand in [
            stem.clone(),
            format!("{stem}.ts"),
            format!("{stem}.tsx"),
            format!("{stem}.js"),
            format!("{stem}/index.ts"),
            format!("{stem}/index.tsx"),
            format!("{stem}/index.js"),
            format!("{dir}/index.ts"),
            format!("{dir}/index.tsx"),
            format!("{dir}/index.js"),
        ] {
            if cx.files.contains(&cand) {
                return Some(cand);
            }
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
        // (spec-text span) -> its clause's alias bindings, from a SEPARATE
        // regex pass over `clean` matching only `import <clause> from
        // "spec"` (the main loop below also matches bare/require/export-from
        // forms via `ts_spec_re`, which carry no clause to alias). The two
        // regexes' spec-text spans line up byte-for-byte for the same
        // statement (see `ts_import_clause_re`'s doc), so this map joins by
        // span with no second parse.
        let mut clause_bindings: HashMap<(u32, u32), Vec<(String, String)>> = HashMap::new();
        // Same span-join, the `module_binding` superset (plain named +
        // namespace included; see `parse_ts_module_bindings`).
        let mut clause_module_bindings: HashMap<(u32, u32), Vec<(String, String, &'static str)>> = HashMap::new();
        for c in ts_import_clause_re().captures_iter(&clean) {
            let clause = c.get(1).map(|m| m.as_str()).unwrap_or("");
            let spec_span = c.get(2).unwrap();
            let bindings = parse_ts_import_clause(clause);
            if !bindings.is_empty() {
                clause_bindings.insert((spec_span.start() as u32, spec_span.end() as u32), bindings);
            }
            let module_bindings = parse_ts_module_bindings(clause);
            if !module_bindings.is_empty() {
                clause_module_bindings.insert((spec_span.start() as u32, spec_span.end() as u32), module_bindings);
            }
        }
        for c in ts_spec_re().captures_iter(&clean) {
            let spec = c[1].to_string();
            let line = line_of(&clean, c.get(0).unwrap().start());
            let m = c.get(1).unwrap();
            let span = Some((m.start() as u32, m.end() as u32));
            // Which alternative of `ts_spec_re` matched (`from` | `import` |
            // `require`), so a bare `import "spec";` (no `from`, no clause)
            // can be told apart from a `require(...)` or `export ... from`
            // statement (neither of which binds a local name statically here).
            let matched_import_kw = c.get(0).unwrap().as_str().trim_start().starts_with("import");
            // a template literal with interpolation cannot be resolved statically
            if spec.contains("${") {
                out.push(ModuleRef {
                    specifier: spec.clone(),
                    kind: "import",
                    line,
                    span: None,
                    target: Resolution::Unresolved(format!("{spec}: dynamic")),
                    bindings: vec![],
                    module_bindings: vec![],
                });
                continue;
            }
            // resolve_file (not resolve) so TsconfigDiscovery::Auto finds the nearest
            // tsconfig.json walking up from the importing file (per-package monorepo).
            let target = match self.resolver.resolve_file(&abs, &spec) {
                Ok(r) => {
                    let full = r.full_path();
                    match full
                        .strip_prefix(&self.root)
                        .ok()
                        .map(|p| p.to_string_lossy().replace('\\', "/"))
                        .filter(|rel| cx.files.contains(rel))
                    {
                        Some(rel) => Resolution::File(rel),
                        None => Resolution::External(spec.clone()),
                    }
                }
                Err(_) => match self.workspace_fallback(&spec, cx) {
                    Some(f) => Resolution::File(f),
                    None if spec.starts_with('.') => {
                        Resolution::Unresolved(format!("{spec}: unresolved"))
                    }
                    None => Resolution::External(spec.clone()),
                },
            };
            let bindings = clause_bindings
                .get(&(m.start() as u32, m.end() as u32))
                .cloned()
                .unwrap_or_default();
            let module_bindings = match clause_module_bindings.get(&(m.start() as u32, m.end() as u32)) {
                Some(rows) => rows.clone(),
                // No clause matched this spec span: a genuine bare/side-effect
                // import (`import "spec";`) binds no name but the import still
                // happened, so record it with an empty local/imported name;
                // `require(...)`/`export ... from` (matched_import_kw false)
                // bind no name this parser can see, so no row at all.
                None if matched_import_kw => vec![("".to_string(), "".to_string(), "side_effect")],
                None => vec![],
            };
            out.push(ModuleRef {
                specifier: spec,
                kind: "import",
                line,
                span,
                target,
                bindings,
                module_bindings,
            });
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
// package. expect/actual twins share a decl key, so a decl maps to ALL its
// declaring files and an import fans an edge to each (wildcard-style).
// Same-package references need no import: a word-boundary hit of another
// file's column-0 decl name emits a kind="same-package" edge — by design any
// such match counts, so a name in a comment is a (loud, inspectable) false
// positive. Compiler-exact graphs come from scip-kotlin via the SCIP importer
// instead.

