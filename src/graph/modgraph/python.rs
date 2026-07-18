use super::*;

pub struct PyResolver;

/// Candidate absolute-import roots for a Python file set, longest-path-first
/// (deterministic tie-break: length, then lexicographic) — mirrors
/// `rspath::crate_roots`'s "derive roots from the scanned tree, no manifest
/// required" shape. Candidates: (a) the repo root itself (`""`); (b) a `src`
/// directory directly under the repo root, when any scanned file starts with
/// `src/` (src-layout); (c) the PARENT of every top-level package — a
/// directory containing `__init__.py` whose own parent does NOT also contain
/// one (so a nested sub-package doesn't ALSO offer its immediate parent as a
/// root; only the outermost package boundary does).
pub(crate) fn py_import_roots(files: &HashSet<String>) -> Vec<String> {
    let mut roots: HashSet<String> = HashSet::new();
    roots.insert(String::new());
    if files.iter().any(|f| f.starts_with("src/")) {
        roots.insert("src".to_string());
    }
    let init_dirs: HashSet<String> = files.iter()
        .filter(|f| f.ends_with("/__init__.py") || f.as_str() == "__init__.py")
        .map(|f| parent_dir(f))
        .collect();
    for dir in &init_dirs {
        let parent = parent_dir(dir);
        if !init_dirs.contains(&parent) {
            roots.insert(parent);
        }
    }
    let mut out: Vec<String> = roots.into_iter().collect();
    out.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    out
}

/// Cheap, never-followed `sys.path` mutation detector: a file containing
/// `sys.path.insert` or `sys.path.append` might enable imports that resolve
/// only at runtime — simulating that would mean interpreting Python, which
/// this diet extractor deliberately does not do. Counted instead, so the
/// caller can print ONE loud summary line per refresh explaining why some
/// imports in a root-discovery-defeating layout stay unresolved.
pub fn count_sys_path_mutators(
    files: &HashSet<String>,
    reader: &dyn Fn(&str) -> Option<String>,
) -> usize {
    files.iter()
        .filter(|f| f.ends_with(".py"))
        .filter_map(|f| reader(f))
        .filter(|content| content.contains("sys.path.insert") || content.contains("sys.path.append"))
        .count()
}

/// `import a.b.c[ as alias][, d.e[ as alias2]]*` — one or more comma-separated
/// dotted specifiers on one statement, each independently resolved (a plain
/// `import a.b, c.d` really does depend on two distinct modules).
fn py_import_re() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(
        r"(?m)^[ \t]*import[ \t]+([A-Za-z_][\w.]*(?:[ \t]+as[ \t]+[A-Za-z_]\w*)?(?:[ \t]*,[ \t]*[A-Za-z_][\w.]*(?:[ \t]+as[ \t]+[A-Za-z_]\w*)?)*)"
    ).unwrap())
}

/// `from <dots><module> import <names>` — group 1 is the leading dots (empty
/// for an absolute import), group 2 the optional dotted module name after
/// them, group 3 the name list: `*`, a parenthesized (possibly multi-line)
/// list, or the rest of the physical line.
fn py_from_import_re() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(
        r"(?m)^[ \t]*from[ \t]+(\.*)([A-Za-z_][\w.]*)?[ \t]+import[ \t]+(\*|\([^)]*\)|[^\n]+)"
    ).unwrap())
}

/// Split an import name list (`a, b as c, (d,\n e as f,\n)`) into (name, alias)
/// pairs. Parens (multi-line `from` form) are stripped first; a trailing comma
/// before the close-paren leaves one empty item, dropped.
fn parse_py_import_list(text: &str) -> Vec<(String, Option<String>)> {
    let inner = text.trim();
    let inner = inner.strip_prefix('(').and_then(|r| r.strip_suffix(')')).unwrap_or(inner);
    let mut out = Vec::new();
    for item in inner.split(',') {
        let item = item.trim();
        if item.is_empty() {
            continue;
        }
        match item.split_once(" as ") {
            Some((name, alias)) => out.push((name.trim().to_string(), Some(alias.trim().to_string()))),
            None => out.push((item.to_string(), None)),
        }
    }
    out
}

/// `#` line comments (never `//`/`/* */` in Python) and every string literal
/// (single/double, triple-quoted, any `r`/`b`/`f`/`u` prefix — prefixes don't
/// change the quote scan) blanked to spaces, newlines kept, so an `import`
/// inside a comment or a docstring never produces a phantom edge.
fn strip_python(src: &str) -> String {
    let b = src.as_bytes();
    let n = b.len();
    let mut out = Vec::with_capacity(n);
    let mut i = 0;
    let blank_to = |out: &mut Vec<u8>, from: usize, to: usize| {
        for k in from..to {
            out.push(if b[k] == b'\n' { b'\n' } else { b' ' });
        }
    };
    while i < n {
        let c = b[i];
        if c == b'#' {
            let s = i;
            while i < n && b[i] != b'\n' {
                i += 1;
            }
            blank_to(&mut out, s, i);
            continue;
        }
        if c == b'"' || c == b'\'' {
            let quote = c;
            let s = i;
            let triple = i + 2 < n && b[i + 1] == quote && b[i + 2] == quote;
            if triple {
                i += 3;
                while i + 2 < n && !(b[i] == quote && b[i + 1] == quote && b[i + 2] == quote) {
                    if b[i] == b'\\' { i += 2; continue; }
                    i += 1;
                }
                i = (i + 3).min(n);
            } else {
                i += 1;
                while i < n && b[i] != quote && b[i] != b'\n' {
                    if b[i] == b'\\' { i += 2; continue; }
                    i += 1;
                }
                if i < n && b[i] == quote {
                    i += 1;
                }
            }
            blank_to(&mut out, s, i);
            continue;
        }
        out.push(c);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| src.to_string())
}

/// `a.b.c` -> `a/b/c/__init__.py` or `a/b/c.py`, tried under every candidate
/// `root` (see `py_import_roots`). Resolving under exactly one root wins;
/// resolving under 2+ roots to DIFFERENT files stays Unresolved (ambiguous,
/// loud — never a silent guess at which root the runtime would have used). A
/// clean miss resolves Unresolved when the top-level segment is clearly part
/// of this project under SOME root (a same-named top-level file/dir exists),
/// else External (assume stdlib/third-party).
fn py_resolve_absolute(dotted: &str, files: &HashSet<String>, roots: &[String]) -> Resolution {
    let rel = dotted.replace('.', "/");
    let mut hits: Vec<String> = Vec::new();
    for root in roots {
        let base = if root.is_empty() { rel.clone() } else { format!("{root}/{rel}") };
        let init = format!("{base}/__init__.py");
        if files.contains(&init) {
            if !hits.contains(&init) { hits.push(init); }
            continue;
        }
        let modf = format!("{base}.py");
        if files.contains(&modf) && !hits.contains(&modf) {
            hits.push(modf);
        }
    }
    match hits.len() {
        1 => Resolution::File(hits.into_iter().next().unwrap()),
        n if n >= 2 => {
            Resolution::Unresolved(format!("{dotted}: ambiguous across {n} candidate import roots: {hits:?}"))
        }
        _ => {
            let top = dotted.split('.').next().unwrap_or(dotted);
            let known = roots.iter().any(|root| {
                let (top_dir, top_file) = if root.is_empty() {
                    (format!("{top}/"), format!("{top}.py"))
                } else {
                    (format!("{root}/{top}/"), format!("{root}/{top}.py"))
                };
                files.iter().any(|f| f == &top_file || f.starts_with(&top_dir))
            });
            if known {
                Resolution::Unresolved(format!("{dotted}: no matching module/package under any candidate import root"))
            } else {
                Resolution::External(dotted.to_string())
            }
        }
    }
}

/// A relative import resolves off the importing file's OWN package directory
/// (its parent dir — true for both a plain module and its package's
/// `__init__.py`): 1 dot = that directory itself, each further dot pops one
/// more directory level. Never External — a relative import always names
/// something inside this project, so an unresolved one is loud, not silent.
fn py_resolve_relative(dots: usize, dotted: &str, importing_file: &str, files: &HashSet<String>) -> Resolution {
    let spec_text = format!("{}{}", ".".repeat(dots), dotted);
    let base_dir = parent_dir(importing_file);
    let mut comps: Vec<&str> = if base_dir.is_empty() { vec![] } else { base_dir.split('/').collect() };
    let pops = dots.saturating_sub(1);
    if pops > comps.len() {
        return Resolution::Unresolved(format!("{spec_text}: relative import escapes the scanned tree"));
    }
    comps.truncate(comps.len() - pops);
    let mut path = comps.join("/");
    if !dotted.is_empty() {
        if !path.is_empty() {
            path.push('/');
        }
        path.push_str(&dotted.replace('.', "/"));
    }
    let init = if path.is_empty() { "__init__.py".to_string() } else { format!("{path}/__init__.py") };
    if files.contains(&init) {
        return Resolution::File(init);
    }
    if !path.is_empty() {
        let modf = format!("{path}.py");
        if files.contains(&modf) {
            return Resolution::File(modf);
        }
    }
    Resolution::Unresolved(format!("{spec_text}: unresolved relative import"))
}

impl ModuleResolver for PyResolver {
    fn exts(&self) -> &'static [&'static str] {
        &["py"]
    }

    fn edges(&self, file: &str, content: &str, cx: &ProjectCx) -> Vec<ModuleRef> {
        let clean = strip_python(content);
        let roots = cx.python_roots();
        let mut out = Vec::new();

        // `import a.b.c[ as alias][, ...]*`: each dotted name is its own
        // dependency, resolved and pushed independently.
        for c in py_import_re().captures_iter(&clean) {
            let list_span = c.get(1).unwrap();
            let line = line_of(&clean, c.get(0).unwrap().start());
            let span = Some((list_span.start() as u32, list_span.end() as u32));
            for (dotted, alias) in parse_py_import_list(list_span.as_str()) {
                let target = py_resolve_absolute(&dotted, cx.files, roots);
                let bindings = match (&alias, &target) {
                    (Some(a), Resolution::File(_)) => {
                        let source = dotted.rsplit('.').next().unwrap_or(&dotted);
                        vec![(a.clone(), source.to_string())]
                    }
                    _ => vec![],
                };
                // `import a.b.c` with no alias binds the TOP name (`a`) into
                // scope, not the leaf (`c`); an alias binds instead to the
                // full dotted target (its leaf name, matching `bindings`
                // above). Every resolution kind (not just File) so a library
                // import (`import numpy as np`) counts.
                let top = dotted.split('.').next().unwrap_or(&dotted).to_string();
                let module_bindings = match &alias {
                    Some(local) => {
                        let source = dotted.rsplit('.').next().unwrap_or(&dotted).to_string();
                        vec![(local.clone(), source, "named")]
                    }
                    None => vec![(top.clone(), top, "named")],
                };
                out.push(ModuleRef {
                    specifier: dotted,
                    kind: "import",
                    line,
                    span,
                    target,
                    bindings,
                    module_bindings,
                });
            }
        }

        // `from <dots><module> import <names>`: one specifier, one ModuleRef,
        // every named import's (local, source) binding attached to it.
        for c in py_from_import_re().captures_iter(&clean) {
            let dots = c.get(1).map(|m| m.as_str().len()).unwrap_or(0);
            let module = c.get(2).map(|m| m.as_str()).unwrap_or("");
            if dots == 0 && module.is_empty() {
                continue; // `from import x` is not valid Python; defensive skip
            }
            let names_text = c.get(3).map(|m| m.as_str()).unwrap_or("");
            let whole = c.get(0).unwrap();
            let line = line_of(&clean, whole.start());
            let span = Some((whole.start() as u32, whole.end() as u32));
            let spec = format!("{}{}", ".".repeat(dots), module);

            if names_text.trim() == "*" {
                out.push(ModuleRef {
                    specifier: format!("{spec}.*"),
                    kind: "import",
                    line,
                    span,
                    target: Resolution::Unresolved(format!("{spec}.*: star import not expanded")),
                    bindings: vec![],
                    module_bindings: vec![],
                });
                continue;
            }

            let target = if dots > 0 {
                py_resolve_relative(dots, module, file, cx.files)
            } else {
                py_resolve_absolute(module, cx.files, roots)
            };
            let names = parse_py_import_list(names_text);
            if names.is_empty() {
                continue;
            }
            let bindings: Vec<(String, String)> = if matches!(target, Resolution::File(_)) {
                names.iter().map(|(name, alias)| {
                    let local = alias.clone().unwrap_or_else(|| name.clone());
                    (local, name.clone())
                }).collect()
            } else {
                vec![]
            };
            // Every resolution kind (not just File): `from django.db import
            // models` is exactly the "which library" case module_binding
            // targets.
            let module_bindings: Vec<(String, String, &'static str)> = names.iter()
                .map(|(name, alias)| {
                    let local = alias.clone().unwrap_or_else(|| name.clone());
                    (local, name.clone(), "named")
                }).collect();
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
