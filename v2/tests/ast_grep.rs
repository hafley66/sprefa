//! ast[lang](pattern) op integration tests.

use v2::ops::default_registry;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use futures::executor::block_on;
use futures_util::stream::StreamExt;

use v2::{
    Config, DiagSink, EventSink, MemReader, MemWriter,
    OpCtx, OpId, OperatorRegistry, ProgramCtx, RunId,
    RuntimeConfig, host_parse, lower_rules,
};
use v2::_0_types::Cursor;

fn make_config() -> Arc<Config> {
    Arc::new(Config {
        repos:        vec![],
        revs:         vec![],
        fs_exclude:   vec![],
        sprf_files:   vec![],
        shell_allow:  vec![],
        runtime: RuntimeConfig {
            worker_threads:       1,
            buffer_size:          256,
            flush_interval_ms:    100,
            collect_witnesses:    true,
            xref_cartesian_limit: 10_000,
        },
        content_hash: 0,
    })
}

fn make_registry() -> Arc<OperatorRegistry> {
    Arc::new(default_registry())
}

fn run_rule(src: &str, reader: Arc<MemReader>) -> (Vec<Cursor>, Vec<String>) {
    let cfg = reader.config.clone();
    let invs = host_parse(src, Arc::from(PathBuf::from("<test>").as_path())).unwrap();
    let pctx = ProgramCtx::new(cfg.clone(), make_registry());
    let outcome = lower_rules(invs, pctx);
    assert!(outcome.diags.is_empty(),
        "lower diags: {:?}",
        outcome.diags.iter().map(|d| d.code()).collect::<Vec<_>>());

    let collected: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let collected_sink = collected.clone();

    let writer = Arc::new(MemWriter::new());
    let (result_store, xref_seen) = OpCtx::fresh_xref_state();
    let ctx = OpCtx {
        run_id: RunId(1),
        op_id:  OpId(0),
        reader: reader.clone(),
        writer,
        config: cfg.clone(),
        diags:  DiagSink(Arc::new(move |d| {
            collected_sink.lock().unwrap().push(d.code().to_owned());
        })),
        events: EventSink(Arc::new(|_| {})),
        result_store,
        xref_seen,
    };
    let empty: futures_core::stream::BoxStream<'static, Arc<[Cursor]>> =
        futures_util::stream::iter(Vec::<Arc<[Cursor]>>::new()).boxed();
    let batches: Vec<Arc<[Cursor]>> =
        block_on(outcome.pipelines[0].run(empty, ctx).collect());
    let cursors: Vec<Cursor> = batches.into_iter()
        .flat_map(|b| b.iter().cloned().collect::<Vec<_>>())
        .collect();
    let diag_codes = collected.lock().unwrap().clone();
    (cursors, diag_codes)
}

fn lower_diag_codes(src: &str) -> Vec<String> {
    let cfg = make_config();
    let invs = host_parse(src, Arc::from(PathBuf::from("<test>").as_path())).unwrap();
    let pctx = ProgramCtx::new(cfg, make_registry());
    let outcome = lower_rules(invs, pctx);
    outcome.diags.iter().map(|d| d.code().to_string()).collect()
}

// ── 1. native $NAME captures ─────────────────────────────────────────────────

#[test]
fn ast_rust_native_single_capture() {
    let cfg = make_config();
    let content = b"fn alpha() { let x = 1; }\nfn beta() { let y = 2; }\n";
    let reader = Arc::new(
        MemReader::new(cfg)
            .with_repo("r", &["main"])
            .with_files("r", "main", &["src/lib.rs"])
            .with_content("r", "main", "src/lib.rs", content),
    );

    let src = r#"rule(R){ > repo(r) > rev(main) > fs(**/lib.rs) > ast[rust](fn $NAME() { $$$BODY }) };"#;
    let (out, diags) = run_rule(src, reader);

    assert_eq!(out.len(), 2, "expected two fn matches, got {}: diags={:?}", out.len(), diags);
    let names: Vec<String> = out.iter()
        .map(|c| c.captures.get("NAME").unwrap().value.to_string())
        .collect();
    assert!(names.contains(&"alpha".to_string()), "names: {:?}", names);
    assert!(names.contains(&"beta".to_string()),  "names: {:?}", names);
    assert!(diags.is_empty(), "no runtime diags expected, got {:?}", diags);
}

// ── 2. native $$$VAR multi-node capture ──────────────────────────────────────

#[test]
fn ast_rust_native_multi_capture() {
    let cfg = make_config();
    let content = b"fn work() { let x = 1; let y = 2; }\n";
    let reader = Arc::new(
        MemReader::new(cfg)
            .with_repo("r", &["main"])
            .with_files("r", "main", &["src/lib.rs"])
            .with_content("r", "main", "src/lib.rs", content),
    );

    let src = r#"rule(R){ > repo(r) > rev(main) > fs(**/lib.rs) > ast[rust](fn $NAME() { $$$BODY }) };"#;
    let (out, _) = run_rule(src, reader);

    assert_eq!(out.len(), 1);
    assert_eq!(out[0].captures.get("NAME").unwrap().value.as_ref(), "work");
    let body = out[0].captures.get("BODY").expect("$$$BODY must bind").value.to_string();
    assert!(body.contains("let x = 1"), "body={body:?}");
    assert!(body.contains("let y = 2"), "body={body:?}");
}

// ── 3. ${VAR} sugar filters on surrounding token run ─────────────────────────

#[test]
fn ast_rust_sugar_filters_prefix() {
    let cfg = make_config();
    let content = b"fn get_user() { 1 }\nfn set_user() { 2 }\nfn get_repo() { 3 }\n";
    let reader = Arc::new(
        MemReader::new(cfg)
            .with_repo("r", &["main"])
            .with_files("r", "main", &["src/lib.rs"])
            .with_content("r", "main", "src/lib.rs", content),
    );

    let src = r#"rule(R){ > repo(r) > rev(main) > fs(**/lib.rs) > ast[rust](fn get_${SUFFIX}() { $$$B }) };"#;
    let (out, diags) = run_rule(src, reader);

    assert_eq!(out.len(), 2,
        "sugar must filter to get_* fns only; got {} cursors, diags={:?}",
        out.len(), diags);
    let suffixes: Vec<String> = out.iter()
        .map(|c| c.captures.get("SUFFIX").unwrap().value.to_string())
        .collect();
    assert!(suffixes.contains(&"user".to_string()), "suffixes: {:?}", suffixes);
    assert!(suffixes.contains(&"repo".to_string()), "suffixes: {:?}", suffixes);
    assert!(!suffixes.iter().any(|s| s == "set_user"),
        "set_user must be filtered by sugar regex; suffixes={:?}", suffixes);
}

// ── 4. byte_range narrows to matched node ────────────────────────────────────

#[test]
fn ast_byte_range_narrows_to_match() {
    let cfg = make_config();
    let content = b"// head\nfn alpha() { let x = 1; }\n";
    let reader = Arc::new(
        MemReader::new(cfg)
            .with_repo("r", &["main"])
            .with_files("r", "main", &["src/lib.rs"])
            .with_content("r", "main", "src/lib.rs", content),
    );

    let src = r#"rule(R){ > repo(r) > rev(main) > fs(**/lib.rs) > ast[rust](fn $NAME() { $$$B }) };"#;
    let (out, _) = run_rule(src, reader);

    assert_eq!(out.len(), 1);
    let r = out[0].byte_range.clone().expect("byte_range must be set");
    let matched = &content[r.clone()];
    assert_eq!(matched, b"fn alpha() { let x = 1; }",
        "byte_range must carve out the full fn; got {:?}", std::str::from_utf8(matched));
    let ev = out[0].evidence.last().expect("ast must append evidence");
    assert_eq!(ev.op_name, "ast");
    assert_eq!(ev.matched.as_ref(), "fn alpha() { let x = 1; }");
}

// ── 5. content contract — rebased sub-content via json > &.$C > ast ─────────

#[test]
fn ast_content_contract_after_rebase() {
    // Outer file is JSON. json() binds $SRC to the unescaped code string, `&.$SRC`
    // rebases cursor content to those bytes, and ast[rust] must parse the rebased
    // content — NOT the raw JSON bytes.
    let cfg = make_config();
    let json_body = br#"{"code":"fn gamma() { let z = 1; }"}"#;
    let reader = Arc::new(
        MemReader::new(cfg)
            .with_repo("r", &["main"])
            .with_files("r", "main", &["snippets.json"])
            .with_content("r", "main", "snippets.json", json_body),
    );

    let src = r#"rule(R){ > repo(r) > rev(main) > fs(**/snippets.json) > json({code:$SRC}) > &.$SRC > ast[rust](fn $NAME() { $$$B }) };"#;
    let (out, diags) = run_rule(src, reader);

    assert_eq!(out.len(), 1,
        "ast must match one fn in rebased content; cursors={} diags={:?}",
        out.len(), diags);
    assert_eq!(out[0].captures.get("NAME").unwrap().value.as_ref(), "gamma",
        "NAME must come from rebased content, proving PATH B contract");
    let ev = out[0].evidence.last().unwrap();
    assert_eq!(ev.op_name, "ast");
    assert_eq!(ev.matched.as_ref(), "fn gamma() { let z = 1; }",
        "matched evidence proves ast read cursor.content, not reader bytes");
}

// ── 6. content preservation for chaining ─────────────────────────────────────

#[test]
fn ast_preserves_content_for_chaining() {
    let cfg = make_config();
    let content = b"fn only() { let x = 1; }\n";
    let reader = Arc::new(
        MemReader::new(cfg)
            .with_repo("r", &["main"])
            .with_files("r", "main", &["src/lib.rs"])
            .with_content("r", "main", "src/lib.rs", content),
    );

    let src = r#"rule(R){ > repo(r) > rev(main) > fs(**/lib.rs) > ast[rust](fn $NAME() { $$$B }) };"#;
    let (out, _) = run_rule(src, reader);

    assert_eq!(out.len(), 1);
    let preserved = out[0].content.as_ref().expect("content must be preserved");
    assert_eq!(preserved.as_ref().as_ref(), content,
        "content must be bit-identical to source bytes after ast op");
}

// ── 7. no-match diagnostic ───────────────────────────────────────────────────

#[test]
fn ast_no_match_emits_diagnostic() {
    let cfg = make_config();
    let content = b"struct S { x: i32 }\n";
    let reader = Arc::new(
        MemReader::new(cfg)
            .with_repo("r", &["main"])
            .with_files("r", "main", &["src/lib.rs"])
            .with_content("r", "main", "src/lib.rs", content),
    );

    let src = r#"rule(R){ > repo(r) > rev(main) > fs(**/lib.rs) > ast[rust](fn $NAME() { $$$B }) };"#;
    let (out, diags) = run_rule(src, reader);
    assert_eq!(out.len(), 0);
    assert!(diags.contains(&"ast/no-match".to_string()),
        "expected ast/no-match, got {:?}", diags);
}

// ── 8. missing lang diagnostic ───────────────────────────────────────────────

#[test]
fn ast_missing_lang_diagnostic() {
    let src = r#"rule(R){ > repo(r) > rev(main) > ast(fn $NAME() {}) };"#;
    let codes = lower_diag_codes(src);
    assert!(codes.iter().any(|c| c == "ast/missing-lang"),
        "expected ast/missing-lang, got {:?}", codes);
}

// ── 9. unknown lang diagnostic ───────────────────────────────────────────────

#[test]
fn ast_unknown_lang_diagnostic() {
    let src = r#"rule(R){ > repo(r) > rev(main) > ast[cobol](fn $NAME() {}) };"#;
    let codes = lower_diag_codes(src);
    assert!(codes.iter().any(|c| c == "ast/unknown-lang"),
        "expected ast/unknown-lang, got {:?}", codes);
}

// ── 10. bad pattern diagnostic ───────────────────────────────────────────────

#[test]
fn ast_bad_pattern_empty_paren_diagnostic() {
    let src = r#"rule(R){ > repo(r) > rev(main) > ast[rust]() };"#;
    let codes = lower_diag_codes(src);
    assert!(codes.iter().any(|c| c == "ast/bad-pattern"),
        "expected ast/bad-pattern on empty paren, got {:?}", codes);
}

// ── 11. typescript happy path ────────────────────────────────────────────────

#[test]
fn ast_typescript_hook_query_shape() {
    // The motivating use case: find `use${NAME}Query()` call sites in TS.
    let cfg = make_config();
    let content = b"const a = useUserQuery();\nconst b = useRepoQuery();\nconst c = usePlainFn();\n";
    let reader = Arc::new(
        MemReader::new(cfg)
            .with_repo("r", &["main"])
            .with_files("r", "main", &["hooks.ts"])
            .with_content("r", "main", "hooks.ts", content),
    );

    let src = r#"rule(R){ > repo(r) > rev(main) > fs(**/hooks.ts) > ast[typescript](use${NAME}Query()) };"#;
    let (out, diags) = run_rule(src, reader);

    assert_eq!(out.len(), 2,
        "sugar must match only use*Query call sites; got {} diags={:?}",
        out.len(), diags);
    let names: Vec<String> = out.iter()
        .map(|c| c.captures.get("NAME").unwrap().value.to_string())
        .collect();
    assert!(names.contains(&"User".to_string()), "names: {:?}", names);
    assert!(names.contains(&"Repo".to_string()), "names: {:?}", names);
}
