//! The Rust normal form (`tests/bench/mod.rs`) must agree with
//! `plans/extract-bench-2026-08-29/normalize.py`, which stays the reference
//! implementation. Two cases: a synthetic one that runs everywhere and pins
//! the column layout, and the go-corpus one that runs BOTH implementations
//! over the same raw resolve output and diffs them.

mod bench;

use std::path::Path;

use sprefa_extract::FlatFact;

fn edge(caller_path: &str, caller_name: Option<&str>, callee_path: &str, callee_name: Option<&str>) -> FlatFact {
    FlatFact::ResolvedEdge {
        caller_path: caller_path.to_string(),
        caller_name: caller_name.map(String::from),
        callee_path: callee_path.to_string(),
        callee_name: callee_name.map(String::from),
        caller_site_start: 0,
        caller_site_end: 0,
        kind: "name_resolve".to_string(),
        resolution_origin: "corpus_unique".to_string(),
    }
}

fn type_edge(owner_path: &str, owner_name: Option<&str>, target_path: &str, target_name: Option<&str>) -> FlatFact {
    FlatFact::ResolvedTypeEdge {
        owner_path: owner_path.to_string(),
        owner_name: owner_name.map(String::from),
        owner_start: 0,
        owner_end: 0,
        target_path: target_path.to_string(),
        target_name: target_name.map(String::from),
        kind: "uses".to_string(),
        resolution_origin: "corpus_unique".to_string(),
    }
}

fn import(src_path: &str, target_path: &str) -> FlatFact {
    FlatFact::ResolvedImportRow {
        src_path: src_path.to_string(),
        name: "name".to_string(),
        local: "local".to_string(),
        target_path: target_path.to_string(),
        target_name: None,
        kind: "local".to_string(),
        hops: 0,
    }
}

#[test]
fn normal_form_rows_match_the_python_column_layout() {
    // The root as normalize.py receives it: absolute, no trailing slash.
    let root = Path::new("/corpus/root");
    let facts = vec![
        edge(
            "/corpus/root/src/a.ts",
            Some("caller"),
            "/corpus/root/src/b.ts",
            Some("callee"),
        ),
        // A null name renders as the empty column (`d.get(...) or ""`).
        edge("/corpus/root/src/a.ts", None, "/corpus/root/src/b.ts", None),
        // A path outside the root passes through untouched (relp's else arm).
        edge(
            "/corpus/root/src/a.ts",
            Some("caller"),
            "/elsewhere/c.ts",
            Some("callee"),
        ),
        type_edge(
            "/corpus/root/src/a.ts",
            Some("Owner"),
            "/corpus/root/src/b.ts",
            Some("Target"),
        ),
        type_edge("/corpus/root/src/a.ts", None, "/corpus/root/src/b.ts", None),
        import("/corpus/root/src/a.ts", "/corpus/root/src/b.ts"),
        import("/corpus/root/src/a.ts", "/corpus/root/src/b.ts"),
    ];
    let forms = bench::normal_form(root, &facts);
    let rows: Vec<&str> = forms.call.iter().map(String::as_str).collect();
    assert_eq!(
        rows,
        vec![
            "src/a.ts\t\tsrc/b.ts\t",
            "src/a.ts\tcaller\t/elsewhere/c.ts\tcallee",
            "src/a.ts\tcaller\tsrc/b.ts\tcallee",
        ]
    );
    let rows: Vec<&str> = forms.type_edges.iter().map(String::as_str).collect();
    assert_eq!(
        rows,
        vec![
            "src/a.ts\t\tsrc/b.ts\t",
            "src/a.ts\tOwner\tsrc/b.ts\tTarget",
        ]
    );
    // normalize.py drops the name/local/kind/hops columns entirely and dedups.
    let rows: Vec<&str> = forms.module.iter().map(String::as_str).collect();
    assert_eq!(rows, vec!["src/a.ts\t\tsrc/b.ts\t"]);
}

/// The two implementations over the SAME raw resolve output: the go corpus
/// resolved in-process (the ratchet's own path, serialized to JSONL the way
/// the CLI prints it), then `normalize.py resolved` over that file, then both
/// call and type tsvs must be byte-equal to the Rust fold. This is the
/// agreement the committed `go.parse.call.tsv` chain rests on. Also prints
/// the row delta against that committed tsv, informational: it is a
/// #565-era binary's output, and drift there is the ratchet's job to price,
/// not this test's.
#[test]
#[ignore = "local corpora only; run via `just extract-ratchet` (or directly with --ignored)"]
fn rust_normal_form_agrees_with_normalize_py_over_the_go_corpus() {
    let corpus = bench::corpus("go");
    if !corpus.root.is_dir() {
        println!("parity: absent (corpus root {} missing), skipped", corpus.root.display());
        return;
    }
    let files = bench::enumerate(&corpus);
    let facts = sprefa_extract::resolve_project(&sprefa_extract::ResolveRequest {
        paths: &files,
        arms: sprefa_extract::ResolveArms {
            call: true,
            types: true,
            flow: false,
        },
        scip: sprefa_extract::ScipMode::Off,
        project_root: None,
        scip_records: sprefa_extract::ScipRecords::all(),
        occurrence_text: false,
        rust_checker: None,
    })
    .expect("go corpus resolves");
    let scratch = std::env::temp_dir().join("extract_ratchet_parity");
    std::fs::create_dir_all(&scratch).expect("create scratch dir");
    let raw = scratch.join("go.raw.jsonl");
    let mut text = String::new();
    for fact in &facts {
        text.push_str(&serde_json::to_string(fact).expect("fact serializes"));
        text.push('\n');
    }
    std::fs::write(&raw, text).expect("write raw jsonl");
    let call_out = scratch.join("go.py.call.tsv");
    let type_out = scratch.join("go.py.type.tsv");
    let status = std::process::Command::new("python3")
        .arg("normalize.py")
        .arg("resolved")
        .arg(&raw)
        .arg(&corpus.root)
        .arg(&call_out)
        .arg(&type_out)
        .current_dir(bench::BENCH_DIR)
        .status()
        .expect("python3 runs normalize.py");
    assert!(status.success(), "normalize.py exited {status}");

    let forms = bench::normal_form(&corpus.root, &facts);
    for (name, ours, theirs) in [
        ("call", &forms.call, &call_out),
        ("type", &forms.type_edges, &type_out),
    ] {
        let theirs = bench::load_tsv(theirs);
        let only_rust: Vec<&String> = ours.difference(&theirs).collect();
        let only_python: Vec<&String> = theirs.difference(ours).collect();
        assert!(
            only_rust.is_empty() && only_python.is_empty(),
            "parity {name}: {} rows only in the Rust fold, {} only in normalize.py; first Rust-only {:?}, first python-only {:?}",
            only_rust.len(),
            only_python.len(),
            only_rust.first(),
            only_python.first(),
        );
        println!("parity {name}: {} rows agree", ours.len());
    }

    let committed = Path::new(bench::BENCH_DIR).join("go.parse.call.tsv");
    if committed.is_file() {
        let committed = bench::load_tsv(&committed);
        let gained = forms.call.difference(&committed).count();
        let lost = committed.difference(&forms.call).count();
        println!(
            "parity vs committed go.parse.call.tsv ({} rows): +{gained} new, -{lost} gone",
            committed.len()
        );
    }
}
