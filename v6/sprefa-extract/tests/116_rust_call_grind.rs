// TEST: the rust CALL family, SYNTAX tier, ground against the three call
// oracles (RATCHET.tsv `rust call syntax *`). Each fixture test below pins
// one wrong-callee or missed-callee mechanism the census over
// `/Users/chrishafley/projects/rust-analyzer` surfaced; the dump test is the
// census input.
//
// FAIL-FIRST receipts sit on each test.

mod bench;

use std::path::PathBuf;

use sprefa_extract::{resolve_project, FlatFact, ResolveArms, ResolveRequest};

/// `RUST_SYNTAX_CALL_DUMP=<path>`: our SYNTAX-tier call rows over the corpus,
/// five columns (the four of the normal form plus the origin set, `|`-joined),
/// the input of `rust.call_census.py` and the precision census.
#[test]
#[ignore = "local corpora only; run with RUST_SYNTAX_CALL_DUMP=<path>"]
fn dump_rust_syntax_call_rows() {
    let Ok(out) = std::env::var("RUST_SYNTAX_CALL_DUMP") else {
        panic!("set RUST_SYNTAX_CALL_DUMP=<path>");
    };
    let corpus = bench::corpus("rust");
    assert!(corpus.root.is_dir(), "corpus root {} missing", corpus.root.display());
    let measurement = bench::run("rust", bench::Tier::Syntax);
    let mut body = Vec::with_capacity(measurement.forms.call.len());
    for row in &measurement.forms.call {
        let origins: Vec<&str> = measurement
            .forms
            .call_origins
            .get(row)
            .map(|set| set.iter().map(String::as_str).collect())
            .unwrap_or_default();
        body.push(format!("{row}\t{}", origins.join("|")));
    }
    std::fs::write(&out, body.join("\n") + "\n").unwrap();
    println!("wrote {} call rows to {out}", body.len());
}

fn fixture(name: &str) -> PathBuf {
    let mut path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf();
    path.push("tests/fixtures/rust_call_grind");
    path.push(name);
    path
}

/// (caller file, caller name, callee file, callee name) of every resolved
/// call row over the named fixtures, syntax tier.
fn rows(names: &[&str]) -> Vec<(String, String, String, String)> {
    let paths: Vec<PathBuf> = names.iter().map(|name| fixture(name)).collect();
    let facts = resolve_project(&ResolveRequest {
        paths: &paths,
        arms: ResolveArms { call: true, types: false, flow: false },
        scip: Default::default(),
        project_root: None,
        scip_records: Default::default(),
        occurrence_text: false,
        rust_checker: None,
        ts_checker: None,
        witness: false,
    })
    .expect("the fixture corpus resolves");
    let leaf = |path: &str| path.rsplit('/').next().unwrap_or(path).to_string();
    facts
        .into_iter()
        .filter_map(|fact| match fact {
            FlatFact::ResolvedEdge {
                caller_path,
                caller_name,
                callee_path,
                callee_name,
                ..
            } => Some((
                leaf(&caller_path),
                caller_name.unwrap_or_default(),
                leaf(&callee_path),
                callee_name.unwrap_or_default(),
            )),
            _ => None,
        })
        .collect()
}

#[allow(dead_code)]
fn has(rows: &[(String, String, String, String)], caller: &str, callee_file: &str, callee: &str) -> bool {
    rows.iter()
        .any(|(_, c, file, name)| c == caller && file == callee_file && name == callee)
}

// FAIL-FIRST (origin/main 1b2464c9b): `Shape::Circle { radius: 1 }` minted a
// call site that `variant_ctor_target` bound to the `Circle` variant def,
// so `build_circle -> Circle` was emitted; no rust call oracle scores a
// struct-variant literal as a call (census: ra 16 matched / 431 contradicted,
// scip 4 / 328, codeql 0 / 489 over the corpus).
#[test]
fn variant_literal_mints_no_call_row() {
    let rows = rows(&["shapes.rs"]);
    assert!(
        !has(&rows, "build_circle", "shapes.rs", "Circle"),
        "a struct-variant literal is not a call: {rows:?}"
    );
}

/// A plain struct literal keeps its row: raw scip scores 888 of them and ra
/// 388 over the corpus, so the literal-vs-call line is drawn at the variant.
#[test]
fn struct_literal_keeps_its_row() {
    let rows = rows(&["shapes.rs"]);
    assert!(has(&rows, "origin", "shapes.rs", "Point"), "{rows:?}");
}

// FAIL-FIRST (origin/main 1b2464c9b): `let widget = Widget::new()` in a file
// that declares no `new` left `widget` untyped, so `widget.tick()` fell to the
// corpus-wide name match and dropped `ambiguous` (decoy.rs also declares
// `tick`).
#[test]
fn cross_file_new_types_the_binding() {
    let rows = rows(&["widget.rs", "user.rs", "decoy.rs"]);
    assert!(
        has(&rows, "cross_file_new_caller", "widget.rs", "tick"),
        "{rows:?}"
    );
}

// FAIL-FIRST (origin/main 1b2464c9b): a call-result receiver
// (`Widget::new().tick()`) and a method-call initializer
// (`let gauge = Widget::new().tick()`) were both `Inferred`, so `tick` and
// `read` dropped against the decoy's same-named methods.
#[test]
fn call_result_receiver_types_through_same_file_returns() {
    let rows = rows(&["widget.rs", "decoy.rs"]);
    assert!(has(&rows, "chain_caller", "widget.rs", "tick"), "{rows:?}");
    assert!(has(&rows, "chain_caller", "widget.rs", "read"), "{rows:?}");
}

#[test]
fn method_init_hops_through_the_receiver_type() {
    let rows = rows(&["widget.rs", "decoy.rs"]);
    assert!(has(&rows, "hop_caller", "widget.rs", "tick"), "{rows:?}");
    assert!(has(&rows, "hop_caller", "widget.rs", "read"), "{rows:?}");
}
