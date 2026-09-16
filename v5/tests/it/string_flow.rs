//! `std/strings.dl` (string-values arc item 4): `string_flow`/`string_flow_trace`
//! follow a `df_lit` id forward through `df_edge`, proving a template
//! interpolation is traceable from its literal seed to wherever it's used.
//!
//! In-proc Engine + `prepare_paths`, mirroring `tests/it/scip_gate.rs`. `use
//! "std/strings.dl".` resolves off disk first, falling back to the binary's
//! embedded corpus (`src/corpus.rs`) — the same path a real program takes.

use std::fs;
use std::path::{Path, PathBuf};

use sprefa_v5::db;
use sprefa_v5::engine::Engine;
use sprefa_v5::prepare_paths;

fn sandbox(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("dl_string_flow_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

fn tick_once(root: &Path) -> Engine {
    let conn = db::open(Some(root.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, root.to_path_buf());
    let (prog, diags, _) = prepare_paths(&[root.join("p.dl")]).unwrap();
    let errs = diags
        .iter()
        .filter(|d| d.severity == sprefa_v5::ast::Severity::Error)
        .count();
    assert_eq!(errs, 0, "program should typecheck: {diags:?}");
    eng.tick(&prog, true).unwrap();
    eng
}

/// A template interpolation traced end to end: `home`'s literal value flows
/// into the template that builds `greeting`, which lands in a `let_bind` node.
#[test]
fn template_interpolation_traces_from_literal_seed_to_binding() {
    let d = sandbox("template");
    fs::write(
        d.join("routes.ts"),
        "const home = '/home';\nconst greeting = `hi ${home}`;\n",
    )
    .unwrap();
    fs::write(
        d.join("p.dl"),
        "rel src(p: file).\n\
         src(p) <- scan(\"WORK\", \"**/*.ts\", p, rev).\n\
         use \"std/strings.dl\".\n",
    )
    .unwrap();
    let eng = tick_once(&d);

    // string_flow_trace: (text, kind, from, to)
    let trace = eng.rel_rows("string_flow_trace", 4);
    assert!(
        trace.iter().any(|r| r[0] == "/home" && r[1] == "lit"),
        "the /home literal must seed a string_flow row: {trace:?}"
    );
    // the template's OWN df_lit row (raw source, hole intact) must also
    // reach somewhere (here: the let_bind for `greeting`).
    assert!(
        trace
            .iter()
            .any(|r| r[0] == "`hi ${home}`" && r[1] == "template"),
        "the template's own text must seed a string_flow row too: {trace:?}"
    );

    // df_edge sanity: the literal's node must actually appear in df_lit at all
    // (a regression that stops emitting df_lit would make the above vacuous).
    let lits = eng.rel_rows("df_lit", 3);
    assert!(
        lits.iter().any(|r| r[1] == "/home" && r[2] == "lit"),
        "{lits:?}"
    );
}

/// A concat (`base + '/x'`) also seeds string_flow — the new `concat` df_node
/// kind rides the same df_lit/df_edge path as `template`.
#[test]
fn concat_seeds_string_flow_too() {
    let d = sandbox("concat");
    fs::write(
        d.join("build.ts"),
        "function url(base: string) {\n    const full = base + '/x';\n    return full;\n}\n",
    )
    .unwrap();
    fs::write(
        d.join("p.dl"),
        "rel src(p: file).\n\
         src(p) <- scan(\"WORK\", \"**/*.ts\", p, rev).\n\
         use \"std/strings.dl\".\n",
    )
    .unwrap();
    let eng = tick_once(&d);
    let trace = eng.rel_rows("string_flow_trace", 4);
    assert!(
        trace
            .iter()
            .any(|r| r[1] == "concat" && r[0] == "base + '/x'"),
        "{trace:?}"
    );
}
