use super::*;

pub struct KotlinResolver;

pub(crate) struct KotlinIndex {
    /// package name -> .kt files declaring it (sorted, deterministic).
    packages: HashMap<String, Vec<String>>,
    /// (package, top-level decl name) -> ALL defining files (sorted; more than
    /// one means expect/actual twins or a redeclaration).
    decls: HashMap<(String, String), Vec<String>>,
}

pub(crate) fn kotlin_package_re() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?m)^[ \t]*package[ \t]+([\w.`]+)").unwrap())
}

fn kotlin_import_re() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(
        r"(?m)^[ \t]*import[ \t]+([\w`]+(?:\.[\w`]+)*(?:\.\*)?)(?:[ \t]+as[ \t]+([\w`]+))?").unwrap())
}

/// Column-0 declarations only — Kotlin convention indents members, so a line
/// that starts at column 0 with modifiers + a declaring keyword is top-level.
/// `fun interface Name` comes before bare `fun` in the alternation; an optional
/// generic param list and extension receiver sit between `fun` and the name.
pub(crate) fn kotlin_decl_re() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(
        r"(?m)^(?:(?:public|internal|private|protected|open|abstract|final|sealed|data|inline|value|annotation|enum|expect|actual|external|suspend|operator|infix|tailrec|const)[ \t]+)*(?:fun[ \t]+interface|class|interface|object|fun|val|var|typealias)[ \t]+(?:<[^>\n]*>[ \t]*)?(?:[\w.<>?]+\.)?(`[^`\n]+`|\w+)").unwrap())
}

/// Blank `"""…"""` bodies (newlines kept) so a raw string containing `import`
/// lines never reaches the import scan; then the shared rust-mode strip handles
/// `//`, nested `/* */`, ordinary strings, and chars — all valid for Kotlin.
pub(crate) fn strip_kotlin(src: &str) -> String {
    let b = src.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'"' && b.get(i + 1) == Some(&b'"') && b.get(i + 2) == Some(&b'"') {
            let mut j = i + 3;
            while j < b.len()
                && !(b[j] == b'"' && b.get(j + 1) == Some(&b'"') && b.get(j + 2) == Some(&b'"'))
            {
                j += 1;
            }
            let end = (j + 3).min(b.len());
            for k in i..end {
                out.push(if b[k] == b'\n' { b'\n' } else { b' ' });
            }
            i = end;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    strip_noise(
        &String::from_utf8(out).unwrap_or_else(|_| src.to_string()),
        true,
    )
}

impl KotlinIndex {
    pub(crate) fn build(
        files: &HashSet<String>,
        reader: Option<&(dyn Fn(&str) -> Option<String> + Send + Sync)>,
    ) -> Self {
        let mut packages: HashMap<String, Vec<String>> = HashMap::new();
        let mut decls = HashMap::new();
        let Some(read) = reader else {
            return KotlinIndex { packages, decls };
        };
        let mut kt: Vec<&String> = files
            .iter()
            .filter(|f| f.ends_with(".kt") || f.ends_with(".kts"))
            .collect();
        kt.sort();
        for f in kt {
            let Some(content) = read(f) else { continue };
            let clean = strip_kotlin(&content);
            let pkg = kotlin_package_re()
                .captures(&clean)
                .map(|c| c[1].replace('`', ""))
                .unwrap_or_default();
            packages.entry(pkg.clone()).or_default().push(f.clone());
            for c in kotlin_decl_re().captures_iter(&clean) {
                let name = c[1].trim_matches('`').to_string();
                let files = decls.entry((pkg.clone(), name)).or_insert_with(Vec::new);
                if !files.contains(f) {
                    files.push(f.clone());
                }
            }
        }
        KotlinIndex { packages, decls }
    }

    /// Longest-package-prefix resolution: first prefix split whose (package,
    /// next segment) is a known decl wins (handles `a.b.Outer.Inner`) and
    /// yields one File per declaring file (expect/actual twins fan out); else a
    /// known package with no such decl is Unresolved; else External (a jar).
    fn resolve(&self, spec: &str) -> Vec<Resolution> {
        let segs: Vec<&str> = spec.split('.').collect();
        for len in (1..segs.len()).rev() {
            let pkg = segs[..len].join(".");
            if let Some(fs) = self.decls.get(&(pkg, segs[len].to_string())) {
                return fs.iter().map(|f| Resolution::File(f.clone())).collect();
            }
        }
        for len in (1..segs.len()).rev() {
            if self.packages.contains_key(&segs[..len].join(".")) {
                return vec![Resolution::Unresolved(format!(
                    "{spec}: no column-0 decl `{}` in package {}",
                    segs[len],
                    segs[..len].join(".")
                ))];
            }
        }
        vec![Resolution::External(spec.to_string())]
    }
}

/// First word-boundary occurrence of `name` in `text` (identifier chars on
/// neither side), or None. The same-package scan's matcher — substring find +
/// boundary check, no per-name regex compile.
fn word_boundary_find(text: &str, name: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let is_ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let mut from = 0;
    while let Some(off) = text[from..].find(name) {
        let start = from + off;
        let end = start + name.len();
        let left_ok = start == 0 || !is_ident(bytes[start - 1]);
        let right_ok = end >= bytes.len() || !is_ident(bytes[end]);
        if left_ok && right_ok {
            return Some(start);
        }
        from = start + 1;
    }
    None
}

impl ModuleResolver for KotlinResolver {
    fn exts(&self) -> &'static [&'static str] {
        &["kt", "kts"]
    }

    fn edges(&self, file: &str, content: &str, cx: &ProjectCx) -> Vec<ModuleRef> {
        let clean = strip_kotlin(content);
        let idx = cx.kotlin_index();
        let mut out = Vec::new();
        for c in kotlin_import_re().captures_iter(&clean) {
            let m = c.get(1).unwrap();
            let spec = m.as_str().replace('`', "");
            let line = line_of(&clean, c.get(0).unwrap().start());
            let span = Some((m.start() as u32, m.end() as u32));
            // `import a.b.C as D` — never on a wildcard/same-package ref.
            let alias = c.get(2).map(|a| a.as_str().replace('`', ""));
            if let Some(pkg) = spec.strip_suffix(".*") {
                match idx.packages.get(pkg) {
                    // a wildcard import depends on every file of the package
                    Some(fs) => {
                        for f in fs.iter().filter(|f| f.as_str() != file) {
                            out.push(ModuleRef {
                                specifier: spec.clone(),
                                kind: "import",
                                line,
                                span,
                                target: Resolution::File(f.clone()),
                                bindings: vec![],
                                // wildcard: no single local name (see the
                                // `ModuleRef::module_bindings` doc).
                                module_bindings: vec![],
                            });
                        }
                    }
                    None => out.push(ModuleRef {
                        specifier: spec.clone(),
                        kind: "import",
                        line,
                        span,
                        target: Resolution::External(spec.clone()),
                        bindings: vec![],
                        module_bindings: vec![],
                    }),
                }
                continue;
            }
            for target in idx.resolve(&spec) {
                let bindings = match (&alias, &target) {
                    (Some(alias), Resolution::File(_)) => {
                        let source = spec.rsplit('.').next().unwrap_or(&spec);
                        vec![(alias.clone(), source.to_string())]
                    }
                    _ => vec![],
                };
                // Unlike `bindings` above (File-target only), `module_binding`
                // needs every resolution kind — a library import resolves
                // External and is exactly the case a mapping query cares about.
                let source = spec.rsplit('.').next().unwrap_or(&spec).to_string();
                let local = alias.clone().unwrap_or_else(|| source.clone());
                let module_bindings = vec![(local, source, "named")];
                out.push(ModuleRef {
                    specifier: spec.clone(),
                    kind: "import",
                    line,
                    span,
                    target,
                    bindings,
                    module_bindings,
                });
            }
        }

        // Same-package implicit refs: another file's column-0 decl name used
        // with no import. A name this file also declares is skipped (local
        // wins, expect/actual twins would self-match every keyword hit).
        let pkg = kotlin_package_re()
            .captures(&clean)
            .map(|c| c[1].replace('`', ""))
            .unwrap_or_default();
        let mut hits: Vec<(&str, usize, &Vec<String>)> = idx
            .decls
            .iter()
            .filter(|((p, _), fs)| *p == pkg && !fs.iter().any(|f| f == file))
            .filter_map(|((_, name), fs)| {
                word_boundary_find(&clean, name).map(|pos| (name.as_str(), pos, fs))
            })
            .collect();
        hits.sort();
        for (name, pos, fs) in hits {
            let line = line_of(&clean, pos);
            for f in fs.iter().filter(|f| f.as_str() != file) {
                out.push(ModuleRef {
                    specifier: name.to_string(),
                    kind: "same-package",
                    line,
                    span: None,
                    target: Resolution::File(f.clone()),
                    bindings: vec![],
                    module_bindings: vec![],
                });
            }
        }
        out
    }
}

