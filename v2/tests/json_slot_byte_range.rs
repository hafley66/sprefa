//! Commit-2 behavior: json op publishes `JSON_TREE` slot on every output
//! cursor, narrows `cursor.byte_range` to the matched row's capture span,
//! and reuses an upstream `JSON_TREE` slot when already present.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use bytes::Bytes;
use futures::executor::block_on;
use futures_util::stream::StreamExt;

use v2::{
    Config, DiagSink, EventSink, MemReader, MemWriter,
    OpCtx, OpId, OpInvocation, OperatorRegistry, ProgramCtx, RunId,
    RuntimeConfig, host_parse, lower_rules,
};
use v2::_0_types::{Cursor, FilePath, ParseSeg, ParseSite, SprfPath};
use v2::_5_op::Operator;
use v2::data::parse_by_ext;
use v2::ops::{FsFactory, JsonFactory, JsonTree, JSON_TREE, ReadFactory, RepoFactory, RevFactory, RuleFactory};

fn make_config() -> Arc<Config> {
    Arc::new(Config {
        repos:       vec![],
        revs:        vec![],
        fs_exclude:  vec![],
        sprf_files:  vec![],
        shell_allow: vec![],
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
    let mut r = OperatorRegistry::new();
    r.register(Arc::new(RuleFactory));
    r.register(Arc::new(RepoFactory));
    r.register(Arc::new(RevFactory));
    r.register(Arc::new(FsFactory));
    r.register(Arc::new(ReadFactory));
    r.register(Arc::new(JsonFactory));
    Arc::new(r)
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
    let (result_store, xref_seen) = OpCtx::fresh_xref_state();
    let ctx = OpCtx {
        run_id: RunId(1),
        op_id:  OpId(0),
        reader: reader.clone(),
        writer,
        config: cfg.clone(),
        diags:  DiagSink(Arc::new(|_| {})),
        events: EventSink(Arc::new(|_| {})),
        result_store,
        xref_seen,
    };
    let empty: futures_core::stream::BoxStream<'static, Arc<[Cursor]>> =
        futures_util::stream::iter(Vec::<Arc<[Cursor]>>::new()).boxed();
    let batches: Vec<Arc<[Cursor]>> =
        block_on(outcome.pipelines[0].run(empty, ctx).collect());
    batches.into_iter()
        .flat_map(|b| b.iter().cloned().collect::<Vec<_>>())
        .collect()
}

// ── 1. single capture: slot is stamped, byte_range bounds the value ─────────

#[test]
fn json_stamps_slot_and_byte_range() {
    let cfg = make_config();
    let pkg = r#"{"name":"acme","version":"1.2.3"}"#;
    let reader = Arc::new(
        MemReader::new(cfg)
            .with_repo("r", &["main"])
            .with_files("r", "main", &["package.json"])
            .with_content("r", "main", "package.json", pkg.as_bytes())
    );

    let src = r#"
        rule(v) {
            > repo(r) > rev(main) > fs(**/package.json)
              > json({ version: $V })
        };
    "#;
    let out = run_rule(src, reader);
    assert_eq!(out.len(), 1);

    let c = &out[0];
    let tree = c.get_slot(JSON_TREE)
        .expect("json op must publish JSON_TREE slot on output cursor");
    use v2::data::DataNode;
    assert_eq!(tree.0.source(), pkg.as_bytes());

    let r = c.byte_range.clone()
        .expect("json op must narrow byte_range on output cursor");
    // "1.2.3" (with quotes) lives at bytes 26..33 in the source.
    let slice = &pkg.as_bytes()[r.clone()];
    assert_eq!(std::str::from_utf8(slice).unwrap(), "\"1.2.3\"",
        "byte_range {r:?} must cover the captured value");
}

// ── 2. multi-row walk shares one tree Arc ──────────────────────────────────

#[test]
fn json_rows_share_single_tree_arc() {
    let cfg = make_config();
    let doc = r#"{"users":[{"name":"ada","age":"36"},{"name":"bob","age":"42"}]}"#;
    let reader = Arc::new(
        MemReader::new(cfg)
            .with_repo("r", &["main"])
            .with_files("r", "main", &["d.json"])
            .with_content("r", "main", "d.json", doc.as_bytes())
    );
    let src = r#"
        rule(u) {
            > repo(r) > rev(main) > fs(**/d.json)
              > json({ users: [...{ name: $N, age: $A }] })
        };
    "#;
    let out = run_rule(src, reader);
    assert_eq!(out.len(), 2);

    let t0 = out[0].get_slot(JSON_TREE).expect("row 0 has slot");
    let t1 = out[1].get_slot(JSON_TREE).expect("row 1 has slot");
    assert!(Arc::ptr_eq(&t0.0, &t1.0),
        "both row cursors must share the same Arc<AnyDataNode>");

    let r0 = out[0].byte_range.clone().unwrap();
    let r1 = out[1].byte_range.clone().unwrap();
    assert_ne!(r0, r1, "row byte_ranges must differ across distinct matches");
}

// ── 3. slot reuse: upstream pre-stamped tree is used, reader is not hit ────

#[test]
fn json_reuses_upstream_slot() {
    let cfg = make_config();
    let pkg = r#"{"name":"acme","version":"9.9.9"}"#;

    // Reader with NO content. If the op re-parses via reader.bytes, it gets
    // empty bytes, parse_by_ext("json", b"") fails, and zero rows emit.
    // Only the slot-reuse path produces a row.
    let reader: Arc<MemReader> = Arc::new(
        MemReader::new(cfg.clone())
            .with_repo("r", &["main"])
            .with_files("r", "main", &["package.json"])
    );

    // Build a bare json() op via JsonFactory::parse.
    let parse_site = Arc::new(ParseSite {
        file:       Arc::from(Path::new("<test>.sprf")),
        path:       Arc::from(Vec::<ParseSeg>::new().into_boxed_slice()),
        byte_range: 0..1,
    });
    let inv = OpInvocation {
        name:       Arc::from("json"),
        brackets:   vec![],
        paren_src:  Some(v2::ParenSlot {
            src:        Arc::from("{ version: $V }"),
            byte_range: 0..15,
        }),
        brace_src:  None,
        parse_site: parse_site.clone(),
        crossrefs:  vec![],
    };
    let mut pctx = ProgramCtx::new(cfg.clone(), make_registry());
    let pipeline = JsonFactory.parse(&inv, &mut pctx).expect("parse json op");

    // Pre-stamp the input cursor with JSON_TREE.
    let arc_bytes = Arc::new(Bytes::copy_from_slice(pkg.as_bytes()));
    let tree = parse_by_ext("json", arc_bytes.clone()).unwrap();

    let mut c0 = Cursor {
        run_id:  RunId(1),
        repo:    Arc::from("r"),
        rev:     Arc::from("main"),
        fs:      Some(FilePath(Arc::from(Path::new("package.json")))),
        content: Some(arc_bytes),
        path:    SprfPath::default(),
        ..Default::default()
    };
    c0.set_slot(JSON_TREE, JsonTree(Arc::new(tree)));

    let writer = Arc::new(MemWriter::new());
    let (result_store, xref_seen) = OpCtx::fresh_xref_state();
    let ctx = OpCtx {
        run_id: RunId(1),
        op_id:  OpId(0),
        reader: reader.clone(),
        writer,
        config: cfg.clone(),
        diags:  DiagSink(Arc::new(|_| {})),
        events: EventSink(Arc::new(|_| {})),
        result_store,
        xref_seen,
    };

    let input: futures_core::stream::BoxStream<'static, Arc<[Cursor]>> =
        futures_util::stream::iter(vec![
            Arc::<[Cursor]>::from(vec![c0].into_boxed_slice()),
        ]).boxed();
    let batches: Vec<Arc<[Cursor]>> =
        block_on(pipeline.run(input, ctx).collect());
    let out: Vec<Cursor> = batches.into_iter()
        .flat_map(|b| b.iter().cloned().collect::<Vec<_>>())
        .collect();

    assert_eq!(out.len(), 1, "slot-reuse path must emit one row");
    assert_eq!(out[0].captures.get("V").map(|c| c.value.as_ref()), Some("9.9.9"));
    assert!(out[0].get_slot(JSON_TREE).is_some(),
        "output cursor carries JSON_TREE slot re-stamped");
}
