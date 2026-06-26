//! The built-in `doc_ref(_, file, line, sym, kind, matched_name)` doc→code bridge
//! relation. A doc heading whose name matches a `type_entity` name links the doc
//! position to the code symbol, making `doc_node` joinable to the type graphs.
//! Three matching paths: exact heading name, normalized heading name (articles
//! + trailing kind words stripped), and code-block identifier scan. Populated in
//! `refresh_doc_rels` after both `doc_node` and `type_entity` (the latter earlier
//! in the tick) are filled. Empty unless the program also uses type relations,
//! so each test forces the type-graph refresh with a `force_type` rule over
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
        "force_type(count(sym)) <- type_entity(_, sym, _, _, _, _, _).\n",
        "? force_type(n).\n",
        "? doc_ref(_, file, line, sym, kind, matched_name).\n",
    );
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "stderr: {err}\nstdout: {out}");
    // Engine heading at line 1 bridges to the Engine struct symbol.
    assert!(out.contains("docs.md\t1\t") && out.contains("Engine"),
        "Engine bridge row missing:\n{out}");
    // Widget heading at line 3 bridges to the Widget struct symbol.
    assert!(out.contains("docs.md\t3\t") && out.contains("Widget"),
        "Widget bridge row missing:\n{out}");
    // The kind column for a heading bridge is "heading".
    assert!(out.contains("heading"),
        "heading kind missing:\n{out}");
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
        "? doc_ref(_, file, line, sym, kind, matched_name).\n",
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

/// Heading normalization: `## The Engine struct` bridges to `Engine`, not just
/// exact-name headings. The original heading text is recorded as `matched_name`
/// (verbatim from the doc) so a rule can cross-reference `doc_node`.
#[test]
fn doc_ref_normalizes_heading_articles_and_kind_words() {
    let d = sandbox("norm");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("docs.md"), concat!(
        "# The Engine struct\n",
        "body\n",
        "# A Widget\n",
        "# fn do_thing\n",
    )).unwrap();
    fs::write(d.join("src/lib.rs"), concat!(
        "struct Engine;\n",
        "struct Widget;\n",
        "fn do_thing() {}\n",
    )).unwrap();
    let prog = concat!(
        "rel seen(path: file).\n",
        "seen(path) <- scan(\"WORK\", \"**/*.md\", path, rev), match(path, rev, /./, line).\n",
        "seen(path) <- scan(\"WORK\", \"**/*.rs\", path, rev), match(path, rev, /./, line).\n",
        "rel force_type(n: int).\n",
        "force_type(count(sym)) <- type_entity(_, sym, _, _, _, _, _).\n",
        "? force_type(n).\n",
        "? doc_ref(_, file, line, sym, kind, matched_name).\n",
    );
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "stderr: {err}\nstdout: {out}");
    // Three bridges: line 1 -> Engine, line 3 -> Widget, line 4 -> do_thing.
    assert!(out.contains("docs.md\t1\t") && out.contains("Engine"),
        "normalized Engine bridge missing:\n{out}");
    assert!(out.contains("docs.md\t3\t") && out.contains("Widget"),
        "normalized Widget bridge missing:\n{out}");
    assert!(out.contains("docs.md\t4\t") && out.contains("do_thing"),
        "trailing-fn bridge missing:\n{out}");
    // matched_name preserves the original heading text.
    assert!(out.contains("The Engine struct"),
        "matched_name must preserve original heading text:\n{out}");
}

/// Code-block content match: a fenced Rust block that mentions `Engine` and
/// `Widget` bridges to those symbols. `kind` = `code_block`, `matched_name` =
/// the identifier token from the code, and the bridge sits at the fence line.
/// Repeated mentions of the same symbol collapse to one row.
#[test]
fn doc_ref_bridges_code_block_mentions() {
    let d = sandbox("block");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("docs.md"), concat!(
        "# Examples\n",
        "```rs\n",
        "let e = Engine::new();\n",
        "let w = Widget::default();\n",
        "let e2 = Engine::clone();\n",
        "```\n",
    )).unwrap();
    fs::write(d.join("src/lib.rs"), concat!(
        "struct Engine;\n",
        "struct Widget;\n",
    )).unwrap();
    let prog = concat!(
        "rel seen(path: file).\n",
        "seen(path) <- scan(\"WORK\", \"**/*.md\", path, rev), match(path, rev, /./, line).\n",
        "seen(path) <- scan(\"WORK\", \"**/*.rs\", path, rev), match(path, rev, /./, line).\n",
        "rel force_type(n: int).\n",
        "force_type(count(sym)) <- type_entity(_, sym, _, _, _, _, _).\n",
        "? force_type(n).\n",
        "? doc_ref(_, file, line, sym, kind, matched_name).\n",
    );
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "stderr: {err}\nstdout: {out}");
    // code_block rows sit at the fence line (2); both symbols bridge.
    assert!(out.contains("code_block"),
        "code_block kind missing:\n{out}");
    assert!(out.contains("docs.md\t2\t"),
        "row not anchored at fence line:\n{out}");
    assert!(out.contains("Engine"),
        "Engine code-block bridge missing:\n{out}");
    assert!(out.contains("Widget"),
        "Widget code-block bridge missing:\n{out}");
    // The matched_name for a code-block row is the identifier token, lowercased
    // in the join but recorded verbatim from the source text.
    assert!(out.contains("\tEngine\t") || out.contains("\tEngine\n"),
        "matched_name token 'Engine' missing:\n{out}");
}
