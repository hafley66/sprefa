//! `const_string_member(file, object, member, value)` — retired as a builtin
//! relation (plans/2026-07-10-string-values-const-value.md follow-up: the
//! evidence diff against `const_value ⋈ type_entity` found exactly ONE gap,
//! a const declared inside a function body, closed in `typegraph.rs`'s
//! `TsNestedConstWalker`). It now lives as a DERIVED VIEW in
//! `std/strings.dl`, same 4-arity shape, so every existing consumer keeps
//! working via `use "std/strings.dl".`.
//!
//! In-proc Engine + `prepare_paths`, mirroring `tests/it/string_flow.rs`.

use std::fs;
use std::path::{Path, PathBuf};

use sprefa_v5::db;
use sprefa_v5::engine::Engine;
use sprefa_v5::prepare_paths;

fn sandbox(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("dl_strings_view_{tag}_{}", std::process::id()));
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

fn write_program(root: &Path) {
    fs::write(
        root.join("p.dl"),
        "rel src(p: file).\n\
         src(p) <- scan(\"WORK\", \"**/*.ts\", p, rev).\n\
         use \"std/strings.dl\".\n",
    )
    .unwrap();
}

/// ACCEPT: a route lookup table's flat string members all show up, keyed by
/// the const's own binding name.
#[test]
fn route_table_members_are_discovered() {
    let d = sandbox("routes");
    fs::write(
        d.join("routes.ts"),
        "export const ROUTES = {\n    home: \"/\",\n    user: \"/u/:id\",\n};\n",
    )
    .unwrap();
    write_program(&d);
    let eng = tick_once(&d);

    // const_string_member: (file, object, member, value)
    let mut rows = eng.rel_rows("const_string_member", 4);
    rows.sort_by(|a, b| a[2].cmp(&b[2]));
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert_eq!(rows[0], vec!["routes.ts", "ROUTES", "home", "/"]);
    assert_eq!(rows[1], vec!["routes.ts", "ROUTES", "user", "/u/:id"]);
}

/// ACCEPT (the two-line-join criterion, per the mapping-feedback plan):
/// resolving "which route string does key X map to" is exactly a two-line
/// dl join over `const_string_member`.
#[test]
fn resolving_a_route_key_is_a_two_line_join() {
    let d = sandbox("two_line_join");
    fs::write(
        d.join("routes.ts"),
        "export const ROUTES = { home: \"/\", user: \"/u/:id\" };\n",
    )
    .unwrap();
    fs::write(
        d.join("p.dl"),
        "rel src(p: file).\n\
         src(p) <- scan(\"WORK\", \"**/*.ts\", p, rev).\n\
         use \"std/strings.dl\".\n\
         rel route_for(routeKey: text, routePath: text).\n\
         route_for(routeKey, routePath) <- const_string_member(_, \"ROUTES\", routeKey, routePath).\n",
    )
    .unwrap();
    let eng = tick_once(&d);
    let rows = eng.rel_rows("route_for", 2);
    let hit = rows.iter().find(|r| r[0] == "user").expect("user row");
    assert_eq!(hit[1], "/u/:id");
}

/// ACCEPT: `export const` and `as const` both discover the same shape; a
/// nested object value and a non-string value are both skipped rather than
/// guessed at (the view's flat-field filter + `const_value`'s "lit" kind
/// gate, matching the retired builtin's "flat members only" contract).
#[test]
fn export_and_as_const_wrappers_unwrap_and_non_string_members_skip() {
    let d = sandbox("wrappers");
    fs::write(
        d.join("mixed.ts"),
        "export const STATUS = {\n    ok: \"OK\",\n    nested: { inner: \"skipped\" },\n    count: 42,\n} as const;\n",
    )
    .unwrap();
    write_program(&d);
    let eng = tick_once(&d);
    let rows = eng.rel_rows("const_string_member", 4);
    assert_eq!(
        rows.len(),
        1,
        "{rows:?}: only the flat string member survives"
    );
    assert_eq!(rows[0], vec!["mixed.ts", "STATUS", "ok", "OK"]);
}

/// ACCEPT: the evidence-diff gap fix — a lookup table declared inside a
/// function body still counts (`const_string_member`'s "generically
/// discovered, not scope-restricted" contract, now backed by `const_value`'s
/// `TsNestedConstWalker`).
#[test]
fn const_inside_function_body_still_counts() {
    let d = sandbox("nested_scope");
    fs::write(
        d.join("table.ts"),
        "function makeTable() {\n    const INNER_TABLE = { x: '/inner/x' };\n    return INNER_TABLE;\n}\n",
    )
    .unwrap();
    write_program(&d);
    let eng = tick_once(&d);
    let rows = eng.rel_rows("const_string_member", 4);
    assert!(
        rows.iter()
            .any(|r| r[1] == "INNER_TABLE" && r[2] == "x" && r[3] == "/inner/x"),
        "{rows:?}"
    );
}

/// REJECT: `const_string_member` is no longer a reserved builtin name — the
/// retirement means a program can declare its own `rel const_string_member`
/// (shadowing the std/strings.dl view only if that module isn't `use`d).
#[test]
fn const_string_member_is_a_plain_user_declarable_rel_without_the_module() {
    let d = sandbox("not_reserved");
    fs::write(d.join("empty.ts"), "export const X = 1;\n").unwrap();
    fs::write(
        d.join("p.dl"),
        "rel const_string_member(file: text, object: text, member: text, value: text).\n\
         const_string_member(\"f.ts\", \"O\", \"m\", \"v\").\n",
    )
    .unwrap();
    let eng = tick_once(&d);
    let rows = eng.rel_rows("const_string_member", 4);
    assert_eq!(
        rows,
        vec![vec![
            "f.ts".to_string(),
            "O".to_string(),
            "m".to_string(),
            "v".to_string()
        ]]
    );
}
