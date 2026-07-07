//! Compile vendored tree-sitter grammar C sources into the crate.
//!
//! These two grammars have no usable crates.io dependency against our
//! tree-sitter 0.25 core:
//!   - dockerfile: only crate is 0.2.0, pins tree-sitter 0.20 (incompatible ABI).
//!   - go-template: no crate published at all.
//!
//! Both ship a generated `parser.c` (ABI 14/15, which 0.25 loads) plus the
//! `tree_sitter/` header dir. dockerfile also ships an external `scanner.c`.
//! We compile them here and declare/wrap the extern entry points in
//! src/engine.rs (ts_lang). Vendored sources: vendor/grammars/.

use std::path::Path;

fn main() {
    assert_vsix_version_parity();

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("vendor/grammars");

    // go-template: parser.c only, no external scanner.
    let gotmpl = root.join("gotmpl");
    let mut b = cc::Build::new();
    b.include(&gotmpl)
        .file(gotmpl.join("parser.c"))
        .flag_if_supported("-Wno-unused-parameter")
        .flag_if_supported("-Wno-unused-but-set-variable")
        .warnings(false)
        .compile("tree_sitter_gotmpl");
    println!("cargo:rerun-if-changed={}", gotmpl.join("parser.c").display());

    // dockerfile: parser.c + external scanner.c.
    let dockerfile = root.join("dockerfile");
    let mut b = cc::Build::new();
    b.include(&dockerfile)
        .file(dockerfile.join("parser.c"))
        .file(dockerfile.join("scanner.c"))
        .flag_if_supported("-Wno-unused-parameter")
        .flag_if_supported("-Wno-unused-but-set-variable")
        .warnings(false)
        .compile("tree_sitter_dockerfile");
    println!("cargo:rerun-if-changed={}", dockerfile.join("parser.c").display());
    println!("cargo:rerun-if-changed={}", dockerfile.join("scanner.c").display());

    embed_corpus(Path::new(env!("CARGO_MANIFEST_DIR")));
}

/// Embed the `.dl` corpus (examples + std libs) into the binary so `dl examples`
/// (list/search/show) and the embedded `use` fallback work from a prebuilt
/// download with no source tree. Generates `$OUT_DIR/embedded_corpus.rs` with
/// two `include_str!`-backed arrays; include_str! makes each file a rebuild dep.
fn embed_corpus(manifest: &Path) {
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("embedded_corpus.rs");
    let mut s = String::new();

    // examples/*.dl (flat) -> (filename, body)
    let mut ex: Vec<std::path::PathBuf> = std::fs::read_dir(manifest.join("examples"))
        .into_iter().flatten().filter_map(|e| e.ok()).map(|e| e.path())
        .filter(|p| p.extension().map_or(false, |x| x == "dl")).collect();
    ex.sort();
    s.push_str("pub static EMBEDDED_EXAMPLES: &[(&str, &str)] = &[\n");
    for p in &ex {
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        s.push_str(&format!("    ({:?}, include_str!({:?})),\n", name, p.display().to_string()));
    }
    s.push_str("];\n");
    println!("cargo:rerun-if-changed={}", manifest.join("examples").display());

    // std/**/*.dl (recursive) -> ("std/<rel>", body)
    let std_root = manifest.join("std");
    let mut std_files: Vec<(String, String)> = Vec::new();
    collect_dl(&std_root, &std_root, &mut std_files);
    std_files.sort();
    s.push_str("pub static EMBEDDED_STD: &[(&str, &str)] = &[\n");
    for (rel, abs) in &std_files {
        s.push_str(&format!("    ({:?}, include_str!({:?})),\n", format!("std/{rel}"), abs));
    }
    s.push_str("];\n");
    println!("cargo:rerun-if-changed={}", std_root.display());

    std::fs::write(&out, s).unwrap();
}

/// The `dl` binary embeds the VS Code extension VSIX (src/setup.rs,
/// include_bytes!), so the two ship as one version. Cargo.toml is the single
/// source; `scripts/build-vsix.sh` stamps the extension's package.json to it and
/// rebuilds `editors/vscode-dl/dl-lsp.vsix`. If the extension version drifts from
/// the crate version, REFUSE to compile — a drifted pair must never be released
/// (the loud twin of the `.dl/vsix-version-drift.dl` rail for a source build).
fn assert_vsix_version_parity() {
    let crate_ver = env!("CARGO_PKG_VERSION");
    let pkg = Path::new(env!("CARGO_MANIFEST_DIR")).join("editors/vscode-dl/package.json");
    println!("cargo:rerun-if-changed={}", pkg.display());
    let Ok(text) = std::fs::read_to_string(&pkg) else { return };
    // First top-level `"version": "X"` line (dependency ranges are keyed by
    // package name, so the version key appears once).
    let ext_ver = text.lines().find_map(|raw| {
        let rest = raw.trim().strip_prefix("\"version\"")?;
        let rest = rest.trim_start().strip_prefix(':')?.trim_start();
        rest.strip_prefix('"')?.split('"').next()
    });
    if let Some(found) = ext_ver {
        assert!(
            found == crate_ver,
            "VSIX version drift: editors/vscode-dl/package.json is {found} but the crate is \
             {crate_ver}. Run scripts/build-vsix.sh to rebuild dl-lsp.vsix at {crate_ver}, then \
             commit it."
        );
    }
}

/// Recursively collect `*.dl` under `dir`, pushing (path-relative-to-base, abspath).
fn collect_dl(dir: &Path, base: &Path, out: &mut Vec<(String, String)>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.filter_map(|e| e.ok()) {
        let p = e.path();
        if p.is_dir() {
            collect_dl(&p, base, out);
        } else if p.extension().map_or(false, |x| x == "dl") {
            let rel = p.strip_prefix(base).unwrap().to_string_lossy().into_owned();
            out.push((rel, p.display().to_string()));
        }
    }
}
