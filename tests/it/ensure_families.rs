//! `Engine::ensure_families` (src/engine/tick.rs): force a named extraction
//! family's rows present without ticking a program that references any of its
//! rels (the `used(prog)` gate a normal `tick`/`run` applies). In-proc Engine +
//! `prepare_paths`, mirroring `tests/it/const_value.rs`.

use std::fs;
use std::path::{Path, PathBuf};

use sprefa_v5::db;
use sprefa_v5::engine::Engine;
use sprefa_v5::prepare_paths;

fn sandbox(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("dl_ensure_families_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

fn write_fixture(root: &Path) {
    fs::write(
        root.join("greet.ts"),
        "function greet(name: string): string {\n    return `hi ${name}`;\n}\n",
    )
    .unwrap();
    // A baseline program that scans the corpus but names none of the
    // type-graph rels anywhere in a rule body/head, so `TypeFamily::used`
    // is false and a normal tick never refreshes it.
    fs::write(
        root.join("p.dl"),
        "rel src(p: file).\nsrc(p) <- scan(\"WORK\", \"**/*.ts\", p, rev).\n",
    )
    .unwrap();
}

/// One tick over an in-proc engine (populates the scanned file corpus + the
/// empty builtin tables, but no type-graph rows — the baseline program never
/// names a type-graph rel); returns it for `ensure_families` + `rel_rows`.
fn tick_once(root: &Path) -> Engine {
    let conn = db::open(Some(root.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, root.to_path_buf());
    let (prog, diags, _) = prepare_paths(&[root.join("p.dl")]).unwrap();
    let errs = diags.iter().filter(|d| d.severity == sprefa_v5::ast::Severity::Error).count();
    assert_eq!(errs, 0, "program should typecheck: {diags:?}");
    eng.tick(&prog, true).unwrap();
    eng
}

#[test]
fn ensure_families_populates_an_unused_family_without_a_program_tick() {
    let d = sandbox("type");
    write_fixture(&d);
    let mut eng = tick_once(&d);

    // Baseline: the program never names a type-graph rel, so the normal tick
    // left `type_entity` empty (the `used(prog)` gate skipped `TypeFamily`).
    let before = eng.rel_rows("type_entity", 7);
    assert!(before.is_empty(), "type_entity should start empty: {before:?}");

    eng.ensure_families(&["type-rels"]).expect("ensure_families(type-rels)");

    // After the forced refresh, the already-scanned `greet.ts` is parsed and
    // its function entity is present — no program tick over a `? type_entity`
    // query was needed to get here.
    let after = eng.rel_rows("type_entity", 7);
    assert!(
        after.iter().any(|row| row[2] == "greet"),
        "expected a greet type_entity row after ensure_families: {after:?}"
    );
}

#[test]
fn ensure_families_errors_on_an_unknown_family_name() {
    let d = sandbox("unknown");
    write_fixture(&d);
    let mut eng = tick_once(&d);

    let err = eng
        .ensure_families(&["not-a-real-family"])
        .expect_err("an unregistered family name must error, not silently no-op");
    assert!(
        err.to_string().contains("not-a-real-family"),
        "error should name the bad family: {err}"
    );
}
