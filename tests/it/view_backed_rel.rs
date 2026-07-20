//! View-backed declared-rel primitive (plans/2026-07-20-view-backed-rel.md).
//! Eight legacy twin rels (df_node_repo, df_arg, df_field, type_edge,
//! module_unresolved, module_binding_resolved, module_binding, const_value) no
//! longer hold a duplicate base table: `rel_<name>` is now a
//! `CREATE VIEW ... AS SELECT DISTINCT <non-rev cols> FROM rel_<name>_rev`.
//!
//! The safety claim is that the DISTINCT view is ROW-IDENTICAL to what the old
//! base table held. The old table was populated one of two ways, both of which
//! collapse to `SELECT DISTINCT <cols>` because every one of these rels has a
//! FULL-ROW primary key (no `key(...)` narrowing, confirmed in
//! src/engine/decls.rs):
//!   - module/type/const: `INSERT OR IGNORE INTO legacy SELECT <cols> FROM rev`
//!     (rebuild_legacy_*), which dedups by the full-row PK == DISTINCT;
//!   - df trio: a direct `refresh_rel(rows.X)` whose Rust dedup key
//!     (`seen_node_repo`/`seen_arg`/`seen_field`, extract/dataflow.rs) is the
//!     full non-rev column set == DISTINCT.
//!
//! This test reconstructs the exact old contract on real data — an `old_<name>`
//! table with the legacy full-row-PK DDL, filled by `INSERT OR IGNORE ... SELECT
//! <cols> FROM rel_<name>_rev` — and asserts `view EXCEPT old` and
//! `old EXCEPT view` are BOTH empty, for every converted rel whose `_rev` twin
//! carries rows. Without two distinct revs the DISTINCT is untested, so the
//! corpus is a committed HEAD plus an edited WORK (two rev strings), and the
//! test also asserts at least one rel carries a logical row present at BOTH revs
//! (so the view's DISTINCT actually collapses a duplicate the twin held twice).

use rusqlite::Connection;
use sprefa_v5::{db, engine::Engine, lex, parse};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("view_backed_rel_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    dir
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git").arg("-C").arg(dir).args(args).output().expect("git");
    assert!(out.status.success(), "git {args:?} failed: {}", String::from_utf8_lossy(&out.stderr));
}

fn init_git(d: &Path) {
    git(d, &["init", "-q"]);
    git(d, &["config", "user.email", "t@example.com"]);
    git(d, &["config", "user.name", "T"]);
}

// Reference every family's `_rev` twin so extraction runs for BOTH scanned revs
// (same mechanism graph_diff_rev.rs uses). The scans bind the rev slot from the
// `diff_pair` fact, so HEAD and WORK both flow into the twins; the legacy rels
// under test are the VIEWs over those twins.
const PROG: &str = r#"
rel diff_pair(base_rev: text, head_rev: text).
diff_pair("HEAD", "WORK").

rel seen(path: file).
seen(path) <- diff_pair(_, head_ref), scan(head_ref, "src/**/*.rs", path, rev).
seen(path) <- diff_pair(base_ref, _), scan(base_ref, "src/**/*.rs", path, rev).

rel touch_df_node_repo(id: text).
touch_df_node_repo(id) <- df_node_repo_rev(id, _, _).
rel touch_df_arg(call: text).
touch_df_arg(call) <- df_arg_rev(call, _, _, _).
rel touch_df_field(id: text).
touch_df_field(id) <- df_field_rev(id, _, _, _).
rel touch_type_edge(a: text).
touch_type_edge(a) <- type_edge_rev(a, _, _, _, _).
rel touch_const_value(sym: text).
touch_const_value(sym) <- const_value_rev(_, sym, _, _, _, _, _, _).
rel touch_module_unresolved(file: text).
touch_module_unresolved(file) <- module_unresolved_rev(file, _, _, _, _).
rel touch_module_binding(file: text).
touch_module_binding(file) <- module_binding_rev(file, _, _, _, _, _).
rel touch_module_binding_resolved(file: text).
touch_module_binding_resolved(file) <- module_binding_resolved_rev(file, _, _, _, _).
"#;

const CARGO_TOML: &str = "[package]\nname = \"fixture\"\nversion = \"0.0.0\"\nedition = \"2021\"\n";
const LIB_RS: &str = "pub mod a;\npub mod b;\n";

// HEAD: Alpha, Beta (Beta -> Alpha field edge), a const string, calls + a
// struct literal (df_arg/df_field), and imports (module_binding + a broken one
// for module_unresolved).
const A_HEAD: &str = "\
pub const GREETING: &str = \"hello\";\n\
pub struct Alpha { pub n: i64 }\n\
pub struct Beta { pub a: Alpha }\n\
pub fn make(x: i64) -> Beta { Beta { a: Alpha { n: x } } }\n\
pub fn run() { let _b = make(1); }\n";

const B_HEAD: &str = "\
use crate::a::Alpha as AliasedAlpha;\n\
use crate::ghost::Missing;\n\
pub fn other() -> AliasedAlpha { AliasedAlpha { n: 0 } }\n";

// WORK: identical to HEAD except one struct added (Gamma) and one const value
// changed — so most rows are byte-identical at BOTH revs (the DISTINCT-collapse
// case), with a few rows unique to one rev.
const A_WORK: &str = "\
pub const GREETING: &str = \"hello\";\n\
pub struct Alpha { pub n: i64 }\n\
pub struct Beta { pub a: Alpha }\n\
pub struct Gamma { pub a: Alpha }\n\
pub fn make(x: i64) -> Beta { Beta { a: Alpha { n: x } } }\n\
pub fn run() { let _b = make(1); }\n";

/// (rel, columns) for each converted rel. Columns are the exact non-rev columns
/// of the legacy decl, in order — the same list the view body and the old
/// rebuild SQL use.
const CONVERTED: &[(&str, &[&str])] = &[
    ("df_node_repo", &["id", "repo"]),
    ("df_arg", &["call", "pos", "arg"]),
    ("df_field", &["id", "field", "value"]),
    ("type_edge", &["from", "to", "kind", "repo"]),
    ("module_unresolved", &["file", "specifier", "reason", "line"]),
    ("module_binding_resolved", &["file", "local", "source", "dst"]),
    ("module_binding", &["file", "local_name", "source_module", "imported_name", "kind"]),
    ("const_value", &["repo", "sym", "field", "text", "kind", "file", "line"]),
];

fn quoted(cols: &[&str]) -> String {
    cols.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(", ")
}

fn count(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}

fn object_type(conn: &Connection, name: &str) -> Option<String> {
    conn.query_row(
        "SELECT type FROM sqlite_master WHERE name = ?1",
        [name],
        |r| r.get::<_, String>(0),
    )
    .ok()
}

#[test]
fn converted_rel_views_are_row_identical_to_the_old_table_across_two_revs() {
    let d = sandbox("except");
    fs::write(d.join("Cargo.toml"), CARGO_TOML).unwrap();
    fs::write(d.join("src/lib.rs"), LIB_RS).unwrap();
    fs::write(d.join("src/a.rs"), A_HEAD).unwrap();
    fs::write(d.join("src/b.rs"), B_HEAD).unwrap();
    init_git(&d);
    git(&d, &["add", "."]);
    git(&d, &["commit", "-q", "-m", "base"]);
    // WORK edit: add Gamma; leave b.rs identical so its imports are byte-equal
    // at both revs (the DISTINCT-collapse case for module_binding).
    fs::write(d.join("src/a.rs"), A_WORK).unwrap();

    let prog = parse::parse(lex::lex(PROG).unwrap()).unwrap();
    let conn = db::open(Some(d.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    // Fact lands tick 1, scans + extraction tick 2, twins converge after. Tick
    // to a fixpoint (the daemon's steady state).
    for _ in 0..5 {
        eng.tick(&prog, true).unwrap();
    }

    let vconn = Connection::open(d.join("db")).unwrap();

    // Every converted rel is a VIEW now, never a base table.
    for (rel, _) in CONVERTED {
        assert_eq!(
            object_type(&vconn, &format!("rel_{rel}")).as_deref(),
            Some("view"),
            "rel_{rel} must be a VIEW, not a table"
        );
    }

    // The corpus spans two distinct revs (sanity: the data-driven base scan
    // resolved and the twins carry both).
    assert!(
        count(&vconn, "SELECT COUNT(DISTINCT rev) FROM rel_df_node_repo_rev") >= 2,
        "fixture must populate df_node_repo_rev across two distinct revs"
    );

    let mut proven = 0usize;
    for (rel, cols) in CONVERTED {
        let rev_tbl = format!("rel_{rel}_rev");
        let rows_in_rev: i64 = count(&vconn, &format!("SELECT COUNT(*) FROM {rev_tbl}"));
        if rows_in_rev == 0 {
            // The DISTINCT is vacuously correct on an empty twin, but proves
            // nothing — skip and let the coverage floor below catch a fixture
            // that populated nothing.
            continue;
        }
        let cols_sql = quoted(cols);
        let pk_sql = quoted(cols);
        let old = format!("old_{rel}");
        // The legacy full-row-PK table shape (no key() narrowing on any of these
        // rels) + the exact pre-change rebuild write.
        vconn.execute_batch(&format!("DROP TABLE IF EXISTS {old};")).unwrap();
        vconn
            .execute_batch(&format!(
                "CREATE TABLE {old} ({}, PRIMARY KEY ({pk_sql}));",
                cols.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(", ")
            ))
            .unwrap();
        vconn
            .execute_batch(&format!(
                "INSERT OR IGNORE INTO {old} ({cols_sql}) SELECT {cols_sql} FROM {rev_tbl};"
            ))
            .unwrap();

        let view_minus_old: i64 = count(
            &vconn,
            &format!("SELECT COUNT(*) FROM (SELECT {cols_sql} FROM rel_{rel} EXCEPT SELECT {cols_sql} FROM {old})"),
        );
        let old_minus_view: i64 = count(
            &vconn,
            &format!("SELECT COUNT(*) FROM (SELECT {cols_sql} FROM {old} EXCEPT SELECT {cols_sql} FROM rel_{rel})"),
        );
        assert_eq!(view_minus_old, 0, "{rel}: view has rows the old table did not");
        assert_eq!(old_minus_view, 0, "{rel}: old table has rows the view does not");
        proven += 1;
    }

    // Coverage floor: the dataflow trio + type_edge must actually be populated
    // and proven (a fixture that silently extracted nothing would pass every
    // EXCEPT vacuously otherwise).
    for rel in ["df_node_repo", "df_arg", "df_field", "type_edge", "const_value"] {
        assert!(
            count(&vconn, &format!("SELECT COUNT(*) FROM rel_{rel}")) > 0,
            "{rel} view must carry rows in this fixture"
        );
    }
    assert!(proven >= 5, "expected at least 5 converted rels proven, got {proven}");

    // DISTINCT is genuinely exercised: some logical (id, repo) row appears at
    // BOTH revs in the twin, so the view collapsed a duplicate the twin held
    // twice. (Most of the corpus is byte-identical across HEAD and WORK.)
    let collapsed: i64 = count(
        &vconn,
        "SELECT COUNT(*) FROM (SELECT \"id\", \"repo\" FROM rel_df_node_repo_rev \
         GROUP BY \"id\", \"repo\" HAVING COUNT(DISTINCT rev) >= 2)",
    );
    assert!(
        collapsed > 0,
        "expected at least one df_node_repo row present at both revs (so the view's DISTINCT collapses it)"
    );
}
