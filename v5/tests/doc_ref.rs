//! The built-in `doc_ref(file, line, sym)` doc→code bridge relation. A doc
//! heading whose name matches a `type_entity` name links the doc position to the
//! code symbol, making `doc_node` joinable to the type graphs. Populated in
//! `refresh_doc_rels` after both `doc_node` and `type_entity` (the latter earlier
//! in the tick) are filled. Empty unless the program also uses type relations, so
//! each test forces the type-graph refresh with a `force_type` rule over
//! `type_entity`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("doc_ref_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(dir: &Path, prog: &str) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--root", dir.to_str().unwrap(), "--db", dir.join("db").to_str().unwrap()])
        .output().expect("run dl");
    (out.status.code().unwrap_or(-1),
     String::from_utf8_lossy(&out.stdout).into_owned(),
     String::from_utf8_lossy(&out.stderr).into_owned())
}

/// `docs.md` carries headings `Engine` and `Widget` (plus an `Unrelated` heading
/// with no matching type); `src/lib.rs` declares structs `Engine`, `Widget`, and
/// `Unrelated`. Only the name-matched headings should bridge to their symbols.
fn fixture(tag: &str) -> PathBuf {
    let d = sandbox(tag);
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("docs.md"), concat!(
        "# Engine\n",
        "Some text.\n",
        "# Widget\n",
        "More text.\n",
        "# NoMatch\n",
    )).unwrap();
    fs::write(d.join("src/lib.rs"), concat!(
        "struct Engine;\n",
        "struct Widget;\n",
        "struct Unrelated;\n",
    )).unwrap();
    d
}

/// The `? doc_ref(...)` query forces the doc-graph refresh (which now includes
/// doc_ref); the `force_type` rule over `type_entity` forces the type-graph
/// refresh so `type_entity` is populated before the bridge runs. Both matched
/// headings bridge; `NoMatch` (no type of that name) and the unmatched struct
/// `Unrelated` do not.
#[test]
fn doc_ref_bridges_headings_to_matching_type_entities() {
    let d = fixture("bridge");
    let prog = concat!(
        // Feed _file with both the markdown and the rust source.
        "rel seen(path: file).\n",
        "seen(path) <- scan(\"WORK\", \"**/*.md\", path, rev), match(path, rev, /./, line).\n",
        "seen(path) <- scan(\"WORK\", \"**/*.rs\", path, rev), match(path, rev, /./, line).\n",
        // Force the type-graph refresh so type_entity is populated.
        "rel force_type(n: int).\n",
        "force_type(count(sym)) <- type_entity(sym, _, _, _, _, _).\n",
        "? force_type(n).\n",
        "? doc_ref(file, line, sym).\n",
    );
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "stderr: {err}\nstdout: {out}");
    // Engine heading at line 1 bridges to the Engine struct symbol.
    assert!(out.contains("docs.md\t1\t") && out.contains("Engine"),
        "Engine bridge row missing:\n{out}");
    // Widget heading at line 3 bridges to the Widget struct symbol.
    assert!(out.contains("docs.md\t3\t") && out.contains("Widget"),
        "Widget bridge row missing:\n{out}");
    // NoMatch heading has no matching type; Unrelated struct has no matching doc.
    assert!(!out.contains("NoMatch"), "unmatched heading must not bridge:\n{out}");
    assert!(!out.contains("Unrelated"), "unmatched struct must not bridge:\n{out}");
}

/// `doc_ref` is empty when the program uses doc relations but not type relations:
/// `type_entity` exists as a table but is never populated, so the name-match join
/// yields nothing.
#[test]
fn doc_ref_empty_without_type_relations() {
    let d = fixture("no_type");
    let prog = concat!(
        "rel seen(path: file).\n",
        "seen(path) <- scan(\"WORK\", \"**/*.md\", path, rev), match(path, rev, /./, line).\n",
        "? doc_ref(file, line, sym).\n",
    );
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "stderr: {err}\nstdout: {out}");
    // No type relations used -> type_entity unpopulated -> bridge is empty.
    assert!(!out.contains("Engine") && !out.contains("Widget"),
        "doc_ref must be empty without type relations:\n{out}");
}

/// `doc_ref` is a reserved name.
#[test]
fn doc_ref_is_reserved() {
    let d = sandbox("reserved");
    let (code, _out, err) = run(&d, "rel doc_ref(a: text).\n");
    assert_ne!(code, 0);
    assert!(err.contains("built-in"), "reserved-name error expected:\n{err}");
}
