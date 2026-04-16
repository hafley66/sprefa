//! line() op integration tests.

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

/// Run one rule and return (cursors, runtime-diagnostic-codes).
/// Lower diagnostics still panic via assert — they belong to parse, not runtime.
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

// ── 1. regex with named groups ───────────────────────────────────────────────

#[test]
fn line_regex_captures_named_groups() {
    let cfg = make_config();
    let content = b"hello\nTODO finish port\nworld\n";
    let reader = Arc::new(
        MemReader::new(cfg)
            .with_repo("r", &["main"])
            .with_files("r", "main", &["src/file.txt"])
            .with_content("r", "main", "src/file.txt", content),
    );

    let src = r#"rule(R){ > repo(r) > rev(main) > fs(**/file.txt) > line(re:TODO (?P<MSG>.*)) };"#;
    let (out, diags) = run_rule(src, reader);

    assert_eq!(out.len(), 1, "expected one matching line, got {}", out.len());
    assert_eq!(out[0].captures.get("MSG").unwrap().value.as_ref(), "finish port");
    let ev = out[0].evidence.last().expect("line must append evidence");
    assert_eq!(ev.op_name, "line");
    assert_eq!(ev.matched.as_ref(), "TODO finish port");
    assert!(diags.is_empty(), "no runtime diags expected, got {:?}", diags);
}

// ── 2. segment capture pattern ───────────────────────────────────────────────

#[test]
fn line_segment_capture_pattern() {
    let cfg = make_config();
    let content = b"alice:pass1\nbob:pass2\n";
    let reader = Arc::new(
        MemReader::new(cfg)
            .with_repo("r", &["main"])
            .with_files("r", "main", &["creds.txt"])
            .with_content("r", "main", "creds.txt", content),
    );

    let src = r#"rule(R){ > repo(r) > rev(main) > fs(**/creds.txt) > line($USER:$PWD) };"#;
    let (out, diags) = run_rule(src, reader);

    assert_eq!(out.len(), 2, "expected two matching lines, got {}", out.len());
    assert!(diags.is_empty(), "{:?}", diags);

    let rows: Vec<(String, String, String)> = out.iter().map(|c| (
        c.captures.get("USER").unwrap().value.to_string(),
        c.captures.get("PWD").unwrap().value.to_string(),
        c.evidence.last().unwrap().matched.to_string(),
    )).collect();

    assert!(rows.contains(&("alice".into(), "pass1".into(), "alice:pass1".into())),
        "missing alice row: {:?}", rows);
    assert!(rows.contains(&("bob".into(),   "pass2".into(), "bob:pass2".into())),
        "missing bob row: {:?}", rows);
}

// ── 3. byte_range narrows to matched line ────────────────────────────────────

#[test]
fn line_byte_range_narrows_to_matched_line() {
    // "hello\n" = 0..6, "TODO finish port" = 6..22 (bytes before the '\n' at 22).
    let cfg = make_config();
    let content = b"hello\nTODO finish port\nworld\n";
    let reader = Arc::new(
        MemReader::new(cfg)
            .with_repo("r", &["main"])
            .with_files("r", "main", &["src/file.txt"])
            .with_content("r", "main", "src/file.txt", content),
    );

    let src = r#"rule(R){ > repo(r) > rev(main) > fs(**/file.txt) > line(re:TODO (?P<MSG>.*)) };"#;
    let (out, _diags) = run_rule(src, reader);

    assert_eq!(out.len(), 1);
    let r = out[0].byte_range.clone().expect("byte_range must be set on line match");
    assert_eq!(r, 6..22);
    assert_eq!(&content[r], b"TODO finish port");
}

// ── 4. content preserved for chaining ───────────────────────────────────────

#[test]
fn line_preserves_content_for_chaining() {
    let cfg = make_config();
    let content = b"x\nTODO fix\ny\n";
    let reader = Arc::new(
        MemReader::new(cfg)
            .with_repo("r", &["main"])
            .with_files("r", "main", &["a.txt"])
            .with_content("r", "main", "a.txt", content),
    );

    let src = r#"rule(R){ > repo(r) > rev(main) > fs(**/a.txt) > line(re:TODO) };"#;
    let (out, _) = run_rule(src, reader);

    assert_eq!(out.len(), 1);
    let preserved = out[0].content.as_ref().expect("content must be preserved");
    assert_eq!(preserved.as_ref().as_ref(), content,
        "content must be bit-identical to source bytes");
}

// ── 5. content contract after rebase ────────────────────────────────────────

#[test]
fn line_content_contract_after_rebase() {
    // json({code:$C}) > &.$C > line(re:TODO (?P<REST>.*))
    // Outer JSON: `{"code":"x\nTODO fix\ny"}`. $C is the UNESCAPED string "x\nTODO fix\ny".
    // line() must read the rebased sub-content and match inside it.
    // If it fell through to PATH C it would scan the raw JSON bytes (one logical line,
    // no literal '\n') and match zero.
    let cfg = make_config();
    let json_body = br#"{"code":"x\nTODO fix\ny"}"#;
    let reader = Arc::new(
        MemReader::new(cfg)
            .with_repo("r", &["main"])
            .with_files("r", "main", &["config.json"])
            .with_content("r", "main", "config.json", json_body),
    );

    let src = r#"rule(R){ > repo(r) > rev(main) > fs(**/config.json) > json({code:$C}) > &.$C > line(re:TODO (?P<REST>.*)) };"#;
    let (out, diags) = run_rule(src, reader);

    assert_eq!(out.len(), 1,
        "line must match exactly the rebased TODO line; cursors={} diags={:?}",
        out.len(), diags);
    assert_eq!(out[0].captures.get("REST").unwrap().value.as_ref(), "fix",
        "REST must capture the text after TODO in the REBASED content");
    let ev = out[0].evidence.last().unwrap();
    assert_eq!(ev.op_name, "line");
    assert_eq!(ev.matched.as_ref(), "TODO fix",
        "matched evidence proves line read cursor.content, not reader.bytes");
}

// ── 6. no match emits diagnostic ─────────────────────────────────────────────

#[test]
fn line_no_match_emits_diagnostic() {
    let cfg = make_config();
    let content = b"hello\nworld\n";
    let reader = Arc::new(
        MemReader::new(cfg)
            .with_repo("r", &["main"])
            .with_files("r", "main", &["a.txt"])
            .with_content("r", "main", "a.txt", content),
    );

    let src = r#"rule(R){ > repo(r) > rev(main) > fs(**/a.txt) > line(re:TODO) };"#;
    let (out, diags) = run_rule(src, reader);
    assert_eq!(out.len(), 0, "no match must emit zero cursors");
    assert!(diags.contains(&"line/no-match".to_string()),
        "expected line/no-match diagnostic, got {:?}", diags);
}

// ── 7. missing arg diagnostic ────────────────────────────────────────────────

#[test]
fn line_missing_arg_diagnostic() {
    let cfg = make_config();
    let reader = Arc::new(MemReader::new(cfg).with_repo("r", &["main"]));

    let src = r#"rule(R){ > repo(r) > rev(main) > line() };"#;
    let invs = host_parse(src, Arc::from(PathBuf::from("<test>").as_path())).unwrap();
    let pctx = ProgramCtx::new(reader.config.clone(), make_registry());
    let outcome = lower_rules(invs, pctx);

    let codes: Vec<&str> = outcome.diags.iter().map(|d| d.code()).collect();
    assert!(
        codes.iter().any(|c| *c == "line/missing-arg"),
        "expected line/missing-arg diagnostic, got: {:?}", codes
    );
}

// ── 8. re: $-capture sugar expands to named groups ──────────────────────────

#[test]
fn line_re_dollar_sugar_captures_word_boundaries() {
    let cfg = make_config();
    let content = b"v1.2.3-abc9f\nnope\n";
    let reader = Arc::new(
        MemReader::new(cfg)
            .with_repo("r", &["main"])
            .with_files("r", "main", &["tag.txt"])
            .with_content("r", "main", "tag.txt", content),
    );

    let src = r#"rule(R){ > repo(r) > rev(main) > fs(**/tag.txt) > line(re:$VER-$SHA) };"#;
    let (out, _) = run_rule(src, reader);

    assert_eq!(out.len(), 1, "expected exactly one match, got {}", out.len());
    assert_eq!(out[0].captures.get("VER").unwrap().value.as_ref(), "v1.2.3");
    assert_eq!(out[0].captures.get("SHA").unwrap().value.as_ref(), "abc9f");
    assert_eq!(out[0].evidence.last().unwrap().matched.as_ref(), "v1.2.3-abc9f");
}
