//! marker() op integration tests.

use v2::ops::default_registry;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use futures::executor::block_on;
use futures_util::stream::StreamExt;

use v2::{
    Config, DiagSink, MemReader, MemWriter,
    OpCtx, OperatorRegistry, ProgramCtx, host_parse, lower_rules,
};
use v2::_0_types::Cursor;

fn make_config() -> Arc<Config> {
    Arc::new(Config::test_default())
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
    let ctx = OpCtx {
        diags: DiagSink(Arc::new(move |d| {
            collected_sink.lock().unwrap().push(d.code().to_owned());
        })),
        ..OpCtx::for_test(cfg.clone(), reader.clone(), writer)
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

// ── 1. Happy path — paired open/close narrows to region ──────────────────────

#[test]
fn marker_paired_single_region() {
    let cfg = make_config();
    let content = b"// BEGIN foo\nfn login() {}\n// END foo\nfn other() {}\n";
    let reader = Arc::new(
        MemReader::new(cfg)
            .with_repo("r", &["main"])
            .with_files("r", "main", &["src/lib.rs"])
            .with_content("r", "main", "src/lib.rs", content),
    );

    let src = r#"rule(R){ > repo(r) > rev(main) > fs(**/lib.rs) > marker("BEGIN", "END") };"#;
    let (out, diags) = run_rule(src, reader);

    assert_eq!(out.len(), 1, "expected 1 region, got {}: diags={:?}", out.len(), diags);
    assert!(diags.is_empty(), "{:?}", diags);

    let r = out[0].byte_range.clone().expect("byte_range must be set");
    let region_bytes = &content[r.clone()];
    let region_str = std::str::from_utf8(region_bytes).unwrap();
    assert!(region_str.contains("login"),     "region must include body: {region_str:?}");
    assert!(region_str.contains("BEGIN foo"), "region must include open marker line: {region_str:?}");
    assert!(region_str.contains("END foo"),   "region must include close marker line: {region_str:?}");
    assert!(!region_str.contains("other"),    "region must not extend past END: {region_str:?}");

    let ev = out[0].evidence.last().expect("marker must append evidence");
    assert_eq!(ev.op_name, "marker");
    assert_eq!(ev.matched.as_ref(), "// BEGIN foo",
        "evidence.matched must be the opening comment line");
}

// ── 2. Capture binding — third arg names a capture for the marker label text ─

#[test]
fn marker_three_arg_captures_label() {
    let cfg = make_config();
    let content = b"// BEGIN imports\nuse std::fs;\n// END imports\n// BEGIN helpers\nfn aux() {}\n// END helpers\n";
    let reader = Arc::new(
        MemReader::new(cfg)
            .with_repo("r", &["main"])
            .with_files("r", "main", &["src/lib.rs"])
            .with_content("r", "main", "src/lib.rs", content),
    );

    let src = r#"rule(R){ > repo(r) > rev(main) > fs(**/lib.rs) > marker("BEGIN", "END", $LABEL) };"#;
    let (out, diags) = run_rule(src, reader);

    assert_eq!(out.len(), 2, "expected 2 regions, got {}: diags={:?}", out.len(), diags);
    assert!(diags.is_empty(), "{:?}", diags);

    let labels: Vec<&str> = out.iter()
        .map(|c| c.captures.get("LABEL").expect("LABEL must bind").value.as_ref())
        .collect();
    assert!(labels.contains(&"imports"), "{labels:?}");
    assert!(labels.contains(&"helpers"), "{labels:?}");

    // Evidence trail reports the opening comment lines in order.
    assert_eq!(out[0].evidence.last().unwrap().matched.as_ref(), "// BEGIN imports");
    assert_eq!(out[1].evidence.last().unwrap().matched.as_ref(), "// BEGIN helpers");
}

// ── 3. Multiple paired regions produce distinct cursors ──────────────────────

#[test]
fn marker_paired_multiple_regions() {
    let cfg = make_config();
    let content = b"// BEGIN a\nalpha\n// END a\nmiddle\n// BEGIN b\nbeta\n// END b\n";
    let reader = Arc::new(
        MemReader::new(cfg)
            .with_repo("r", &["main"])
            .with_files("r", "main", &["src/lib.rs"])
            .with_content("r", "main", "src/lib.rs", content),
    );

    let src = r#"rule(R){ > repo(r) > rev(main) > fs(**/lib.rs) > marker("BEGIN", "END") };"#;
    let (out, diags) = run_rule(src, reader);

    assert_eq!(out.len(), 2, "expected 2 regions, got {}: diags={:?}", out.len(), diags);

    let r0 = out[0].byte_range.clone().unwrap();
    let r1 = out[1].byte_range.clone().unwrap();
    assert!(r0.end <= r1.start, "regions must be disjoint and ordered: {r0:?} vs {r1:?}");

    let t0 = std::str::from_utf8(&content[r0]).unwrap();
    let t1 = std::str::from_utf8(&content[r1]).unwrap();
    assert!(t0.contains("alpha"), "first region must contain alpha: {t0:?}");
    assert!(!t0.contains("beta"), "first region must not contain beta: {t0:?}");
    assert!(t1.contains("beta"),  "second region must contain beta: {t1:?}");
    assert!(!t1.contains("alpha"), "second region must not contain alpha: {t1:?}");
    assert!(!t0.contains("middle"), "middle must be outside both regions: {t0:?}");
    assert!(!t1.contains("middle"), "middle must be outside both regions: {t1:?}");
}

// ── 4. byte_range narrowing pin — exact absolute offsets ─────────────────────

#[test]
fn marker_byte_range_is_absolute_into_content() {
    let cfg = make_config();
    let content = b"// BEGIN z\nbody\n// END z\ntail\n";
    let reader = Arc::new(
        MemReader::new(cfg)
            .with_repo("r", &["main"])
            .with_files("r", "main", &["src/lib.rs"])
            .with_content("r", "main", "src/lib.rs", content),
    );

    let src = r#"rule(R){ > repo(r) > rev(main) > fs(**/lib.rs) > marker("BEGIN", "END") };"#;
    let (out, _) = run_rule(src, reader);

    assert_eq!(out.len(), 1);
    let stored = out[0].content.as_ref().expect("content preserved");
    let r = out[0].byte_range.clone().unwrap();
    let slice = &stored[r.clone()];
    assert_eq!(slice, b"// BEGIN z\nbody\n// END z",
        "byte_range must slice exactly from open-line start to close-line end; got {:?}",
        std::str::from_utf8(slice));
}

// ── 5. Content preservation — source bytes bit-identical after marker ────────

#[test]
fn marker_preserves_content_bytes() {
    let cfg = make_config();
    let content = b"// BEGIN k\nkeep\n// END k\n";
    let reader = Arc::new(
        MemReader::new(cfg)
            .with_repo("r", &["main"])
            .with_files("r", "main", &["src/lib.rs"])
            .with_content("r", "main", "src/lib.rs", content),
    );

    let src = r#"rule(R){ > repo(r) > rev(main) > fs(**/lib.rs) > marker("BEGIN", "END") };"#;
    let (out, _) = run_rule(src, reader);

    assert_eq!(out.len(), 1);
    let preserved = out[0].content.as_ref().expect("content must be preserved");
    assert_eq!(preserved.as_ref().as_ref(), content,
        "content bytes must be bit-identical to source after marker");
}

// ── 6. Content contract PATH B — marker reads rebased content, not file ─────

#[test]
fn marker_content_contract_after_rebase() {
    // Outer file is JSON. json() binds $SRC to an embedded code string. &.$SRC
    // rebases content to that substring. marker() must scan the rebased bytes.
    // Proof: the outer JSON has no `// BEGIN` comments (it's JSON), only the
    // embedded string contains them.
    let cfg = make_config();
    let json_body = br#"{"code":"// BEGIN inner\nlet x = 1;\n// END inner\n"}"#;
    let reader = Arc::new(
        MemReader::new(cfg)
            .with_repo("r", &["main"])
            .with_files("r", "main", &["snippets.json"])
            .with_content("r", "main", "snippets.json", json_body),
    );

    let src = r#"rule(R){ > repo(r) > rev(main) > fs(**/snippets.json) > json({code:$SRC}) > &.$SRC > marker("BEGIN", "END") };"#;
    let (out, diags) = run_rule(src, reader);

    assert_eq!(out.len(), 1,
        "marker must match one region in rebased content; got {} diags={:?}",
        out.len(), diags);

    let stored = out[0].content.as_ref().unwrap();
    let r = out[0].byte_range.clone().unwrap();
    let slice = std::str::from_utf8(&stored[r]).unwrap();
    assert!(slice.contains("let x = 1"),
        "byte_range slice must carve out the rebased body — proves PATH B used: {slice:?}");
    assert!(!slice.contains("json"),
        "byte_range must not include outer JSON bytes: {slice:?}");

    let ev = out[0].evidence.last().unwrap();
    assert_eq!(ev.op_name, "marker");
}

// ── 7. No-match diagnostic when patterns find nothing ────────────────────────

#[test]
fn marker_no_match_emits_diagnostic() {
    let cfg = make_config();
    let content = b"fn plain() { let y = 2; }\n";
    let reader = Arc::new(
        MemReader::new(cfg)
            .with_repo("r", &["main"])
            .with_files("r", "main", &["src/lib.rs"])
            .with_content("r", "main", "src/lib.rs", content),
    );

    let src = r#"rule(R){ > repo(r) > rev(main) > fs(**/lib.rs) > marker("BEGIN", "END") };"#;
    let (out, diags) = run_rule(src, reader);

    assert_eq!(out.len(), 0);
    assert!(diags.contains(&"marker/no-match".to_string()),
        "expected marker/no-match diagnostic, got {:?}", diags);
}

// ── 8. Missing arg diagnostic ────────────────────────────────────────────────

#[test]
fn marker_missing_arg_diagnostic() {
    let src = r#"rule(R){ > repo(r) > rev(main) > marker() };"#;
    let codes = lower_diag_codes(src);
    assert!(codes.iter().any(|c| c == "marker/missing-arg"),
        "expected marker/missing-arg, got {:?}", codes);
}

// ── 9. Bad regex diagnostic ──────────────────────────────────────────────────

#[test]
fn marker_bad_regex_diagnostic() {
    // Unterminated quote triggers the split_args error path via marker/bad-arg.
    let src = r#"rule(R){ > repo(r) > rev(main) > marker("[unbalanced") };"#;
    let codes = lower_diag_codes(src);
    assert!(codes.iter().any(|c| c == "marker/bad-arg"),
        "expected marker/bad-arg for invalid regex, got {:?}", codes);
}

// ── 10. Sequential flat markers — next open or EOF ends region ───────────────

#[test]
fn marker_sequential_flat_regions() {
    let cfg = make_config();
    let content = b"// SECTION: imports\nuse std::fs;\nuse std::path::Path;\n// SECTION: helpers\nfn cleanup() {}\n";
    let reader = Arc::new(
        MemReader::new(cfg)
            .with_repo("r", &["main"])
            .with_files("r", "main", &["src/lib.rs"])
            .with_content("r", "main", "src/lib.rs", content),
    );

    let src = r#"rule(R){ > repo(r) > rev(main) > fs(**/lib.rs) > marker("SECTION:") };"#;
    let (out, diags) = run_rule(src, reader);

    assert_eq!(out.len(), 2, "expected 2 flat regions, got {}: diags={:?}", out.len(), diags);
    assert!(diags.is_empty(), "{:?}", diags);

    let r0 = out[0].byte_range.clone().unwrap();
    let r1 = out[1].byte_range.clone().unwrap();
    assert_eq!(r0.end, r1.start,
        "first region must end exactly where the next marker starts: {r0:?} | {r1:?}");
    assert_eq!(r1.end, content.len(),
        "final region must extend to EOF; got end={} len={}", r1.end, content.len());

    let t0 = std::str::from_utf8(&content[r0]).unwrap();
    assert!(t0.contains("use std::fs"), "{t0:?}");
    assert!(!t0.contains("cleanup"),    "{t0:?}");
    let t1 = std::str::from_utf8(&content[r1]).unwrap();
    assert!(t1.contains("cleanup"), "{t1:?}");
}

// ── 11. Line-prefix fallback handles non-rust extensions ─────────────────────

#[test]
fn marker_line_prefix_fallback_for_conf_file() {
    let cfg = make_config();
    let content = b"# SECTION: net\naddr=1.2.3.4\n# SECTION: ui\ntheme=dark\n";
    let reader = Arc::new(
        MemReader::new(cfg)
            .with_repo("r", &["main"])
            .with_files("r", "main", &["app.conf"])
            .with_content("r", "main", "app.conf", content),
    );

    let src = r#"rule(R){ > repo(r) > rev(main) > fs(**/app.conf) > marker("SECTION:") };"#;
    let (out, diags) = run_rule(src, reader);

    assert_eq!(out.len(), 2, "expected 2 regions via # comments; got {}: diags={:?}", out.len(), diags);
}
