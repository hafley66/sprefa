use super::*;

// ── Go ──────────────────────────────────────────────────────────────────────
//
// Static resolution without the go tool. `go.mod`'s `module <path>` line is the
// import-path namespace root for the tree under it; every directory below that
// root is one package (Go's "one package per directory" rule), so an import
// resolves to the WHOLE set of `.go` files in its target directory — same
// wildcard-style fan-out as a Kotlin `import a.b.*`, just unconditional (Go has
// no per-symbol import). Aliased imports (`import f "some/path"`) carry a local
// binding whose source name is the import path's last segment (the package's
// conventional default name — Go does not require the alias to match the
// package's actual `package` clause, but the last path segment IS that
// convention in the overwhelming majority of real code, and this tier is
// syntax-only by design). Cross-module resolution (an import naming a
// DIFFERENT module than any `go.mod` in the file set) is a non-goal: it lands
// `External`, honestly, same as a stdlib import.
pub struct GoResolver;

pub(crate) struct GoIndex {
    /// (module import-path prefix, project-relative root directory) pairs,
    /// longest prefix first so a nested module's `go.mod` wins over an
    /// enclosing one for the same import path.
    modules: Vec<(String, String)>,
    /// project-relative directory (`""` for the repo/module root) -> its `.go`
    /// files, sorted for deterministic output.
    dirs: HashMap<String, Vec<String>>,
}

fn go_module_re() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?m)^[ \t]*module[ \t]+(\S+)").unwrap())
}

/// A single-line `import "path"` or `import alias "path"` (alias absent for a
/// plain import, `_`/`.` for a blank/dot import). Anchored to `^import` so an
/// arbitrary quoted string elsewhere in the file (a struct tag, a format
/// string) is never mistaken for one.
fn go_import_single_re() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?m)^[ \t]*import[ \t]+(?:(_|\.|\w+)[ \t]+)?"([^"]*)""#).unwrap()
    })
}

/// The parenthesized body of a grouped `import (...)` block. Go import blocks
/// never contain literal parens, so a non-nesting `[^()]*` is exact.
fn go_import_block_re() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?s)import[ \t]*\(([^()]*)\)").unwrap())
}

/// One import line INSIDE a block's body (see `go_import_block_re`): same shape
/// as the single-line form, applied per line of the block's inner text.
fn go_import_line_re() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"(?m)^[ \t]*(?:(_|\.|\w+)[ \t]+)?"([^"]*)""#).unwrap())
}

fn go_join_dir(root_dir: &str, rest: &str) -> String {
    match (root_dir.is_empty(), rest.is_empty()) {
        (true, _) => rest.to_string(),
        (false, true) => root_dir.to_string(),
        (false, false) => format!("{root_dir}/{rest}"),
    }
}

impl GoIndex {
    pub(crate) fn build(files: &HashSet<String>, manifests: &HashMap<String, String>) -> Self {
        let mut modules: Vec<(String, String)> = Vec::new();
        let mut sorted_manifests: Vec<_> = manifests.iter().collect();
        sorted_manifests.sort_by(|(ap, _), (bp, _)| ap.cmp(bp));
        for (path, content) in sorted_manifests {
            if !path.ends_with("go.mod") {
                continue;
            }
            let Some(m) = go_module_re().captures(content) else {
                continue;
            };
            modules.push((m[1].to_string(), parent_dir(path)));
        }
        modules.sort_by(|a, b| {
            b.0.len()
                .cmp(&a.0.len())
                .then_with(|| a.0.cmp(&b.0))
                .then_with(|| a.1.cmp(&b.1))
        });

        let mut dirs: HashMap<String, Vec<String>> = HashMap::new();
        for f in files {
            if !f.ends_with(".go") {
                continue;
            }
            dirs.entry(parent_dir(f)).or_default().push(f.clone());
        }
        for v in dirs.values_mut() {
            v.sort();
        }
        GoIndex { modules, dirs }
    }

    /// Longest-module-prefix resolution: the import path either equals a
    /// module's own root package or names a directory under it; either way the
    /// whole target directory's files resolve (Go's one-package-per-dir rule
    /// makes narrower, symbol-level resolution unnecessary). No module claims
    /// the prefix (a stdlib or third-party import, or a sibling module outside
    /// this file set) -> External.
    fn resolve(&self, spec: &str) -> Vec<Resolution> {
        for (module_path, root_dir) in &self.modules {
            let dir = if spec == module_path.as_str() {
                Some(root_dir.clone())
            } else {
                spec.strip_prefix(module_path.as_str())
                    .and_then(|rest| rest.strip_prefix('/'))
                    .map(|rest| go_join_dir(root_dir, rest))
            };
            let Some(dir) = dir else { continue };
            return match self.dirs.get(&dir) {
                Some(fs) if !fs.is_empty() => {
                    fs.iter().map(|f| Resolution::File(f.clone())).collect()
                }
                _ => vec![Resolution::Unresolved(format!(
                    "{spec}: no .go files in \"{dir}\""
                ))],
            };
        }
        vec![Resolution::External(spec.to_string())]
    }
}

/// Emit zero or more `ModuleRef` rows for one `import` occurrence: one File
/// row per file in the target directory (Go's whole-package fan-out), or one
/// Unresolved/External row. A blank (`spec` empty, a malformed capture) import
/// is skipped.
fn push_go_import(
    out: &mut Vec<ModuleRef>,
    idx: &GoIndex,
    clean: &str,
    alias: Option<&str>,
    spec: &str,
    start: usize,
    end: usize,
) {
    if spec.is_empty() {
        return;
    }
    let line = line_of(clean, start);
    let span = Some((start as u32, end as u32));
    let source = spec.rsplit('/').next().unwrap_or(spec).to_string();
    // `module_binding` superset (every resolution, not just File): `_` is
    // Go's blank/side-effect-only import, `.` merges the package's exports
    // unqualified (no single local name — same skip as a glob), an explicit
    // alias or the bare package name otherwise binds one local name.
    let module_bindings: Vec<(String, String, &'static str)> = match alias {
        Some("_") => vec![("".to_string(), "".to_string(), "side_effect")],
        Some(".") => vec![],
        Some(a) => vec![(a.to_string(), source.clone(), "named")],
        None => vec![(source.clone(), source.clone(), "named")],
    };
    for target in idx.resolve(spec) {
        let bindings = match (alias, &target) {
            (Some(a), Resolution::File(_)) if a != "_" && a != "." => {
                vec![(a.to_string(), source.clone())]
            }
            _ => vec![],
        };
        out.push(ModuleRef {
            specifier: spec.to_string(),
            kind: "import",
            line,
            span,
            target,
            bindings,
            module_bindings: module_bindings.clone(),
        });
    }
}

impl ModuleResolver for GoResolver {
    fn exts(&self) -> &'static [&'static str] {
        &["go"]
    }

    fn edges(&self, file: &str, content: &str, cx: &ProjectCx) -> Vec<ModuleRef> {
        let _ = file;
        // Go string literals ARE the specifiers (like TS), so keep their
        // content; only comments are blanked.
        let clean = strip_noise(content, false);
        let idx = cx.go_index();
        let mut out = Vec::new();
        // A single-line import inside the SAME file as a block needs no dedup
        // guard: `go_import_single_re` is anchored to a line starting
        // literally with `import`, and no line inside a `(...)` block does —
        // the two passes never double-count the same specifier.
        for c in go_import_block_re().captures_iter(&clean) {
            let inner = c.get(1).unwrap();
            let base = inner.start();
            for lc in go_import_line_re().captures_iter(inner.as_str()) {
                let alias = lc.get(1).map(|m| m.as_str());
                let spec_m = lc.get(2).unwrap();
                push_go_import(
                    &mut out,
                    idx,
                    &clean,
                    alias,
                    spec_m.as_str(),
                    base + spec_m.start(),
                    base + spec_m.end(),
                );
            }
        }
        for c in go_import_single_re().captures_iter(&clean) {
            let alias = c.get(1).map(|m| m.as_str());
            let spec_m = c.get(2).unwrap();
            push_go_import(
                &mut out,
                idx,
                &clean,
                alias,
                spec_m.as_str(),
                spec_m.start(),
                spec_m.end(),
            );
        }
        out
    }
}

// ── Python ──────────────────────────────────────────────────────────────────
//
// Purely path-based (no compiler, no content index needed beyond the import
// text itself). Python's real import roots are RUNTIME state (PYTHONPATH,
// `sys.path.insert`, `.pth` files) that a syntactic pass can't and shouldn't
// simulate, so an absolute `import a.b.c` / `from a.b.c import x` is tried
// under every CANDIDATE root discovered from the scanned file set (the
// `rspath::crate_roots` precedent — see `py_import_roots`): the repo root
// itself, any `src/` directly under it (src-layout), and the parent of every
// top-level package (a directory holding `__init__.py` whose own parent does
// not). Resolving under exactly ONE candidate root wins; resolving under two
// or more roots to DIFFERENT files stays Unresolved (loud, not a guess — the
// same honesty law as the module_binding_resolved alias hop). `sys.path` mutation is
// NEVER followed, only detected and counted (see `count_sys_path_mutators`).
// Relative imports (`from . import x`, `from .sub import y`) are UNAFFECTED
// by root discovery — they resolve off the IMPORTING FILE'S OWN package
// directory (its parent dir — true for both a regular module and its
// package's `__init__.py`), consuming one directory level per dot past the
// first (1 dot = the current package itself). A relative import that fails
// to resolve is always Unresolved (loud): by definition it can never be
// "external". An absolute import that fails under every root resolves
// External UNLESS its top-level segment is clearly part of this project's
// own tree (a same-named top-level file/dir exists under some root), in
// which case it's Unresolved (loud) — the same known-package-but-missing-
// target split Kotlin's resolver makes. `from m import *` is NEVER expanded
// (a stated non-goal): always Unresolved, even when `m` itself would
// resolve, so a star import never silently produces a wrong or absent edge.
// NON-GOAL: a `py_root(path)` user-fact seam (letting a program declare its
// own import roots) was considered and deferred — it would need the
// declared-sink treatment (like `repo`/`diag`), not a resolver-internal read.
