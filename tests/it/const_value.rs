//! `const_value`/`const_value_rev` + `df_lit`/`df_lit_rev` (string-values arc,
//! items 1 and 3, plans/2026-07-10-string-values-const-value.md): a
//! `const`-bound string value folded into a relation, joinable off the
//! `type_entity` sym `const_value.sym` carries. A route-table object literal
//! (`const routes = { home: '/home', nested: { a: '/a' } }`) is the proving
//! fixture: SCIP already resolves REFERENCES to `routes.home`, but the literal
//! path string itself was unreachable in any relation before this arc.
//!
//! In-proc Engine + `prepare_paths`, mirroring `tests/it/scip_gate.rs`.

use std::fs;
use std::path::{Path, PathBuf};

use sprefa_v5::db;
use sprefa_v5::engine::Engine;
use sprefa_v5::prepare_paths;

fn sandbox(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("dl_const_value_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

fn write_fixture(root: &Path) {
    fs::write(
        root.join("routes.ts"),
        "\
const routes = {\n    \
    home: '/home',\n    \
    nested: { a: '/a' },\n\
};\n\
const greeting = `hi ${routes.home}`;\n\
enum Kind { A = 'alpha', B = 'beta' }\n\
let mutablePath = '/mut';\n\
const spread = { ...routes, extra: '/extra' };\n",
    )
    .unwrap();
    fs::write(
        root.join("consts.rs"),
        "const HOME: &str = \"/home\";\n",
    )
    .unwrap();
    fs::write(
        root.join("p.dl"),
        "rel src(p: file).\n\
         src(p) <- scan(\"WORK\", \"**/*.{ts,rs}\", p, rev).\n\
         # bare passthroughs so the type/dataflow family gates fire even though\n\
         # this program's only real interest is reading the builtin tables directly.\n\
         rel touch_entity(sym: text).\n\
         touch_entity(sym) <- type_entity(repo, sym, name, kind, parent, file, line).\n\
         rel touch_const(sym: text).\n\
         touch_const(sym) <- const_value(repo, sym, field, text, kind, file, line).\n\
         rel touch_lit(id: text).\n\
         touch_lit(id) <- df_lit(id, text, kind).\n",
    )
    .unwrap();
}

/// One tick over an in-proc engine; returns it for direct `rel_rows` reads.
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
fn object_literal_const_folds_dotted_fields_and_mints_entity() {
    let d = sandbox("routes");
    write_fixture(&d);
    let eng = tick_once(&d);

    // type_entity: (repo, sym, name, kind, parent, file, line)
    let entities = eng.rel_rows("type_entity", 7);
    let routes_ent = entities.iter().find(|r| r[2] == "routes").unwrap_or_else(|| panic!("no routes entity: {entities:?}"));
    assert_eq!(routes_ent[3], "const", "routes must be a const-kind type_entity: {routes_ent:?}");
    let routes_sym = routes_ent[1].clone();

    // const_value: (repo, sym, field, text, kind, file, line)
    let consts = eng.rel_rows("const_value", 7);
    let home = consts.iter().find(|r| r[1] == routes_sym && r[2] == "home").unwrap_or_else(|| panic!("no routes.home row: {consts:?}"));
    assert_eq!(home[3], "/home");
    assert_eq!(home[4], "lit");
    let nested = consts.iter().find(|r| r[1] == routes_sym && r[2] == "nested.a").unwrap_or_else(|| panic!("no routes.nested.a row: {consts:?}"));
    assert_eq!(nested[3], "/a");
}

#[test]
fn template_const_kind_and_string_enum_member() {
    let d = sandbox("template_enum");
    write_fixture(&d);
    let eng = tick_once(&d);

    let entities = eng.rel_rows("type_entity", 7);
    let consts = eng.rel_rows("const_value", 7);

    let greeting_ent = entities.iter().find(|r| r[2] == "greeting").expect("greeting entity");
    let greeting_row = consts.iter().find(|r| r[1] == greeting_ent[1]).expect("greeting const_value row");
    assert_eq!(greeting_row[4], "template");
    assert_eq!(greeting_row[3], "`hi ${routes.home}`", "holes stay intact: {greeting_row:?}");

    let kind_ent = entities.iter().find(|r| r[2] == "Kind").expect("Kind enum entity");
    assert_eq!(kind_ent[3], "enum");
    let member_a = consts.iter().find(|r| r[1] == kind_ent[1] && r[2] == "A").expect("Kind.A row");
    assert_eq!(member_a[3], "alpha");
    let member_b = consts.iter().find(|r| r[1] == kind_ent[1] && r[2] == "B").expect("Kind.B row");
    assert_eq!(member_b[3], "beta");
}

#[test]
fn let_binding_excluded_spread_property_counted() {
    let d = sandbox("soundness");
    write_fixture(&d);
    let eng = tick_once(&d);

    let entities = eng.rel_rows("type_entity", 7);
    let consts = eng.rel_rows("const_value", 7);
    assert!(!entities.iter().any(|r| r[2] == "mutablePath"), "let binding must not mint an entity: {entities:?}");
    assert!(!consts.iter().any(|r| r[3] == "/mut"), "let binding's string must never be folded: {consts:?}");

    // spread: `extra` still lands (a plain property), the spread base is
    // counted loudly and never followed (no field named for it here).
    let spread_ent = entities.iter().find(|r| r[2] == "spread").expect("spread const entity");
    assert!(consts.iter().any(|r| r[1] == spread_ent[1] && r[2] == "extra" && r[3] == "/extra"));
}

#[test]
fn rust_const_str_and_df_lit_populate() {
    let d = sandbox("rust");
    write_fixture(&d);
    let eng = tick_once(&d);

    let entities = eng.rel_rows("type_entity", 7);
    let consts = eng.rel_rows("const_value", 7);
    let home_ent = entities.iter().find(|r| r[2] == "HOME").expect("Rust HOME const entity");
    assert_eq!(home_ent[3], "const");
    let home_row = consts.iter().find(|r| r[1] == home_ent[1]).expect("HOME const_value row");
    assert_eq!(home_row[3], "/home");

    // df_lit: (id, text, kind) — the dataflow-level twin, independent of the
    // const/let distinction (df_lit captures every string-carrying VALUE node).
    let lits = eng.rel_rows("df_lit", 3);
    assert!(lits.iter().any(|r| r[1] == "/home"), "{lits:?}");
    assert!(lits.iter().any(|r| r[1] == "/mut"), "df_lit is unconditional on const/let: {lits:?}");
    assert!(lits.iter().any(|r| r[2] == "template" && r[1].contains("${")), "{lits:?}");
}

#[test]
fn const_value_rev_carries_rev_column_and_reserved_name_rejected() {
    let d = sandbox("rev_and_guard");
    write_fixture(&d);
    let eng = tick_once(&d);
    // const_value_rev: (repo, sym, field, text, kind, file, line, rev)
    let rev_rows = eng.rel_rows("const_value_rev", 8);
    // `WORK` is an ALIAS; a sandbox with no git HEAD stores the zero sentinel.
    assert!(rev_rows.iter().any(|r| r[7] == crate::util::NO_HEAD_REV), "{rev_rows:?}");

    // A user program declaring `rel const_value` must bail (reserved name).
    let bad = d.join("bad.dl");
    fs::write(&bad, "rel const_value(a: text, b: text).\n").unwrap();
    let res = prepare_paths(&[bad]);
    match res {
        Ok((prog, diags, _)) => {
            let conn = db::open(Some(d.join("db2").to_str().unwrap())).unwrap();
            let mut eng2 = Engine::new(conn, d.to_path_buf());
            let tick_err = eng2.tick(&prog, true);
            assert!(
                tick_err.is_err() || diags.iter().any(|dd| dd.severity == sprefa_v5::ast::Severity::Error),
                "declaring rel const_value must be rejected somewhere in the pipeline"
            );
        }
        Err(_) => {} // also acceptable: rejected at parse/typecheck time
    }
}
