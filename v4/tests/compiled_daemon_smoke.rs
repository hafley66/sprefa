use std::path::PathBuf;

use v4::compile::rust_daemon::{
    compile_rust_daemon, emit_rust_daemon, emit_rust_daemon_source, RustDaemonSpec,
};

fn spec(name: &str) -> RustDaemonSpec {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    RustDaemonSpec {
        sprf_path: manifest_dir.join("examples").join(name),
        root: PathBuf::from("."),
        bind: "127.0.0.1:0".to_string(),
        fact_db: None,
        queue_db: None,
    }
}

#[test]
fn compiled_daemon_source_lowers_str_rule_to_concrete_components() {
    let src = include_str!("../examples/str-rule.sprf");
    let rendered = emit_rust_daemon_source(src, &spec("str-rule.sprf")).unwrap();

    assert!(
        rendered.contains("StrConstComponent"),
        "generated source should construct str component:\n{rendered}"
    );
    assert!(
        rendered.contains("Term::bind(Arc::<str>::from(\"MSG\"))"),
        "generated source should construct term bind:\n{rendered}"
    );
    assert!(
        rendered.contains("FactWrite::projected"),
        "generated source should construct projected fact write:\n{rendered}"
    );
    assert!(
        !rendered.contains("host_parse"),
        "generated daemon must not parse sprf at startup:\n{rendered}"
    );
    assert!(
        !rendered.contains("default_registry"),
        "generated daemon must not use registry dispatch:\n{rendered}"
    );
}

#[test]
fn compiled_daemon_source_lowers_json_and_rule_query_paths() {
    let json = emit_rust_daemon_source(
        include_str!("../examples/json-extract.sprf"),
        &spec("json-extract.sprf"),
    )
    .unwrap();
    assert!(
        json.contains("JsonDsl::compile_typed"),
        "generated source should compile json DSL directly:\n{json}"
    );
    assert!(
        json.contains("JsonComponent::new"),
        "generated source should construct json component:\n{json}"
    );

    let rules = emit_rust_daemon_source(
        include_str!("../examples/rule-sink-fact.sprf"),
        &spec("rule-sink-fact.sprf"),
    )
    .unwrap();
    assert!(
        rules.contains("SqlQueryComponent::with_referenced_tables"),
        "generated source should construct rule query SQL component:\n{rules}"
    );
    assert!(
        rules.contains("SELECT input.__cursor_idx, __rule.WORD AS WORD\\nFROM input JOIN words AS __rule ON 1=1"),
        "generated source should embed the direct SQL query:\n{rules}"
    );
}

#[test]
fn compiled_daemon_source_rejects_unsupported_ops_with_compile_error() {
    let rendered = emit_rust_daemon_source("repo(:x);\n", &spec("unsupported.sprf")).unwrap();
    assert!(
        rendered.contains("compile_error!(\"op `repo` has no direct Rust emitter yet\")"),
        "unsupported ops should become an explicit Rust compile error:\n{rendered}"
    );
}

#[test]
#[ignore = "builds a nested generated release crate"]
fn compiled_daemon_crate_builds_for_str_rule() {
    let artifact = emit_rust_daemon(&spec("str-rule.sprf")).unwrap();
    let bin = compile_rust_daemon(&artifact).unwrap();
    assert!(bin.exists(), "compiled daemon binary missing at {bin:?}");
}
