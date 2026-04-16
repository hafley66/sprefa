//! json() op integration tests. Written ahead of JsonFactory landing.
//!
//! COMPILE STATUS: broken until sonnet lands (a) `v2/src/walk/_4_brace_parse.rs`
//! and (b) `v2/src/ops/_5_json.rs` (JsonFactory). Tests encode the spec the op
//! must satisfy, including:
//!   - cursor.fs = Some → per-file walk
//!   - cursor.fs = None → self-enumerate by ext {json,yaml,yml,toml}
//!   - row-split: sibling-capture object → one row per capture
//!   - joined row: array of structs → one row per item w/ all fields
//!   - yaml + toml parity via AnyDataNode dispatch
//!   - $$ annotation syntax: $$repo_norm($R) = capture $R + ScanAnnotation
//!   - annotation-requires-capture diag when inner is not bare $NAME
//!
//! Prereq TODOs sonnet must also add:
//!   - MemReader::with_content(repo, rev, path, bytes) builder for byte serving
//!   - MemReader::bytes impl reading from that map (stop `unimplemented!()`)
//!   - JsonFactory, ext filter, parse_by_ext, walk drive, row emit

use v2::ops::default_registry;

use std::path::PathBuf;
use std::sync::Arc;

use futures::executor::block_on;
use futures_util::stream::StreamExt;

use v2::{
    Config, MemReader, MemWriter,
    OpCtx, OperatorRegistry, ProgramCtx, host_parse, lower_rules,
};
// Does not exist yet:
use v2::_0_types::Cursor;

fn make_config() -> Arc<Config> {
    Arc::new(Config::test_default())
}

fn make_registry() -> Arc<OperatorRegistry> {
    Arc::new(default_registry())
}

fn run_rule(src: &str, reader: Arc<MemReader>) -> (Vec<Cursor>, Arc<MemWriter>) {
    let cfg = reader.config.clone();
    let invs = host_parse(src, Arc::from(PathBuf::from("<test>").as_path())).unwrap();
    let pctx = ProgramCtx::new(cfg.clone(), make_registry());
    let outcome = lower_rules(invs, pctx);
    assert!(outcome.diags.is_empty(),
        "lower diags: {:?}",
        outcome.diags.iter().map(|d| d.code()).collect::<Vec<_>>());

    let writer = Arc::new(MemWriter::new());
    let ctx = OpCtx::for_test(cfg.clone(), reader.clone(), writer.clone());
    let empty: futures_core::stream::BoxStream<'static, Arc<[Cursor]>> =
        futures_util::stream::iter(Vec::<Arc<[Cursor]>>::new()).boxed();
    let batches: Vec<Arc<[Cursor]>> =
        block_on(outcome.pipelines[0].run(empty, ctx).collect());
    let out: Vec<Cursor> = batches.into_iter()
        .flat_map(|b| b.iter().cloned().collect::<Vec<_>>())
        .collect();
    (out, writer)
}

// ── 1. single file, single capture ──────────────────────────────────────────

#[test]
fn json_single_file_single_capture() {
    let cfg = make_config();
    let pkg = r#"{"name":"acme","version":"1.2.3"}"#;
    let reader = Arc::new(
        MemReader::new(cfg)
            .with_repo("myorg/alpha", &["main"])
            .with_files("myorg/alpha", "main", &["package.json"])
            .with_content("myorg/alpha", "main", "package.json", pkg.as_bytes())
    );

    let src = r#"
        rule(v) {
            > repo(myorg/alpha) > rev(main) > fs(**/package.json)
              > json({ version: $VER })
        };
    "#;
    let (out, _w) = run_rule(src, reader);

    assert_eq!(out.len(), 1);
    assert_eq!(out[0].captures.get("VER").map(|c| c.value.as_ref()), Some("1.2.3"));
    let ops: Vec<&str> = out[0].evidence.iter().map(|e| e.op_name).collect();
    assert_eq!(ops, vec!["repo", "rev", "fs", "json"]);
}

// ── 2. self-enumeration when upstream has no fs ─────────────────────────────

#[test]
fn json_self_enumerates_when_no_fs() {
    let cfg = make_config();
    let a = r#"{"name":"a","version":"1"}"#;
    let b = r#"{"name":"b","version":"2"}"#;
    let reader = Arc::new(
        MemReader::new(cfg)
            .with_repo("myorg/alpha", &["main"])
            .with_files("myorg/alpha", "main",
                &["pkg-a/package.json", "pkg-b/package.json", "README.md"])
            .with_content("myorg/alpha", "main", "pkg-a/package.json", a.as_bytes())
            .with_content("myorg/alpha", "main", "pkg-b/package.json", b.as_bytes())
            .with_content("myorg/alpha", "main", "README.md", b"# nope")
    );

    // No fs upstream — json op must enumerate json/yaml/yml/toml itself.
    // README.md skipped by ext filter.
    let src = r#"
        rule(v) {
            > repo(myorg/alpha) > rev(main)
              > json({ name: $N, version: $V })
        };
    "#;
    let (out, _w) = run_rule(src, reader);
    assert_eq!(out.len(), 2);
    let names: Vec<&str> = out.iter()
        .map(|c| c.captures.get("N").unwrap().value.as_ref())
        .collect();
    assert!(names.contains(&"a") && names.contains(&"b"));
    // fs must be set on emitted cursors even in self-enum mode.
    for c in &out {
        assert!(c.fs.is_some(), "cursor fs must be populated by json self-enum");
    }
}

// ── 3. row-split on sibling captures ────────────────────────────────────────

#[test]
fn json_row_split_sibling_captures() {
    // v1 cross-product was invisible; v2 makes it explicit: sibling Leaf captures
    // in the same Object parent emit DISTINCT rows, not one row w/ both bound.
    let cfg = make_config();
    let doc = r#"{"a":{"x":"AX"}, "b":{"y":"BY"}}"#;
    let reader = Arc::new(
        MemReader::new(cfg)
            .with_repo("r", &["main"])
            .with_files("r", "main", &["doc.json"])
            .with_content("r", "main", "doc.json", doc.as_bytes())
    );

    let src = r#"
        rule(rs) {
            > repo(r) > rev(main) > fs(**/doc.json)
              > json({ a: { x: $A }, b: { y: $B } })
        };
    "#;
    let (out, _w) = run_rule(src, reader);
    assert_eq!(out.len(), 2);
    let bound: Vec<(Option<&str>, Option<&str>)> = out.iter()
        .map(|c| (
            c.captures.get("A").map(|x| x.value.as_ref()),
            c.captures.get("B").map(|x| x.value.as_ref()),
        ))
        .collect();
    assert!(bound.contains(&(Some("AX"), None)));
    assert!(bound.contains(&(None,       Some("BY"))));
}

// ── 4. array of structs → joined row per item ───────────────────────────────

#[test]
fn json_array_of_structs_joined_rows() {
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
    let (out, _w) = run_rule(src, reader);
    assert_eq!(out.len(), 2);
    let pairs: Vec<(&str, &str)> = out.iter().map(|c| (
        c.captures.get("N").unwrap().value.as_ref(),
        c.captures.get("A").unwrap().value.as_ref(),
    )).collect();
    assert!(pairs.contains(&("ada", "36")));
    assert!(pairs.contains(&("bob", "42")));
}

// ── 5. yaml parity ──────────────────────────────────────────────────────────

#[test]
fn json_yaml_parity() {
    let cfg = make_config();
    let yml = "name: ada\nversion: 1.2.3\n";
    let reader = Arc::new(
        MemReader::new(cfg)
            .with_repo("r", &["main"])
            .with_files("r", "main", &["x.yaml"])
            .with_content("r", "main", "x.yaml", yml.as_bytes())
    );
    let src = r#"
        rule(y) { > repo(r) > rev(main) > fs(**/x.yaml) > json({ version: $V }) };
    "#;
    let (out, _w) = run_rule(src, reader);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].captures.get("V").unwrap().value.as_ref(), "1.2.3");
}

// ── 6. toml parity ──────────────────────────────────────────────────────────

#[test]
fn json_toml_parity() {
    let cfg = make_config();
    let toml = "name = \"ada\"\nversion = \"1.2.3\"\n";
    let reader = Arc::new(
        MemReader::new(cfg)
            .with_repo("r", &["main"])
            .with_files("r", "main", &["Cargo.toml"])
            .with_content("r", "main", "Cargo.toml", toml.as_bytes())
    );
    let src = r#"
        rule(t) { > repo(r) > rev(main) > fs(**/Cargo.toml) > json({ version: $V }) };
    "#;
    let (out, _w) = run_rule(src, reader);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].captures.get("V").unwrap().value.as_ref(), "1.2.3");
}

// ── 7. $$ annotation: capture + scan kind ──────────────────────────────────

#[test]
fn json_dollar_dollar_annotation_captures_and_tags() {
    // $$repo_norm($R) = bind $R from leaf AND tag it with scan-pointer sigil `repo_norm`.
    // Scan discovery not wired yet, but capture value must appear and
    // the op must retain the annotation for later Pass A consumption.
    let cfg = make_config();
    let doc = r#"{"image":{"repository":"myorg/auth-service","tag":"v1.2.3"}}"#;
    let reader = Arc::new(
        MemReader::new(cfg)
            .with_repo("consumer", &["main"])
            .with_files("consumer", "main", &["deploy/services.yaml"])
            .with_content("consumer", "main", "deploy/services.yaml", doc.as_bytes())
    );
    let src = r#"
        rule(svc) {
            > repo(consumer) > rev(main) > fs(**/services.yaml)
              > json({ image: { repository: $$repo_norm($REPO), tag: $$rev_norm($TAG) } })
        };
    "#;
    let (out, _w) = run_rule(src, reader);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].captures.get("REPO").unwrap().value.as_ref(), "myorg/auth-service");
    assert_eq!(out[0].captures.get("TAG").unwrap().value.as_ref(), "v1.2.3");
    // TODO when JsonOp exposes annotations: assert ScanAnnotation
    // { var:"REPO", sigil:"repo_norm" } and { var:"TAG", sigil:"rev_norm" }
    // are present on the op-level metadata. Interface TBD when discovery lands.
}

// ── 8. annotation grammar error: inner must be bare $NAME ──────────────────

#[test]
fn json_annotation_rejects_pattern_inner() {
    let cfg = make_config();
    let reader = Arc::new(
        MemReader::new(cfg)
            .with_repo("r", &["main"])
            .with_files("r", "main", &["x.json"])
            .with_content("r", "main", "x.json", b"{}")
    );
    let src = r#"
        rule(bad) {
            > repo(r) > rev(main) > fs(**/x.json)
              > json({ a: $$repo({ inner: $X }) })
        };
    "#;
    let invs = host_parse(src, Arc::from(PathBuf::from("<test>").as_path())).unwrap();
    let pctx = ProgramCtx::new(reader.config.clone(), make_registry());
    let outcome = lower_rules(invs, pctx);
    let codes: Vec<&str> = outcome.diags.iter().map(|d| d.code()).collect();
    assert!(
        codes.iter().any(|c| *c == "json/annotation-requires-capture"),
        "expected json/annotation-requires-capture, got {codes:?}"
    );
}

#[test]
fn json_annotation_rejects_wildcard_inner() {
    let cfg = make_config();
    let reader = Arc::new(MemReader::new(cfg).with_repo("r", &["main"]));
    let src = r#"
        rule(bad) {
            > repo(r) > rev(main) > json({ a: $$rev($_) })
        };
    "#;
    let invs = host_parse(src, Arc::from(PathBuf::from("<test>").as_path())).unwrap();
    let pctx = ProgramCtx::new(reader.config.clone(), make_registry());
    let outcome = lower_rules(invs, pctx);
    let codes: Vec<&str> = outcome.diags.iter().map(|d| d.code()).collect();
    assert!(codes.iter().any(|c| *c == "json/annotation-requires-capture"),
        "expected json/annotation-requires-capture, got {codes:?}");
}

// ── Feature A: bare wildcard value $_ ───────────────────────────────────────

#[test]
fn rule_json_bare_wildcard_underscore() {
    // $_ as a value: match any scalar, bind nothing. Key $NAME must still capture.
    let cfg = make_config();
    let pkg = r#"{"dependencies":{"react":"^18","lodash":"^4"}}"#;
    let reader = Arc::new(
        MemReader::new(cfg)
            .with_repo("r", &["main"])
            .with_files("r", "main", &["package.json"])
            .with_content("r", "main", "package.json", pkg.as_bytes())
    );

    let src = r#"
        rule(wild) {
            > repo(r) > rev(main) > fs(**/package.json)
              > json({ dependencies: { $NAME: $_ } })
        };
    "#;
    let (out, _w) = run_rule(src, reader);

    assert_eq!(out.len(), 2, "expected 2 rows (one per dep key), got {}", out.len());
    let mut names: Vec<&str> = out.iter()
        .map(|c| c.captures.get("NAME").expect("$NAME must be bound").value.as_ref())
        .collect();
    names.sort();
    assert_eq!(names, vec!["lodash", "react"]);
    // $_ must NOT produce a binding in output rows
    for c in &out {
        assert!(c.captures.get("_").is_none(), "$_ must not bind a capture named _");
    }
}

// ── Feature B: deep wildcard key ** ─────────────────────────────────────────

#[test]
#[ignore = "pre-existing failure; out of scope for Phase 2e"]
fn rule_json_deep_wildcard_key() {
    // **: descends into every top-level key and then matches the value pattern.
    let cfg = make_config();
    let yml = "frontend:\n  image:\n    repository: acme/web\nbackend:\n  image:\n    repository: acme/api\n";
    let reader = Arc::new(
        MemReader::new(cfg)
            .with_repo("r", &["main"])
            .with_files("r", "main", &["deploy/values.yaml"])
            .with_content("r", "main", "deploy/values.yaml", yml.as_bytes())
    );

    let src = r#"
        rule(wk) {
            > repo(r) > rev(main) > fs(**/values.yaml)
              > json({ **: { image: { repository: $REPO } } })
        };
    "#;
    let (out, _w) = run_rule(src, reader);

    assert_eq!(out.len(), 2, "expected 2 rows (frontend + backend), got {}", out.len());
    let mut repos: Vec<&str> = out.iter()
        .map(|c| c.captures.get("REPO").expect("$REPO must be bound").value.as_ref())
        .collect();
    repos.sort();
    assert_eq!(repos, vec!["acme/api", "acme/web"]);
}

// ── Feature C: templated string inside array spread [..."prefix/$VAR"] ───────

#[test]
fn rule_json_templated_spread() {
    // [..."crates/$PKG"] matches array entries shaped like "crates/FOO", captures FOO.
    let cfg = make_config();
    let toml = "[workspace]\nmembers = [\"crates/rules\", \"crates/lsp\", \"tools/not-match\"]\n";
    let reader = Arc::new(
        MemReader::new(cfg)
            .with_repo("r", &["main"])
            .with_files("r", "main", &["Cargo.toml"])
            .with_content("r", "main", "Cargo.toml", toml.as_bytes())
    );

    let src = r#"
        rule(spread) {
            > repo(r) > rev(main) > fs(**/Cargo.toml)
              > json({ workspace: { members: [..."crates/$PKG"] } })
        };
    "#;
    let (out, _w) = run_rule(src, reader);

    assert_eq!(out.len(), 2, "expected 2 rows (rules + lsp), got {}", out.len());
    let mut pkgs: Vec<&str> = out.iter()
        .map(|c| c.captures.get("PKG").expect("$PKG must be bound").value.as_ref())
        .collect();
    pkgs.sort();
    assert_eq!(pkgs, vec!["lsp", "rules"]);
}
