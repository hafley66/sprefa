//! `module_binding_resolved`/`module_binding_resolved_rev`: the index-free alias hop wired into
//! `refresh_type_rels`'s `resolve` and `refresh_call_rels`'s `resolve_callee`
//! (src/engine/extract.rs), fed by the module resolvers' own parse
//! (src/modgraph.rs's `ModuleRef::bindings`) — no `index.scip` involved.
//! `use x::y as z` / TS `import { a as b }` / Kotlin `import a.b.C as D` name a
//! LOCAL binding the name-keyed def bucket (`by_name`) has no bucket for (the
//! def's real name is `y`/`a`/`C`, not the local `z`/`b`/`D`); the alias hop
//! resolves it directly, pinned to the aliased target file, and never falls
//! through to a coincidental global match on a miss.
//!
//! Mirrors `resolver_import_narrowing.rs`'s harness shape (sandbox + `--no-
//! daemon` + an empty `$SPREFA_CONFIG` for hermeticity).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("resolver_import_alias_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Empty config via `$SPREFA_CONFIG`, same hermeticity rationale as
/// `resolver_import_narrowing.rs::empty_config`.
fn empty_config(dir: &Path) -> PathBuf {
    let cfg = dir.join("empty.toml");
    fs::write(&cfg, "# no repos\n").unwrap();
    cfg
}

const RUST_PROG: &str = r#"
rel seen(path: file).
seen(path) <- scan("WORK", "src/**/*.rs", path, rev), match(path, rev, /./, line).
? type_link(src, dst, kind).
? call_edge(caller, callee, kind).
"#;

const TS_PROG: &str = r#"
rel seen(path: file).
seen(path) <- scan("WORK", "src/**/*.ts", path, rev), match(path, rev, /./, line).
? call_edge(caller, callee, kind).
"#;

fn run(dir: &Path, cfg: &Path, db: &Path, prog: &str) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--no-daemon", "--db", db.to_str().unwrap()])
        .current_dir(dir)
        .env("SPREFA_CONFIG", cfg)
        .output()
        .expect("run dl");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// (a) aliased `use` resolves in type_link + call_edge; (c) shadowing: a local
/// `fn helper()` beats an aliased `use ... as helper` of a DIFFERENT def
/// (`pkg::make`) — proving the shadow guard, not just that by_name happens to
/// find the local fn (if the alias hop fired unconditionally, first, it would
/// wrongly override the correct by_name resolution here).
fn write_rust_fixture(dir: &Path, revision_marker: &str) {
    fs::create_dir_all(dir.join("src/pkg")).unwrap();
    fs::create_dir_all(dir.join("src/app")).unwrap();
    fs::write(dir.join("src/lib.rs"), "mod pkg;\nmod app;\n").unwrap();
    fs::write(
        dir.join("src/pkg.rs"),
        "pub fn make() -> i64 { 1 }\npub struct Config { pub n: i64 }\n",
    )
    .unwrap();
    fs::write(
        dir.join("src/app.rs"),
        format!(
            "use crate::pkg::make as mk;\n\
             use crate::pkg::Config as Cfg;\n\
             pub struct Holder {{ pub c: Cfg }}\n\
             pub fn build() -> i64 {{ mk() }}\n\
             use crate::pkg::make as helper;\n\
             fn helper() -> i64 {{ 2 }}\n\
             pub fn build_shadowed() -> i64 {{ helper() }}\n\
             // {revision_marker}\n"
        ),
    )
    .unwrap();
}

#[test]
fn rust_aliased_use_resolves_without_scip_index() {
    let d = sandbox("rust_alias");
    let cfg = empty_config(&d);
    write_rust_fixture(&d, "rev-1");

    let (code, out, err) = run(&d, &cfg, &d.join("db"), RUST_PROG);
    assert_eq!(code, 0, "run failed:\nstdout={out}\nstderr={err}");

    let links = out.split("? type_link").nth(1).unwrap_or("").split("? call_edge").next().unwrap_or("");
    let calls = out.split("? call_edge").nth(1).unwrap_or("");

    // aliased struct field type (Cfg -> pkg::Config)
    assert!(
        links.lines().any(|l| l.contains("app.rs::struct::Holder")
            && l.contains("pkg.rs::struct::Config")),
        "type_link resolves the aliased struct type:\n{links}"
    );
    // aliased call (mk() -> pkg::make)
    assert!(
        calls.lines().any(|l| l.contains("app.rs::function::build\t")
            && l.contains("pkg.rs::function::make")),
        "call_edge resolves the aliased call:\n{calls}"
    );
}

#[test]
fn local_def_shadows_an_aliased_import_of_the_same_local_name() {
    let d = sandbox("shadow");
    let cfg = empty_config(&d);
    write_rust_fixture(&d, "rev-1");

    let (code, out, err) = run(&d, &cfg, &d.join("db"), RUST_PROG);
    assert_eq!(code, 0, "run failed:\nstdout={out}\nstderr={err}");
    let calls = out.split("? call_edge").nth(1).unwrap_or("");

    // build_shadowed() calls helper(): app.rs both ALIASES pkg::make as
    // "helper" AND declares its own `fn helper()`. The local def must win.
    assert!(
        calls.lines().any(|l| l.contains("app.rs::function::build_shadowed")
            && l.contains("app.rs::function::helper")),
        "local helper() must win over the aliased import:\n{calls}"
    );
    assert!(
        !calls.lines().any(|l| l.contains("build_shadowed") && l.contains("pkg.rs::function::make")),
        "the alias hop must not override the local shadowing def:\n{calls}"
    );
}

#[test]
fn what_verb_surfaces_the_canonical_def_via_module_binding_resolved() {
    // `dl what mk`: no index.scip anywhere, so any hit can only come from the
    // module_binding_resolved-joined anchor path (src/anchor.rs).
    let d = sandbox("what");
    let cfg = empty_config(&d);
    write_rust_fixture(&d, "rev-1");

    let out = Command::new(DL)
        .args(["what", "mk", "--no-daemon"])
        .current_dir(&d)
        .env("SPREFA_CONFIG", &cfg)
        .output()
        .expect("run dl what");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(out.status.success(), "what failed:\nstdout={stdout}\nstderr={stderr}");
    assert!(stdout.contains("module_binding_resolved"), "module_binding_resolved hit missing:\n{stdout}");
    assert!(stdout.contains("pkg.rs"), "should surface pkg.rs, the aliased def's file:\n{stdout}");
}

#[test]
fn editing_the_alias_target_flips_resolution_on_a_retick() {
    // Same shape as resolver_import_narrowing's flip test: pkg.rs declares TWO
    // fns; app.rs's `mk` alias initially targets `make`, then is edited to
    // target `other`. The `extract:type/call:<rev>` digest fold over
    // `module_binding_resolved_rev` (not just `module_edge_rev`) must catch the edit so
    // a warm retick against the SAME db doesn't serve the stale resolution.
    let d = sandbox("flip");
    let cfg = empty_config(&d);
    let db = d.join("db");
    fs::create_dir_all(d.join("src/pkg")).unwrap();
    fs::create_dir_all(d.join("src/app")).unwrap();
    fs::write(d.join("src/lib.rs"), "mod pkg;\nmod app;\n").unwrap();
    fs::write(
        d.join("src/pkg.rs"),
        "pub fn make() -> i64 { 1 }\npub fn other() -> i64 { 2 }\n",
    )
    .unwrap();
    let write_app = |target: &str, marker: &str| {
        fs::write(
            d.join("src/app.rs"),
            format!(
                "use crate::pkg::{target} as mk;\n\
                 pub fn build() -> i64 {{ mk() }}\n\
                 // {marker}\n"
            ),
        )
        .unwrap();
    };

    write_app("make", "rev-1");
    let (code1, out1, err1) = run(&d, &cfg, &db, RUST_PROG);
    assert_eq!(code1, 0, "first run failed:\nstdout={out1}\nstderr={err1}");
    let calls1 = out1.split("? call_edge").nth(1).unwrap_or("");
    assert!(
        calls1.lines().any(|l| l.contains("app.rs::function::build") && l.contains("pkg.rs::function::make")),
        "first tick resolves mk() to pkg::make:\n{calls1}"
    );

    // Retarget the alias to `other`. `marker` differs in length from "rev-1"
    // (the mtime/size fast-path note in resolver_import_narrowing.rs) so this
    // edit can never collide in size with the prior write.
    write_app("other", "rev-2-after-the-alias-edit");
    let (code2, out2, err2) = run(&d, &cfg, &db, RUST_PROG);
    assert_eq!(code2, 0, "second run failed:\nstdout={out2}\nstderr={err2}");
    let calls2 = out2.split("? call_edge").nth(1).unwrap_or("");
    assert!(
        calls2.lines().any(|l| l.contains("app.rs::function::build") && l.contains("pkg.rs::function::other")),
        "re-tick flips mk() to pkg::other after the alias edit:\n{calls2}"
    );
    assert!(
        !calls2.lines().any(|l| l.contains("build") && l.contains("pkg.rs::function::make")),
        "the stale pkg::make resolution must not survive the retick:\n{calls2}"
    );
}

#[test]
fn ts_aliased_import_call_resolves_without_scip_index() {
    let d = sandbox("ts_alias");
    let cfg = empty_config(&d);
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/model.ts"), "export function foo(): number { return 1 }\n").unwrap();
    fs::write(
        d.join("src/app.ts"),
        "import { foo as bar } from './model'\nexport function build(): number { return bar() }\n",
    )
    .unwrap();

    let (code, out, err) = run(&d, &cfg, &d.join("db"), TS_PROG);
    assert_eq!(code, 0, "run failed:\nstdout={out}\nstderr={err}");
    let calls = out.split("? call_edge").nth(1).unwrap_or("");
    assert!(
        calls.lines().any(|l| l.contains("app.ts::function::build")
            && l.contains("model.ts::function::foo")),
        "call_edge resolves the TS aliased import call:\n{calls}"
    );
}
