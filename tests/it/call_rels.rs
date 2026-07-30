//! Phase D: the CALL_RELS built-in family (call_def / call_site / call_edge /
//! call_edge_rev) wired as a lazy indexer alongside TYPE_RELS. The Rust
//! extractor (syn) is live; Kotlin and TS still use the TypeLang default
//! (empty CallFacts) until their extract_calls bodies land.
//!
//! Tests: (1) the four relations are reserved, (2) a language with no
//! call extractor keeps the wiring live with zero rows, (3) the Rust
//! extractor + the engine's caller/callee resolution pass produce a real
//! resolved call graph and closure(call_edge) walks it transitively.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("call_rels_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(dir: &Path, prog: &str, extra: &[&str]) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .current_dir(dir)
        .args(["--db", dir.join("db").to_str().unwrap()])
        .args(extra)
        .output()
        .expect("run dl");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// The CALL_RELS are reserved: a program that declares one by hand errors
/// out, matching the TYPE_RELS / MODULE_RELS contract.
#[test]
fn call_rels_are_reserved() {
    let d = sandbox("reserved");
    for rel in [
        "call_def",
        "call_site",
        "call_edge",
        "call_edge_rev",
        "call_name",
        "call_kind",
    ] {
        let prog = format!("rel {rel}(a: text).\n? {rel}(\"x\").\n");
        let (code, _out, err) = run(&d, &prog, &[]);
        assert_ne!(code, 0, "{rel} must be reserved (expected error):\n{err}");
        assert!(
            err.contains("reserved-name")
                && err.contains(&format!("relation `{rel}`"))
                && err.contains("pick another name"),
            "{rel} parse-tier reservation/fix missing:\n{err}"
        );
    }
}

/// A corpus with no callable source files keeps the wiring live: the lazy
/// indexer runs, the relations are queryable and empty, and closure(call_edge)
/// is a legal (empty) edge rel. The no-op gate (Rust/Kotlin/TS all implement
/// extract_calls now; this exercises the genuinely-empty path).
#[test]
fn empty_call_extractors_keep_wiring_live() {
    let d = sandbox("empty");
    fs::create_dir_all(d.join("src")).unwrap();
    // a non-callable source file: scanned, but no language extracts calls from it.
    fs::write(d.join("src/notes.txt"), "no callables here\n").unwrap();
    let prog = concat!(
        "rel seen(path: file).\n",
        "seen(path) <- scan(\"WORK\", \"src/**/*.txt\", path, rev), match(path, rev, /./, line).\n",
        "rel reaches(a: text, b: text).\n",
        "reaches(a, b) <- closure(call_edge).\n",
        "? call_def(_, sym, kind, file, line, end).\n",
        "? call_site(_, caller, callee, file, line).\n",
        "? reaches(a, b).\n",
    );
    let (code, out, err) = run(&d, prog, &[]);
    assert_eq!(code, 0, "empty extractors must not error:\n{err}");
    assert!(
        out.contains("(0 rows)"),
        "expected zero-row footers:\n{out}"
    );
    assert!(
        !out.contains("(1 rows)") && !out.contains("(2 rows)"),
        "empty corpus produced call rows:\n{out}"
    );
}

/// The Phase D gate: the Rust extractor emits call defs with body spans, the
/// engine's resolution pass attaches each call site to its enclosing def and
/// resolves bare callees to def syms, and closure(call_edge) walks the result
/// transitively. `main -> helper -> leaf` must reach `main -> leaf`.
#[test]
fn rust_call_graph_extracts_resolves_and_closes() {
    let d = sandbox("rust");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(
        d.join("src/lib.rs"),
        "fn main() { helper(1); }\nfn helper(x: i32) { leaf(x); }\nfn leaf(x: i32) {}\n",
    )
    .unwrap();
    let prog = concat!(
        "rel seen(path: file).\n",
        "seen(path) <- scan(\"WORK\", \"src/**/*.rs\", path, rev), match(path, rev, /./, line).\n",
        "rel reaches(a: text, b: text).\n",
        "reaches(a, b) <- closure(call_edge).\n",
        "? call_def(_, sym, kind, file, line, end).\n",
        "? reaches(a, b).\n",
    );
    let (code, out, err) = run(&d, prog, &[]);
    assert_eq!(code, 0, "Rust extraction must not error:\n{err}");

    // Syms are repo-qualified (364de80); the repo slug is the sandbox dir name.
    let repo = d.file_name().unwrap().to_str().unwrap();
    let main = format!("{repo}::src/lib.rs::function::main");
    let helper = format!("{repo}::src/lib.rs::function::helper");
    let leaf = format!("{repo}::src/lib.rs::function::leaf");

    // call_def: all three callables present.
    assert!(out.contains(&main), "main def missing:\n{out}");
    assert!(out.contains(&helper), "helper def missing:\n{out}");
    assert!(out.contains(&leaf), "leaf def missing:\n{out}");
    assert!(
        out.contains("(3 rows)"),
        "expected exactly 3 call_def rows:\n{out}"
    );

    // closure(call_edge): direct main -> helper, plus transitive main -> leaf.
    assert!(
        out.contains(&format!("{main}\t{helper}")),
        "direct edge main -> helper missing from closure:\n{out}"
    );
    assert!(
        out.contains(&format!("{main}\t{leaf}")),
        "transitive reach main -> leaf missing from closure:\n{out}"
    );
}

/// Kotlin parity: the tree-sitter extractor emits defs with body spans and call
/// sites, the engine resolves and closes them just like Rust. `main -> helper ->
/// leaf` must reach `main -> leaf`, and all three defs are present.
#[test]
fn kotlin_call_graph_extracts_resolves_and_closes() {
    let d = sandbox("kotlin");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(
        d.join("src/lib.kt"),
        "fun main() { helper() }\nfun helper() { leaf() }\nfun leaf() {}\n",
    )
    .unwrap();
    let prog = concat!(
        "rel seen(path: file).\n",
        "seen(path) <- scan(\"WORK\", \"src/**/*.kt\", path, rev), match(path, rev, /./, line).\n",
        "rel reaches(a: text, b: text).\n",
        "reaches(a, b) <- closure(call_edge).\n",
        "? call_def(_, sym, kind, file, line, end).\n",
        "? reaches(a, b).\n",
    );
    let (code, out, err) = run(&d, prog, &[]);
    assert_eq!(code, 0, "Kotlin extraction must not error:\n{err}");

    let repo = d.file_name().unwrap().to_str().unwrap();
    let main = format!("{repo}::src/lib.kt::function::main");
    let helper = format!("{repo}::src/lib.kt::function::helper");
    let leaf = format!("{repo}::src/lib.kt::function::leaf");

    assert!(out.contains(&main), "main def missing:\n{out}");
    assert!(out.contains(&helper), "helper def missing:\n{out}");
    assert!(out.contains(&leaf), "leaf def missing:\n{out}");
    assert!(
        out.contains("(3 rows)"),
        "expected exactly 3 call_def rows:\n{out}"
    );

    assert!(
        out.contains(&format!("{main}\t{helper}")),
        "direct edge main -> helper missing from closure:\n{out}"
    );
    assert!(
        out.contains(&format!("{main}\t{leaf}")),
        "transitive reach main -> leaf missing from closure:\n{out}"
    );
}

/// TypeScript parity: the oxc extractor emits defs with body spans and call
/// sites, the engine resolves and closes them just like Rust. `main -> helper ->
/// leaf` must reach `main -> leaf`, and all three defs are present.
#[test]
fn ts_call_graph_extracts_resolves_and_closes() {
    let d = sandbox("ts");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(
        d.join("src/lib.ts"),
        "function main() { helper(); }\nfunction helper() { leaf(); }\nfunction leaf() {}\n",
    )
    .unwrap();
    let prog = concat!(
        "rel seen(path: file).\n",
        "seen(path) <- scan(\"WORK\", \"src/**/*.ts\", path, rev), match(path, rev, /./, line).\n",
        "rel reaches(a: text, b: text).\n",
        "reaches(a, b) <- closure(call_edge).\n",
        "? call_def(_, sym, kind, file, line, end).\n",
        "? reaches(a, b).\n",
    );
    let (code, out, err) = run(&d, prog, &[]);
    assert_eq!(code, 0, "TS extraction must not error:\n{err}");

    let repo = d.file_name().unwrap().to_str().unwrap();
    let main = format!("{repo}::src/lib.ts::function::main");
    let helper = format!("{repo}::src/lib.ts::function::helper");
    let leaf = format!("{repo}::src/lib.ts::function::leaf");

    assert!(out.contains(&main), "main def missing:\n{out}");
    assert!(out.contains(&helper), "helper def missing:\n{out}");
    assert!(out.contains(&leaf), "leaf def missing:\n{out}");
    assert!(
        out.contains("(3 rows)"),
        "expected exactly 3 call_def rows:\n{out}"
    );

    assert!(
        out.contains(&format!("{main}\t{helper}")),
        "direct edge main -> helper missing from closure:\n{out}"
    );
    assert!(
        out.contains(&format!("{main}\t{leaf}")),
        "transitive reach main -> leaf missing from closure:\n{out}"
    );
}

/// `call_kind(fn, kind)` classifies each fn by the read/write shape of its
/// callees: bare method name -> kind (execute/insert -> write; prepare/
/// query_row/query_map -> read; everything else -> no row). One row per
/// (fn, kind) pair, so a fn with both a write and a read call emits two rows.
/// Drives the conn-loop-reachable rail's precision cut (read-only fns stay
/// silent).
#[test]
fn call_kind_classifies_read_and_write() {
    let d = sandbox("kind");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(
        d.join("src/lib.rs"),
        [
            "fn writer() { let c = conn(); c.execute(\"INSERT\", ()); }",
            "fn reader() { let c = conn(); c.query_row(\"SELECT\", (), |_| 1); }",
            "fn mixed() { let c = conn(); c.execute(\"A\", ()); c.query_row(\"B\", (), |_| 1); }",
            "fn neither() { helper(); }",
            "fn conn() -> i32 { 0 }",
            "fn helper() {}",
        ]
        .join("\n"),
    )
    .unwrap();
    let prog = concat!(
        "rel seen(path: file).\n",
        "seen(path) <- scan(\"WORK\", \"src/**/*.rs\", path, rev), match(path, rev, /./, line).\n",
        "? call_kind(fn, kind).\n",
    );
    let (code, out, err) = run(&d, prog, &[]);
    assert_eq!(code, 0, "call_kind extraction must not error:\n{err}");

    let writer = "src/lib.rs::function::writer";
    let reader = "src/lib.rs::function::reader";
    let mixed = "src/lib.rs::function::mixed";

    // writer -> write, reader -> read, mixed -> both.
    assert!(
        out.contains(&format!("{writer}\twrite")),
        "writer must classify as write:\n{out}"
    );
    assert!(
        out.contains(&format!("{reader}\tread")),
        "reader must classify as read:\n{out}"
    );
    assert!(
        out.contains(&format!("{mixed}\twrite")) && out.contains(&format!("{mixed}\tread")),
        "mixed must classify as both write and read:\n{out}"
    );

    // neither / conn / helper have no classified calls -> no rows.
    assert!(
        !out.contains("neither") && !out.contains("helper"),
        "fns with no classified calls must have no call_kind row:\n{out}"
    );

    // Exactly 4 rows: writer:write, reader:read, mixed:write, mixed:read.
    assert!(
        out.contains("(4 rows)"),
        "expected exactly 4 call_kind rows:\n{out}"
    );
}
