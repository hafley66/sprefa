//! Proof of the cross-language module graph (modgraph.rs): the built-in
//! `module_edge`/`module_import`/`module_unresolved` relations are populated by
//! the Rust resolver from `mod`/`use` decls, and `reaches(a,b) <- closure(module_edge)`
//! gives transitive file-to-file reach. "The filesystem from language."

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("modgraph_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(dir: &Path, prog: &str, extra: &[&str]) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--root", dir.to_str().unwrap(), "--db", dir.join("db").to_str().unwrap()])
        .args(extra)
        .output().expect("run dl");
    (out.status.code().unwrap_or(-1),
     String::from_utf8_lossy(&out.stdout).into_owned(),
     String::from_utf8_lossy(&out.stderr).into_owned())
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .expect("git");
    assert!(out.status.success(), "git {:?} failed:\nstdout={}\nstderr={}",
        args, String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
}

// A scan rule populates the file set (built-in `file`); the resolver then runs
// over it. Module relations are unpopulated unless the program references one.
const PROG: &str = r#"
rel seen(path: file).
seen(path) <- scan("WORK", "src/**/*.rs", path, rev), sg(path, rev, :rust, "fn $N() {}", line).
rel reaches(a: text, b: text).
reaches(a, b) <- closure(module_edge).
? module_edge(s, d).
? reaches(a, b).
? module_unresolved(f, spec, why, ln).
"#;

#[test]
fn rust_mod_and_use_edges_with_closure() {
    let d = sandbox("rust");
    fs::create_dir_all(d.join("src")).unwrap();
    // lib.rs declares two submodules; a.rs uses b; b.rs is a leaf; mod missing has no file.
    fs::write(d.join("src/lib.rs"), "mod a;\nmod b;\nmod missing;\nfn x() {}\n").unwrap();
    fs::write(d.join("src/a.rs"), "use crate::b::Thing;\nfn x() {}\n").unwrap();
    fs::write(d.join("src/b.rs"), "pub struct Thing;\nfn x() {}\n").unwrap();

    let (code, out, _) = run(&d, PROG, &[]);
    assert_eq!(code, 0, "run failed: {out}");

    // mod edges (filesystem inclusion)
    assert!(out.contains("src/lib.rs\tsrc/a.rs"), "lib->a mod edge: {out}");
    assert!(out.contains("src/lib.rs\tsrc/b.rs"), "lib->b mod edge: {out}");
    // use edge (cross-module dependency)
    assert!(out.contains("src/a.rs\tsrc/b.rs"), "a->b use edge: {out}");

    // transitive reach: lib -> a -> b means reaches(lib, b) holds
    let reaches_block = out.split("? reaches").nth(1).unwrap_or("");
    assert!(reaches_block.contains("src/lib.rs\tsrc/b.rs"), "transitive reaches(lib,b): {out}");
    assert!(reaches_block.contains("src/a.rs\tsrc/b.rs"), "reaches(a,b): {out}");

    // `mod missing;` has no child file -> module_unresolved
    let unres_block = out.split("? module_unresolved").nth(1).unwrap_or("");
    assert!(unres_block.contains("missing"), "mod missing must be unresolved: {out}");
}

#[test]
fn use_paths_are_located_in_ref_spine() {
    // The import graph feeds `_where_bytes`: a `use` leaf becomes a `ref` row whose
    // `string` is the contiguous path text and whose (lo,hi) is its byte span in the
    // file — the rewrite coordinate an `edit`/`--move` keys off.
    let d = sandbox("usespan");
    fs::create_dir_all(d.join("src")).unwrap();
    // Two leaves under one brace: each gets its own span; the head `crate::b` is shared.
    fs::write(d.join("src/lib.rs"), "mod a;\nmod b;\nfn x() {}\n").unwrap();
    fs::write(d.join("src/a.rs"), "use crate::b::{Thing, Other};\nfn x() {}\n").unwrap();
    fs::write(d.join("src/b.rs"), "pub struct Thing;\npub struct Other;\nfn x() {}\n").unwrap();

    // Reference module_edge (drives the resolver) AND ref/string (drives the spine).
    let prog = r#"
rel seen(path: file).
seen(path) <- scan("WORK", "src/**/*.rs", path, rev), sg(path, rev, :rust, "fn $N() {}", line).
rel reaches(a: text, b: text).
reaches(a, b) <- closure(module_edge).
rel use_at(text: text, lo: int, hi: int).
use_at(txt, lo, hi) <- ref(_, s, _, lo, hi), string(s, txt, _).
? use_at(txt, lo, hi).
"#;
    let (code, out, _) = run(&d, prog, &[]);
    assert_eq!(code, 0, "run failed: {out}");
    let block = out.split("? use_at").nth(1).unwrap_or("");
    // Brace expansion synthesizes `crate::b::Thing`/`crate::b::Other`; each span
    // points at the contiguous leaf text in `use crate::b::{Thing, Other};` —
    // `Thing` at bytes 15..20, `Other` at 22..27. That byte range IS the rewrite
    // coordinate `edit`/`--move` keys off. (`ref.file` is the content-addressed
    // FileId, not the path, so the span is what we assert on.)
    assert!(block.contains("Thing\t15\t20"), "Thing leaf span: {out}");
    assert!(block.contains("Other\t22\t27"), "Other leaf span: {out}");
}

#[test]
fn unreferenced_program_does_not_populate_module_rels() {
    // A program that never mentions a module relation pays nothing: the resolver
    // pass is skipped. Verified indirectly — querying file works, no module rows.
    let d = sandbox("lazy");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/lib.rs"), "mod a;\nfn x() {}\n").unwrap();
    fs::write(d.join("src/a.rs"), "fn x() {}\n").unwrap();
    let prog = r#"
rel seen(path: file).
seen(path) <- scan("WORK", "src/**/*.rs", path, rev), sg(path, rev, :rust, "fn $N() {}", line).
? seen(p).
"#;
    let (code, out, _) = run(&d, prog, &[]);
    assert_eq!(code, 0);
    assert!(out.contains("(2 rows)"), "both files seen: {out}");
}

// Scans both languages into one file set; one `reaches` closure spans the union.
const PROG_XLANG: &str = r#"
rel seen_rs(path: file).
rel seen_ts(path: file).
seen_rs(path) <- scan("WORK", "**/*.rs", path, rev), sg(path, rev, :rust, "fn $N() {}", line).
seen_ts(path) <- scan("WORK", "**/*.ts", path, rev), match(path, rev, /export/, line).
rel reaches(a: text, b: text).
reaches(a, b) <- closure(module_edge).
? module_edge(s, d).
? reaches(a, b).
"#;

#[test]
fn ts_import_edges_via_oxc() {
    let d = sandbox("ts");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/app.ts"), "import './utils';\nimport { y } from './lib/helper';\nexport const z = 1;\n").unwrap();
    fs::write(d.join("src/utils.ts"), "export const x = 1;\n").unwrap();
    fs::create_dir_all(d.join("src/lib")).unwrap();
    fs::write(d.join("src/lib/helper.ts"), "export const y = 2;\n").unwrap();

    let (code, out, _) = run(&d, PROG_XLANG, &[]);
    assert_eq!(code, 0, "run failed: {out}");
    // relative import with extension probing
    assert!(out.contains("src/app.ts\tsrc/utils.ts"), "app->utils import edge: {out}");
    // nested relative import
    assert!(out.contains("src/app.ts\tsrc/lib/helper.ts"), "app->lib/helper edge: {out}");
}

#[test]
fn cross_language_union_in_one_edge_relation() {
    let d = sandbox("xlang");
    fs::create_dir_all(d.join("rs/src")).unwrap();
    fs::create_dir_all(d.join("ts/src")).unwrap();
    // Rust subgraph: lib -> a
    fs::write(d.join("rs/src/lib.rs"), "mod a;\nfn x() {}\n").unwrap();
    fs::write(d.join("rs/src/a.rs"), "fn x() {}\n").unwrap();
    // TS subgraph: app -> util
    fs::write(d.join("ts/src/app.ts"), "import './util';\nexport const z = 1;\n").unwrap();
    fs::write(d.join("ts/src/util.ts"), "export const x = 1;\n").unwrap();

    let (code, out, _) = run(&d, PROG_XLANG, &[]);
    assert_eq!(code, 0, "run failed: {out}");
    // Both languages' edges live in the same module_edge relation
    assert!(out.contains("rs/src/lib.rs\trs/src/a.rs"), "rust edge present: {out}");
    assert!(out.contains("ts/src/app.ts\tts/src/util.ts"), "ts edge present: {out}");
    // The single reaches closure covers both subgraphs
    let reaches_block = out.split("? reaches").nth(1).unwrap_or("");
    assert!(reaches_block.contains("rs/src/lib.rs\trs/src/a.rs"), "rust reach: {out}");
    assert!(reaches_block.contains("ts/src/app.ts\tts/src/util.ts"), "ts reach: {out}");
}

const PROG_TS: &str = r#"
rel seen(path: file).
seen(path) <- scan("WORK", "**/*.ts", path, rev), match(path, rev, /./, line).
rel reaches(a: text, b: text).
reaches(a, b) <- closure(module_edge).
? module_edge(s, d).
? module_unresolved(f, spec, why, ln).
"#;

#[test]
fn ts_per_package_tsconfig_paths() {
    let d = sandbox("ts_tsconfig");
    fs::create_dir_all(d.join("packages/app/src")).unwrap();
    fs::create_dir_all(d.join("packages/lib/src")).unwrap();
    // app's own tsconfig defines a @lib/* alias (baseUrl = the packages dir).
    fs::write(d.join("packages/app/tsconfig.json"),
        r#"{"compilerOptions":{"baseUrl":"..","paths":{"@lib/*":["lib/src/*"]}}}"#).unwrap();
    fs::write(d.join("packages/app/src/main.ts"), "import { u } from '@lib/util';\nexport const z = 1;\n").unwrap();
    fs::write(d.join("packages/lib/src/util.ts"), "export const u = 1;\n").unwrap();

    let (code, out, _) = run(&d, PROG_TS, &[]);
    assert_eq!(code, 0, "run failed: {out}");
    assert!(out.contains("packages/app/src/main.ts\tpackages/lib/src/util.ts"),
        "per-package tsconfig paths alias must resolve: {out}");
}

#[test]
fn ts_workspace_package_json_fallback() {
    let d = sandbox("ts_workspace");
    fs::create_dir_all(d.join("packages/app/src")).unwrap();
    fs::create_dir_all(d.join("packages/shared")).unwrap();
    // No node_modules: a bare import of a workspace package resolves via package.json name.
    fs::write(d.join("packages/shared/package.json"), r#"{"name":"shared","version":"1.0.0"}"#).unwrap();
    fs::write(d.join("packages/shared/index.ts"), "export const s = 1;\n").unwrap();
    fs::write(d.join("packages/app/src/main.ts"), "import { s } from 'shared';\nexport const z = 1;\n").unwrap();

    let (code, out, _) = run(&d, PROG_TS, &[]);
    assert_eq!(code, 0, "run failed: {out}");
    assert!(out.contains("packages/app/src/main.ts\tpackages/shared/index.ts"),
        "workspace package.json fallback must resolve: {out}");
}

#[test]
fn ts_dynamic_template_import_is_unresolved_not_silent() {
    let d = sandbox("ts_dyn");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/app.ts"), "const m = await import(`./mods/${name}`);\nexport const z = 1;\n").unwrap();
    let (code, out, _) = run(&d, PROG_TS, &[]);
    assert_eq!(code, 0, "run failed: {out}");
    let unres = out.split("? module_unresolved").nth(1).unwrap_or("");
    assert!(unres.contains("dynamic"), "interpolated import() must be flagged dynamic, not dropped silently: {out}");
}

#[test]
fn broken_imports_lint_via_check() {
    // The module graph as a linter: unresolved refs become `error` diagnostics, so
    // `--check` exits non-zero and names the offending file:line.
    let d = sandbox("lint");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/lib.rs"), "mod real;\nmod ghost;\nfn x(){}\n").unwrap();
    fs::write(d.join("src/real.rs"), "fn x(){}\n").unwrap();
    fs::write(d.join("src/app.ts"), "import './missing';\nimport './present';\nexport const z=1;\n").unwrap();
    fs::write(d.join("src/present.ts"), "export const p=1;\n").unwrap();
    let prog = r#"
rel seen(path: file).
seen(path) <- scan("WORK", "**/*.{rs,ts}", path, rev), match(path, rev, /./, line).
rel diag(path: text, line: int, severity: text, code: text, msg: text).
diag(p, l, "error", "broken-import", "unresolved `${spec}`") <- module_unresolved(p, spec, reason, l).
"#;
    let (code, _out, err) = run(&d, prog, &["--check"]);
    assert_ne!(code, 0, "broken imports must fail --check");
    assert!(err.contains("src/lib.rs:2") && err.contains("ghost"), "dangling mod ghost flagged at its line: {err}");
    assert!(err.contains("src/app.ts:1") && err.contains("missing"), "broken TS import flagged at its line: {err}");
    assert!(!err.contains("real") && !err.contains("present"), "resolved imports must not be flagged: {err}");
}

#[test]
fn edge_drops_when_target_file_deleted() {
    let d = sandbox("drop");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/lib.rs"), "mod a;\nfn x() {}\n").unwrap();
    fs::write(d.join("src/a.rs"), "fn x() {}\n").unwrap();

    let (_, out, _) = run(&d, PROG, &[]);
    assert!(out.contains("src/lib.rs\tsrc/a.rs"), "cold: lib->a edge: {out}");

    // Delete the submodule file and tick that path. The mod decl in lib.rs still
    // exists but now resolves to nothing -> edge drops, becomes unresolved.
    fs::remove_file(d.join("src/a.rs")).unwrap();
    let _ = run(&d, PROG, &["--changed", "src/a.rs"]);
    let (_, out2, _) = run(&d, PROG, &[]);
    assert!(!out2.contains("src/lib.rs\tsrc/a.rs"), "edge must drop when a.rs gone: {out2}");
}

#[test]
fn module_edge_rev_keeps_head_and_work_separate() {
    let d = sandbox("rev");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/lib.rs"), "mod a;\nfn x() {}\n").unwrap();
    fs::write(d.join("src/a.rs"), "fn x() {}\n").unwrap();
    git(&d, &["init"]);
    git(&d, &["config", "user.email", "test@example.com"]);
    git(&d, &["config", "user.name", "Test User"]);
    git(&d, &["add", "."]);
    git(&d, &["commit", "-m", "head"]);

    fs::write(d.join("src/lib.rs"), "mod b;\nfn x() {}\n").unwrap();
    fs::remove_file(d.join("src/a.rs")).unwrap();
    fs::write(d.join("src/b.rs"), "fn x() {}\n").unwrap();

    let prog = r#"
rel seen(path: file, rev: text).
seen(path, rev) <- scan("HEAD", "src/**/*.rs", path, rev), match(path, rev, /./, line).
seen(path, rev) <- scan("WORK", "src/**/*.rs", path, rev), match(path, rev, /./, line).
rel work_edge(src: text, dst: text).
work_edge(src, dst) <- module_edge_rev(src, dst, "WORK").
rel work_reaches(src: text, dst: text).
work_reaches(src, dst) <- closure(work_edge).
? module_edge_rev(s, d, r).
? work_reaches(s, d).
"#;
    let (code, out, err) = run(&d, prog, &[]);
    assert_eq!(code, 0, "run failed:\nstdout={out}\nstderr={err}");
    assert!(out.contains("src/lib.rs\tsrc/a.rs\t"), "HEAD edge keeps its rev: {out}");
    assert!(out.contains("src/lib.rs\tsrc/b.rs\tWORK"), "WORK edge keeps WORK rev: {out}");
    let work = out.split("? work_reaches").nth(1).unwrap_or("");
    assert!(work.contains("src/lib.rs\tsrc/b.rs"), "filtered WORK closure includes b: {out}");
    assert!(!work.contains("src/lib.rs\tsrc/a.rs"), "filtered WORK closure must not include HEAD-only a: {out}");
}

#[test]
fn changed_work_module_source_preserves_other_revs() {
    let d = sandbox("rev_delta");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/lib.rs"), "mod a;\nfn x() {}\n").unwrap();
    fs::write(d.join("src/a.rs"), "fn x() {}\n").unwrap();
    fs::write(d.join("src/b.rs"), "fn x() {}\n").unwrap();
    fs::write(d.join("src/c.rs"), "fn x() {}\n").unwrap();
    git(&d, &["init"]);
    git(&d, &["config", "user.email", "test@example.com"]);
    git(&d, &["config", "user.name", "Test User"]);
    git(&d, &["add", "."]);
    git(&d, &["commit", "-m", "head"]);

    fs::write(d.join("src/lib.rs"), "mod b;\nfn x() {}\n").unwrap();

    let prog = r#"
rel seen(path: file, rev: text).
seen(path, rev) <- scan("HEAD", "src/**/*.rs", path, rev), match(path, rev, /./, line).
seen(path, rev) <- scan("WORK", "src/**/*.rs", path, rev), match(path, rev, /./, line).
? module_edge_rev(s, d, r).
"#;
    let (code, out, err) = run(&d, prog, &[]);
    assert_eq!(code, 0, "run failed:\nstdout={out}\nstderr={err}");
    assert!(out.contains("src/lib.rs\tsrc/a.rs\t"), "cold HEAD edge present: {out}");
    assert!(out.contains("src/lib.rs\tsrc/b.rs\tWORK"), "cold WORK edge present: {out}");

    fs::write(d.join("src/lib.rs"), "mod c;\nfn x() {}\n").unwrap();
    let (code, out, err) = run(&d, prog, &["--changed", "src/lib.rs"]);
    assert_eq!(code, 0, "changed run failed:\nstdout={out}\nstderr={err}");
    assert!(out.contains("src/lib.rs\tsrc/a.rs\t"), "delta keeps HEAD edge: {out}");
    assert!(out.contains("src/lib.rs\tsrc/c.rs\tWORK"), "delta updates WORK edge: {out}");
    assert!(!out.contains("src/lib.rs\tsrc/b.rs\tWORK"), "delta drops old WORK edge: {out}");
}

#[test]
fn crate_edge_from_cargo_dependencies() {
    let d = sandbox("crate_edge");
    fs::create_dir_all(d.join("app/src")).unwrap();
    fs::create_dir_all(d.join("core/src")).unwrap();
    fs::write(d.join("app/Cargo.toml"),
        "[package]\nname = \"app\"\n\n[dependencies]\nrenamed = { package = \"corelib\", path = \"../core\" }\n").unwrap();
    fs::write(d.join("core/Cargo.toml"), "[package]\nname = \"corelib\"\n").unwrap();
    fs::write(d.join("app/src/lib.rs"), "fn app() {}\n").unwrap();
    fs::write(d.join("core/src/lib.rs"), "fn core() {}\n").unwrap();

    let prog = r#"
rel seen(path: file).
seen(path) <- scan("WORK", "**/*.rs", path, rev), match(path, rev, /./, line).
? crate_edge(src, dst, kind, rev).
"#;
    let (code, out, err) = run(&d, prog, &[]);
    assert_eq!(code, 0, "run failed:\nstdout={out}\nstderr={err}");
    assert!(out.contains("app\tcorelib\tdependencies\tWORK"),
        "Cargo package= rename dependency should be a crate_edge: {out}");
}

#[test]
fn changed_manifest_rebuilds_module_derived_rels() {
    let d = sandbox("manifest_delta");
    fs::create_dir_all(d.join("app/src")).unwrap();
    fs::create_dir_all(d.join("core/src")).unwrap();
    fs::write(d.join("app/Cargo.toml"),
        "[package]\nname = \"app\"\n\n[dependencies]\ncorelib = { path = \"../core\" }\n").unwrap();
    fs::write(d.join("core/Cargo.toml"), "[package]\nname = \"corelib\"\n").unwrap();
    fs::write(d.join("app/src/lib.rs"), "fn app() {}\n").unwrap();
    fs::write(d.join("core/src/lib.rs"), "fn core() {}\n").unwrap();

    let prog = r#"
rel seen(path: file).
seen(path) <- scan("WORK", "**/*.rs", path, rev), match(path, rev, /./, line).
rel dep(src: text, dst: text).
dep(src, dst) <- crate_edge(src, dst, kind, rev).
? dep(src, dst).
"#;
    let (code, out, err) = run(&d, prog, &[]);
    assert_eq!(code, 0, "run failed:\nstdout={out}\nstderr={err}");
    assert!(out.contains("app\tcorelib"), "cold derived crate dep present: {out}");

    fs::write(d.join("app/Cargo.toml"), "[package]\nname = \"app\"\n").unwrap();
    let (code, out, err) = run(&d, prog, &["--changed", "app/Cargo.toml"]);
    assert_eq!(code, 0, "changed manifest run failed:\nstdout={out}\nstderr={err}");
    assert!(!out.contains("app\tcorelib"), "manifest delta must rebuild derived dep: {out}");
}
