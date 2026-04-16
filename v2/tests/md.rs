//! md() op integration tests.

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

// ── 1. heading-scope happy path ──────────────────────────────────────────────

#[test]
fn md_heading_scope_two_sections() {
    let cfg = make_config();
    let doc = b"# Title\n## Alpha\nfoo content\n## Beta\nbar content\n";
    let reader = Arc::new(
        MemReader::new(cfg)
            .with_repo("r", &["main"])
            .with_files("r", "main", &["README.md"])
            .with_content("r", "main", "README.md", doc),
    );

    let src = r#"rule(R){ > repo(r) > rev(main) > fs(**/README.md) > md(## $SECTION) };"#;
    let (out, diags) = run_rule(src, reader);

    assert_eq!(out.len(), 2, "expected 2 sections, got {}: diags={:?}", out.len(), diags);
    assert!(diags.is_empty(), "no runtime diags: {:?}", diags);

    // captures
    let sections: Vec<&str> = out.iter()
        .map(|c| c.captures.get("SECTION").unwrap().value.as_ref())
        .collect();
    assert!(sections.contains(&"Alpha"), "missing Alpha: {:?}", sections);
    assert!(sections.contains(&"Beta"),  "missing Beta: {:?}", sections);

    // byte_range is narrowed to the region (not the whole file)
    for c in &out {
        let r = c.byte_range.clone().expect("byte_range must be set");
        assert!(r.len() < doc.len(), "byte_range must be sub-slice, got {}..{}", r.start, r.end);
    }

    // evidence.matched is the heading line
    let ev = out[0].evidence.last().unwrap();
    assert_eq!(ev.op_name, "md");
    assert!(ev.matched.as_ref().starts_with("## "), "matched must be heading line: {:?}", ev.matched);
}

// ── 2. heading-scope text filter ─────────────────────────────────────────────

#[test]
fn md_heading_scope_text_filter() {
    let cfg = make_config();
    let doc = b"## Install\nstep 1\n## Usage\nstep 2\n## Contributing\nstep 3\n";
    let reader = Arc::new(
        MemReader::new(cfg)
            .with_repo("r", &["main"])
            .with_files("r", "main", &["doc.md"])
            .with_content("r", "main", "doc.md", doc),
    );

    let src = r#"rule(R){ > repo(r) > rev(main) > fs(**/doc.md) > md(## Usage) };"#;
    let (out, diags) = run_rule(src, reader);

    assert_eq!(out.len(), 1, "expected 1 cursor for Usage, got {}: diags={:?}", out.len(), diags);
    let r = out[0].byte_range.clone().expect("byte_range must be set");
    let slice = &doc[r];
    assert!(std::str::from_utf8(slice).unwrap().contains("step 2"), "slice must contain Usage body");
    assert!(!std::str::from_utf8(slice).unwrap().contains("step 1"), "slice must not contain Install body");
    assert!(!std::str::from_utf8(slice).unwrap().contains("step 3"), "slice must not contain Contributing body");
    assert!(diags.is_empty(), "{:?}", diags);
}

// ── 3. heading-scope regex sugar ─────────────────────────────────────────────

#[test]
fn md_heading_scope_regex_sugar() {
    let cfg = make_config();
    let doc = b"## foo-service\ndoc a\n## bar-service\ndoc b\n";
    let reader = Arc::new(
        MemReader::new(cfg)
            .with_repo("r", &["main"])
            .with_files("r", "main", &["svc.md"])
            .with_content("r", "main", "svc.md", doc),
    );

    let src = r#"rule(R){ > repo(r) > rev(main) > fs(**/svc.md) > md(## $NAME-service) };"#;
    let (out, diags) = run_rule(src, reader);

    assert_eq!(out.len(), 2, "expected 2 cursors: diags={:?}", diags);
    assert!(diags.is_empty(), "{:?}", diags);

    let names: Vec<&str> = out.iter()
        .map(|c| c.captures.get("NAME").unwrap().value.as_ref())
        .collect();
    assert!(names.contains(&"foo"), "missing foo: {:?}", names);
    assert!(names.contains(&"bar"), "missing bar: {:?}", names);

    // evidence.matched for each cursor is the heading line
    for c in &out {
        let ev = c.evidence.last().unwrap();
        assert!(ev.matched.as_ref().contains("-service"), "evidence.matched must be heading: {:?}", ev.matched);
    }
}

// ── 4. list items fan-out ────────────────────────────────────────────────────

#[test]
fn md_list_items_fanout() {
    let cfg = make_config();
    let doc = b"## Deps\n- express\n- lodash\n* chalk\n1. first\n";
    let reader = Arc::new(
        MemReader::new(cfg)
            .with_repo("r", &["main"])
            .with_files("r", "main", &["deps.md"])
            .with_content("r", "main", "deps.md", doc),
    );

    let src = r#"rule(R){ > repo(r) > rev(main) > fs(**/deps.md) > md(- $ITEM) };"#;
    let (out, diags) = run_rule(src, reader);

    assert_eq!(out.len(), 4, "expected 4 list cursors, got {}: diags={:?}", out.len(), diags);
    assert!(diags.is_empty(), "{:?}", diags);

    let items: Vec<&str> = out.iter()
        .map(|c| c.captures.get("ITEM").unwrap().value.as_ref())
        .collect();
    assert!(items.contains(&"express"), "{:?}", items);
    assert!(items.contains(&"lodash"),  "{:?}", items);
    assert!(items.contains(&"chalk"),   "{:?}", items);
    assert!(items.contains(&"first"),   "{:?}", items);

    // Per-row evidence.matched must be the list line itself
    for c in &out {
        let ev = c.evidence.last().unwrap();
        assert_eq!(ev.op_name, "md");
        assert!(!ev.matched.is_empty(), "matched must be non-empty");
    }
}

// ── 5. links ─────────────────────────────────────────────────────────────────

#[test]
fn md_links_captures_text_and_url() {
    let cfg = make_config();
    let doc = b"See [docs](https://example.com) and [repo](https://github.com/x)\n";
    let reader = Arc::new(
        MemReader::new(cfg)
            .with_repo("r", &["main"])
            .with_files("r", "main", &["page.md"])
            .with_content("r", "main", "page.md", doc),
    );

    let src = r#"rule(R){ > repo(r) > rev(main) > fs(**/page.md) > md([$TEXT]($URL)) };"#;
    let (out, diags) = run_rule(src, reader);

    assert_eq!(out.len(), 2, "expected 2 links, got {}: diags={:?}", out.len(), diags);
    assert!(diags.is_empty(), "{:?}", diags);

    let pairs: Vec<(&str, &str)> = out.iter()
        .map(|c| (
            c.captures.get("TEXT").unwrap().value.as_ref(),
            c.captures.get("URL").unwrap().value.as_ref(),
        ))
        .collect();
    assert!(pairs.contains(&("docs", "https://example.com")),  "{:?}", pairs);
    assert!(pairs.contains(&("repo", "https://github.com/x")), "{:?}", pairs);
}

// ── 6. code blocks ───────────────────────────────────────────────────────────

#[test]
fn md_code_blocks_captures_lang() {
    let cfg = make_config();
    let doc = b"text\n```rust\nfn main() {}\n```\nmore\n```js\nconsole.log(1)\n```\n";
    let reader = Arc::new(
        MemReader::new(cfg)
            .with_repo("r", &["main"])
            .with_files("r", "main", &["code.md"])
            .with_content("r", "main", "code.md", doc),
    );

    let src = r"rule(R){ > repo(r) > rev(main) > fs(**/code.md) > md(```$LANG) };";
    let (out, diags) = run_rule(src, reader);

    assert_eq!(out.len(), 2, "expected 2 code blocks, got {}: diags={:?}", out.len(), diags);
    assert!(diags.is_empty(), "{:?}", diags);

    let langs: Vec<&str> = out.iter()
        .map(|c| c.captures.get("LANG").unwrap().value.as_ref())
        .collect();
    assert!(langs.contains(&"rust"), "{:?}", langs);
    assert!(langs.contains(&"js"),   "{:?}", langs);

    for c in &out {
        let ev = c.evidence.last().unwrap();
        assert_eq!(ev.op_name, "md");
        assert!(ev.matched.as_ref().starts_with("```"), "evidence.matched must be fence open: {:?}", ev.matched);
    }
}

// ── 7. table rows ────────────────────────────────────────────────────────────

#[test]
fn md_table_rows_skips_separator() {
    let cfg = make_config();
    let doc = b"| Name | Version |\n| --- | --- |\n| express | 4.18 |\n| lodash | 4.17 |\n";
    let reader = Arc::new(
        MemReader::new(cfg)
            .with_repo("r", &["main"])
            .with_files("r", "main", &["tbl.md"])
            .with_content("r", "main", "tbl.md", doc),
    );

    let src = r#"rule(R){ > repo(r) > rev(main) > fs(**/tbl.md) > md(| $ROW |) };"#;
    let (out, diags) = run_rule(src, reader);

    assert_eq!(out.len(), 3, "header + 2 data rows expected, got {}: diags={:?}", out.len(), diags);
    assert!(diags.is_empty(), "{:?}", diags);

    let rows: Vec<&str> = out.iter()
        .map(|c| c.evidence.last().unwrap().matched.as_ref())
        .collect();
    assert!(rows.iter().any(|r| r.contains("Name")),    "header row missing: {:?}", rows);
    assert!(rows.iter().any(|r| r.contains("express")), "express row missing: {:?}", rows);
    assert!(rows.iter().any(|r| r.contains("lodash")),  "lodash row missing: {:?}", rows);
    // separator must not appear
    assert!(!rows.iter().any(|r| r.contains("---")), "separator must be skipped: {:?}", rows);
}

// ── 8. blockquotes ───────────────────────────────────────────────────────────

#[test]
fn md_blockquotes_captures_text() {
    let cfg = make_config();
    let doc = b"> Note: this is important\n> second line\nregular text\n";
    let reader = Arc::new(
        MemReader::new(cfg)
            .with_repo("r", &["main"])
            .with_files("r", "main", &["notes.md"])
            .with_content("r", "main", "notes.md", doc),
    );

    let src = r#"rule(R){ > repo(r) > rev(main) > fs(**/notes.md) > md(> $TEXT) };"#;
    let (out, diags) = run_rule(src, reader);

    assert_eq!(out.len(), 2, "expected 2 blockquote cursors, got {}: diags={:?}", out.len(), diags);
    assert!(diags.is_empty(), "{:?}", diags);

    let texts: Vec<&str> = out.iter()
        .map(|c| c.captures.get("TEXT").unwrap().value.as_ref())
        .collect();
    assert!(texts.contains(&"Note: this is important"), "{:?}", texts);
    assert!(texts.contains(&"second line"), "{:?}", texts);
}

// ── 9. byte_range narrowing standalone ───────────────────────────────────────

#[test]
fn md_heading_scope_byte_range_narrowing() {
    let cfg = make_config();
    let doc = b"## Alpha\nfoo\n## Beta\nbar\n";
    let reader = Arc::new(
        MemReader::new(cfg)
            .with_repo("r", &["main"])
            .with_files("r", "main", &["r.md"])
            .with_content("r", "main", "r.md", doc),
    );

    let src = r#"rule(R){ > repo(r) > rev(main) > fs(**/r.md) > md(## $S) };"#;
    let (out, diags) = run_rule(src, reader);

    assert_eq!(out.len(), 2, "expected 2: diags={:?}", diags);

    // content must be bit-identical to source
    let content = out[0].content.as_ref().expect("content must be set");
    assert_eq!(content.as_ref().as_ref(), doc, "content must be bit-identical to source");

    // byte_range must slice to the correct sub-region
    let alpha_idx = out.iter().position(|c| {
        c.captures.get("S").map_or(false, |cap| cap.value.as_ref() == "Alpha")
    }).expect("Alpha cursor");
    let alpha_r = out[alpha_idx].byte_range.clone().unwrap();
    let alpha_slice = std::str::from_utf8(&doc[alpha_r.clone()]).unwrap();
    assert!(alpha_slice.contains("## Alpha"), "slice must include heading: {alpha_slice:?}");
    assert!(alpha_slice.contains("foo"),      "slice must include body: {alpha_slice:?}");
    assert!(!alpha_slice.contains("## Beta"), "slice must not include next heading: {alpha_slice:?}");
}

// ── 10. content contract PATH B ──────────────────────────────────────────────

#[test]
fn md_content_contract_path_b() {
    // json({doc:$D}) > &.$D > md(## $SECTION)
    // The $D capture is the markdown string value from JSON.
    // If PATH B is honored, md() parses the rebase content; if it falls back
    // to PATH C it would try to parse the outer JSON bytes as markdown.
    let cfg = make_config();
    let md_text = "## Alpha\nfoo\n## Beta\nbar\n";
    let json_body = format!(r#"{{"doc":"{}"}}"#, md_text);
    let reader = Arc::new(
        MemReader::new(cfg)
            .with_repo("r", &["main"])
            .with_files("r", "main", &["data.json"])
            .with_content("r", "main", "data.json", json_body.as_bytes()),
    );

    let src = r#"rule(R){ > repo(r) > rev(main) > fs(**/data.json) > json({doc:$D}) > &.$D > md(## $SECTION) };"#;
    let (out, diags) = run_rule(src, reader);

    assert_eq!(out.len(), 2,
        "PATH B must parse rebased markdown string, producing 2 section cursors; got {}; diags={:?}",
        out.len(), diags);

    let sections: Vec<&str> = out.iter()
        .map(|c| c.captures.get("SECTION").unwrap().value.as_ref())
        .collect();
    assert!(sections.contains(&"Alpha"), "missing Alpha (proves PATH B used): {:?}", sections);
    assert!(sections.contains(&"Beta"),  "missing Beta (proves PATH B used): {:?}", sections);
}

// ── 11. content contract PATH B chained md > md ──────────────────────────────

#[test]
fn md_content_contract_chained_md() {
    // md(## $SEC) > md(- $ITEM)
    // Each heading-scope cursor has byte_range narrowed. The inner md() must
    // parse content[byte_range] (PATH B) so it only sees items within each section.
    let cfg = make_config();
    let doc = b"## Alpha\n- item-a1\n- item-a2\n## Beta\n- item-b1\n- item-b2\n";
    let reader = Arc::new(
        MemReader::new(cfg)
            .with_repo("r", &["main"])
            .with_files("r", "main", &["nested.md"])
            .with_content("r", "main", "nested.md", doc),
    );

    let src = r#"rule(R){ > repo(r) > rev(main) > fs(**/nested.md) > md(## $SEC) > md(- $ITEM) };"#;
    let (out, diags) = run_rule(src, reader);

    assert_eq!(out.len(), 4,
        "4 items total (2 per section), got {}; diags={:?}", out.len(), diags);
    assert!(diags.is_empty(), "{:?}", diags);

    let items: Vec<&str> = out.iter()
        .map(|c| c.captures.get("ITEM").unwrap().value.as_ref())
        .collect();
    assert!(items.contains(&"item-a1"), "{:?}", items);
    assert!(items.contains(&"item-a2"), "{:?}", items);
    assert!(items.contains(&"item-b1"), "{:?}", items);
    assert!(items.contains(&"item-b2"), "{:?}", items);
}

// ── 12. no match emits md/no-match diagnostic ────────────────────────────────

#[test]
fn md_no_match_emits_diagnostic() {
    let cfg = make_config();
    let doc = b"# Title\nsome plain text\n";
    let reader = Arc::new(
        MemReader::new(cfg)
            .with_repo("r", &["main"])
            .with_files("r", "main", &["empty.md"])
            .with_content("r", "main", "empty.md", doc),
    );

    let src = r#"rule(R){ > repo(r) > rev(main) > fs(**/empty.md) > md(## $SECTION) };"#;
    let (out, diags) = run_rule(src, reader);

    assert_eq!(out.len(), 0, "no match must produce zero cursors");
    assert!(diags.contains(&"md/no-match".to_string()),
        "expected md/no-match diagnostic, got {:?}", diags);
}

// ── 13. malformed body emits parse-time diagnostic ───────────────────────────

#[test]
fn md_malformed_body_emits_parse_diag() {
    let cfg = make_config();
    let reader = Arc::new(MemReader::new(cfg).with_repo("r", &["main"]));

    let src = r#"rule(R){ > repo(r) > rev(main) > md(nonsense) };"#;
    let invs = host_parse(src, Arc::from(PathBuf::from("<test>").as_path())).unwrap();
    let pctx = ProgramCtx::new(reader.config.clone(), make_registry());
    let outcome = lower_rules(invs, pctx);

    let codes: Vec<&str> = outcome.diags.iter().map(|d| d.code()).collect();
    assert!(
        codes.iter().any(|c| *c == "md/bad-arg"),
        "expected md/bad-arg diagnostic, got: {:?}", codes
    );
}
