//! CursorRefOp integration tests.
//!
//! Covers: SpanBacked sub-object rebase, Synthesized string materialization,
//! field path rebase (&.fs), and CaptureKind classification at json emit time.

use v2::ops::default_registry;

use std::path::PathBuf;
use std::sync::Arc;

use futures::executor::block_on;
use futures_util::stream::StreamExt;

use v2::{
    Config, MemReader, MemWriter,
    OpCtx, OperatorRegistry, ProgramCtx, host_parse, lower_rules,
};
use v2::_0_types::{CaptureKind, Cursor};
use v2::ops::JSON_TREE;

fn make_config() -> Arc<Config> {
    Arc::new(Config::test_default())
}

fn make_registry() -> Arc<OperatorRegistry> {
    Arc::new(default_registry())
}

fn run_rule(src: &str, reader: Arc<MemReader>) -> Vec<Cursor> {
    let cfg = reader.config.clone();
    let invs = host_parse(src, Arc::from(PathBuf::from("<test>").as_path())).unwrap();
    let pctx = ProgramCtx::new(cfg.clone(), make_registry());
    let outcome = lower_rules(invs, pctx);
    assert!(outcome.diags.is_empty(),
        "lower diags: {:?}",
        outcome.diags.iter().map(|d| d.code()).collect::<Vec<_>>());

    let writer = Arc::new(MemWriter::new());
    let ctx = OpCtx::for_test(cfg.clone(), reader.clone(), writer);
    let empty: futures_core::stream::BoxStream<'static, Arc<[Cursor]>> =
        futures_util::stream::iter(Vec::<Arc<[Cursor]>>::new()).boxed();
    let batches: Vec<Arc<[Cursor]>> =
        block_on(outcome.pipelines[0].run(empty, ctx).collect());
    batches.into_iter()
        .flat_map(|b| b.iter().cloned().collect::<Vec<_>>())
        .collect()
}

// ── 1. span-backed sub-object: byte_range narrows, content preserved ────────

#[test]
fn cursor_ref_span_backed_narrows_byte_range() {
    // File contains a nested object under "pkg". &.$PKG narrows to that object.
    let doc = r#"{"pkg":{"name":"acme","version":"1.2.3"}}"#;
    let reader = Arc::new(
        MemReader::new(make_config())
            .with_repo("r", &["main"])
            .with_files("r", "main", &["d.json"])
            .with_content("r", "main", "d.json", doc.as_bytes()),
    );

    // First half: assert the rebase shape (content preserved, byte_range narrowed,
    // slots cleared).
    let src_rebase_only = r#"
        rule(v) {
            > repo(r) > rev(main) > fs(**/d.json)
              > json({ pkg: $PKG })
              > &.$PKG
        };
    "#;
    let out = run_rule(src_rebase_only, reader.clone());
    assert_eq!(out.len(), 1);

    let c = &out[0];
    // Content must still be the original file bytes.
    let content_bytes = c.content.as_ref().expect("content must be set").clone();
    assert_eq!(content_bytes.as_ref().as_ref(), doc.as_bytes(),
        "content must still be the original file buffer after SpanBacked rebase");

    // byte_range must cover exactly the inner object.
    let r = c.byte_range.clone().expect("byte_range must be set after SpanBacked rebase");
    let slice = std::str::from_utf8(&doc.as_bytes()[r]).unwrap();
    assert_eq!(slice, r#"{"name":"acme","version":"1.2.3"}"#,
        "byte_range must span the captured sub-object");

    // Slots must be cleared (JSON_TREE was on the incoming cursor, rebase drops it).
    assert!(c.get_slot(JSON_TREE).is_none(),
        "rebase must clear slots");

    // Second half: chain a downstream json onto the rebased cursor. The next op
    // must read content[byte_range] (the sub-object bytes) instead of re-fetching
    // the outer file via reader.bytes(c.fs). If the contract is honored, $V binds
    // to "1.2.3" — the version field exists at the TOP level of the sub-object,
    // not at the top level of the outer file.
    let src_chained = r#"
        rule(v) {
            > repo(r) > rev(main) > fs(**/d.json)
              > json({ pkg: $PKG })
              > &.$PKG
              > json({ version: $V })
        };
    "#;
    let out2 = run_rule(src_chained, reader);
    assert_eq!(out2.len(), 1, "chained json after &.$PKG must emit one row");
    let v = out2[0].captures.get("V").expect("V capture from inner json");
    assert_eq!(v.value.as_ref(), "1.2.3",
        "downstream json must parse the rebased sub-object, not the outer file");

    // The chained-json output must re-publish JSON_TREE on its own output cursor
    // (slot-reuse fast path for any further downstream json).
    assert!(out2[0].get_slot(JSON_TREE).is_some(),
        "post-rebase json must publish JSON_TREE on its output");
}

// ── 2. synthesized string: content materialized, escapes resolved ───────────

#[test]
fn cursor_ref_synthesized_materializes_unescaped_content() {
    // Embed escape sequences in the JSON string value.
    let doc = r#"{"raw":"line1\nline2\t\"q\""}"#;
    let reader = Arc::new(
        MemReader::new(make_config())
            .with_repo("r", &["main"])
            .with_files("r", "main", &["d.json"])
            .with_content("r", "main", "d.json", doc.as_bytes()),
    );

    let src = r#"
        rule(v) {
            > repo(r) > rev(main) > fs(**/d.json)
              > json({ raw: $RAW })
              > &.$RAW
        };
    "#;
    let out = run_rule(src, reader);
    assert_eq!(out.len(), 1);

    let c = &out[0];
    // byte_range must be cleared (synthesized path replaces content).
    assert!(c.byte_range.is_none(),
        "Synthesized rebase must clear byte_range");

    // Content must be the unescaped string bytes.
    let got = c.content.as_ref().expect("content must be set after Synthesized rebase");
    assert_eq!(got.as_ref().as_ref(), b"line1\nline2\t\"q\"",
        "materialized content must be the unescaped string bytes");

    // Slots cleared.
    assert!(c.get_slot(JSON_TREE).is_none(),
        "rebase must clear slots");
}

// ── 3. field path: &.fs materializes the file path as content ───────────────

#[test]
fn cursor_ref_field_fs_materializes_path() {
    let doc = r#"{"x":1}"#;
    let reader = Arc::new(
        MemReader::new(make_config())
            .with_repo("r", &["main"])
            .with_files("r", "main", &["src/lib.json"])
            .with_content("r", "main", "src/lib.json", doc.as_bytes()),
    );

    let src = r#"
        rule(v) {
            > repo(r) > rev(main) > fs(**/lib.json)
              > &.fs
        };
    "#;
    let out = run_rule(src, reader);
    assert_eq!(out.len(), 1);

    let c = &out[0];
    assert!(c.byte_range.is_none(), "field rebase clears byte_range");
    let got = c.content.as_ref().expect("content set after &.fs");
    let got_str = std::str::from_utf8(got.as_ref().as_ref()).unwrap();
    // Path may be relative or absolute — must contain the file name.
    assert!(got_str.contains("lib.json"),
        "materialized content must be the file path, got `{got_str}`");
}

// ── 4. CaptureKind classification at json emit time ─────────────────────────

#[test]
fn json_emits_correct_capture_kinds() {
    // Object → SpanBacked. String → Synthesized. Number → Synthesized.
    let doc = r#"{"obj":{"k":"v"},"str":"hello","num":42}"#;
    let reader = Arc::new(
        MemReader::new(make_config())
            .with_repo("r", &["main"])
            .with_files("r", "main", &["d.json"])
            .with_content("r", "main", "d.json", doc.as_bytes()),
    );

    let src = r#"
        rule(v) {
            > repo(r) > rev(main) > fs(**/d.json)
              > json({ obj: $OBJ, str: $STR, num: $NUM })
        };
    "#;
    let out = run_rule(src, reader);
    assert_eq!(out.len(), 1);

    let c = &out[0];
    let obj = c.captures.get("OBJ").expect("OBJ capture");
    let str = c.captures.get("STR").expect("STR capture");
    let num = c.captures.get("NUM").expect("NUM capture");

    assert!(
        matches!(obj.kind, CaptureKind::SpanBacked { .. }),
        "object capture must be SpanBacked, got {:?}", obj.kind
    );
    assert_eq!(str.kind, CaptureKind::Synthesized,
        "string capture must be Synthesized");
    assert_eq!(num.kind, CaptureKind::Synthesized,
        "number capture must be Synthesized");

    // SpanBacked span must cover the object bytes in the source.
    if let CaptureKind::SpanBacked { ref span } = obj.kind {
        let slice = std::str::from_utf8(&doc.as_bytes()[span.clone()]).unwrap();
        assert_eq!(slice, r#"{"k":"v"}"#,
            "SpanBacked span must cover the object sub-document");
    }
}
