//! `template_parts(file, line, node, idx, kind, text)` — every template
//! literal split into its ordered static/interpolated pieces. Own builtin
//! family (rides the oxc TS front-end; see `Engine::refresh_template_rels`),
//! TS/TSX/JS/JSX/MJS/CJS only. `node` groups an occurrence's pieces — it is
//! the `df_node`/`df_lit` id for the SAME template occurrence (`file:byte:
//! template`, text, joins `df_lit.id`/`df_edge.to`), NOT a bare byte offset;
//! `idx` orders them 0-based; `kind` is `static`|`expr`; `text` is verbatim.
//! `line` is 1-based, matching `comment_node`/`sg`/`diag`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// `dl`'s text query output is `? rel => col\tcol...` then one indented row
/// per line, then a `  (N rows)` footer — pull out just the indented data
/// rows, tab-split.
fn data_rows(out: &str) -> Vec<Vec<String>> {
    out.lines()
        .filter(|l| l.starts_with("  ") && !l.trim_start().starts_with('('))
        .map(|l| l.trim().split('\t').map(|s| s.to_string()).collect())
        .collect()
}

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("template_parts_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    dir
}

fn run(dir: &Path, prog: &str) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--db", dir.join("db").to_str().unwrap(), "--no-daemon"])
        .current_dir(dir)
        .output()
        .expect("run dl");
    (out.status.code().unwrap_or(-1),
     String::from_utf8_lossy(&out.stdout).into_owned(),
     String::from_utf8_lossy(&out.stderr).into_owned())
}

const SEEN_TS: &str = r#"
rel seen(path: file).
seen(path) <- scan("WORK", "src/**/*.ts", path, rev).
"#;

/// ACCEPT: `` `GET /users/${userId}/posts` `` splits into three ordered
/// pieces, all sharing one `node` id.
#[test]
fn template_built_route_splits_into_ordered_pieces() {
    let dir = sandbox("route");
    fs::write(dir.join("src/routes.ts"),
        "// routes module\n\
         export const userPostsRoute = `GET /users/${userId}/posts`;\n").unwrap();
    let prog = format!("{SEEN_TS}? template_parts(file, line, node, idx, kind, text).\n");
    let (code, out, err) = run(&dir, &prog);
    assert_eq!(code, 0, "stderr:\n{err}");

    let mut rows: Vec<(String, i64, String, i64, String, String)> = data_rows(&out).into_iter()
        .map(|cols| {
            assert_eq!(cols.len(), 6, "row shape: {cols:?}");
            (cols[0].clone(), cols[1].parse().unwrap(), cols[2].clone(),
             cols[3].parse().unwrap(), cols[4].clone(), cols[5].clone())
        })
        .collect();
    assert_eq!(rows.len(), 3, "{out}");
    rows.sort_by_key(|r| r.3); // sort by idx — the row order the ids prove.

    let node_id = rows[0].2.clone();
    // node is the df_node/df_lit id for this template occurrence:
    // "{file}:{byte-offset}:template".
    assert!(node_id.ends_with(":template"), "{node_id}");
    assert!(node_id.starts_with("src/routes.ts:"), "{node_id}");
    for r in &rows {
        assert_eq!(r.2, node_id, "every piece of one occurrence shares node: {out}");
        assert_eq!(r.0, "src/routes.ts");
        assert_eq!(r.1, 2, "1-based line of the template literal: {out}");
    }
    assert_eq!((rows[0].3, rows[0].4.as_str(), rows[0].5.as_str()), (0, "static", "GET /users/"));
    assert_eq!((rows[1].3, rows[1].4.as_str(), rows[1].5.as_str()), (1, "expr", "userId"));
    assert_eq!((rows[2].3, rows[2].4.as_str(), rows[2].5.as_str()), (2, "static", "/posts"));
}

/// ACCEPT (the mapping use case): a template-built piece joins a rel of known
/// route prefixes — the motivating join from the mapping-feedback plan.
#[test]
fn template_static_piece_joins_known_route_table() {
    let dir = sandbox("join");
    fs::write(dir.join("src/routes.ts"),
        "export const userPostsRoute = `GET /users/${userId}/posts`;\n\
         export const orphanRoute = `DELETE /widgets/${widgetId}`;\n").unwrap();
    let prog = format!(
        "{SEEN_TS}\
         rel known_route(prefix: text).\n\
         known_route(\"GET /users/\").\n\
         rel route_hit(path: text, node: text, prefix: text).\n\
         route_hit(path, node, prefix) <- template_parts(path, _, node, 0, \"static\", prefix), known_route(prefix).\n\
         ? route_hit(path, node, prefix).\n"
    );
    let (code, out, err) = run(&dir, &prog);
    assert_eq!(code, 0, "stderr:\n{err}");
    let rows = data_rows(&out);
    assert_eq!(rows.len(), 1, "{out}");
    assert_eq!(rows[0][0], "src/routes.ts");
    assert_eq!(rows[0][2], "GET /users/");
    // the DELETE route's prefix is NOT in known_route, so it must not appear.
    assert!(!out.contains("DELETE /widgets/"), "{out}");
}

/// ACCEPT (D5/template_parts re-key follow-up, plans/
/// 2026-07-10-string-values-const-value.md): `node` is now the SAME id
/// `df_lit`/`df_node`/`df_edge` mint for the template occurrence, so a
/// static piece joins `df_lit.id` directly, AND `node` joins `df_edge.to`
/// for whatever flows INTO the template — an interpolated variable's
/// `var_read` node has an edge into the template's own node.
#[test]
fn template_node_joins_df_lit_and_df_edge() {
    let dir = sandbox("df_join");
    fs::write(dir.join("src/greet.ts"),
        "const name = \"world\";\nconst greeting = `hi ${name}`;\n").unwrap();

    // every piece of the `hi ${name}` occurrence joins ITS OWN df_lit row
    // (the raw source, hole intact, kind = "template").
    let lit_prog = format!(
        "{SEEN_TS}\
         rel piece_and_lit(path: text, kind: text, text: text, litText: text, litKind: text).\n\
         piece_and_lit(path, kind, text, litText, litKind) <-\n\
             template_parts(path, _, node, _, kind, text),\n\
             df_lit(node, litText, litKind).\n\
         ? piece_and_lit(path, kind, text, litText, litKind).\n"
    );
    let (code, out, err) = run(&dir, &lit_prog);
    assert_eq!(code, 0, "stderr:\n{err}");
    let lit_join = data_rows(&out);
    assert!(
        lit_join.iter().any(|r| r[0] == "src/greet.ts" && r[4] == "template" && r[3].contains("${name}")),
        "{out}"
    );

    // the interpolated `name` var flows INTO the template's node via df_edge.
    let edge_prog = format!(
        "{SEEN_TS}\
         rel piece_and_flow_in(path: text, kind: text, text: text, fromKind: text).\n\
         piece_and_flow_in(path, kind, text, fromKind) <-\n\
             template_parts(path, _, node, 0, kind, text),\n\
             df_edge(from, node),\n\
             df_node(from, fromKind, _, _, _, _).\n\
         ? piece_and_flow_in(path, kind, text, fromKind).\n"
    );
    let (code, out, err) = run(&dir, &edge_prog);
    assert_eq!(code, 0, "stderr:\n{err}");
    let edge_join = data_rows(&out);
    assert!(
        edge_join.iter().any(|r| r[0] == "src/greet.ts" && r[3] == "var_read"),
        "the template's node must be a df_edge target for the interpolated var: {out}"
    );
}

/// REJECT: `rel template_parts(...)` is a reserved built-in name — the engine
/// bails with a named diagnostic, not a silent no-op.
#[test]
fn rel_decl_of_template_parts_bails() {
    let dir = sandbox("reserved");
    let prog = "rel template_parts(file: text, line: int, node: text, idx: int, kind: text, text: text).\n";
    let (code, _out, err) = run(&dir, prog);
    assert_ne!(code, 0, "reserved-name decl must fail");
    assert!(err.contains("built-in template-literal relation") && err.contains("template_parts"),
        "{err}");
}

/// REJECT: querying `template_parts` at the wrong arity surfaces the named
/// `expects N cols, got M` diagnostic instead of silently returning nothing.
/// (Variable names deliberately do NOT match any column name — `file`/`line`/
/// `node`/`idx` would otherwise trigger the all-puns shorthand rewrite
/// `resolve_atom` gives a bare-Var atom whose arity doesn't match, which is a
/// different, intentional feature, not this arity bail.) A per-query failure
/// is reported (not a program-fatal bail — a query error is `eprintln`'d and
/// the tick continues, same as any other bad query), so the assertion is on
/// the named diagnostic text, not the process exit code.
#[test]
fn wrong_arity_query_surfaces_named_diagnostic() {
    let dir = sandbox("arity");
    fs::write(dir.join("src/routes.ts"),
        "export const userPostsRoute = `GET /users/${userId}/posts`;\n").unwrap();
    let prog = format!("{SEEN_TS}? template_parts(sourcePath, sourceLine, occurrenceNode, pieceIndex).\n");
    let (_code, out, err) = run(&dir, &prog);
    assert!(err.contains("template_parts") && err.contains("expects 6 cols, got 4"), "{err}");
    // never a silent no-op: no "? template_parts => ..." header/rows printed.
    assert!(!out.contains("? template_parts"), "{out}");
}
