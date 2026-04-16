//! LSP hover/completion surface tests.
//! Cursor values are built inline. Op instances extracted via lower_chain.

use v2::ops::{
    default_registry, FsFactory, JsonFactory, LineFactory, MdFactory, ReadFactory, RepoFactory,
    RevFactory, RuleFactory,
};

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use v2::analysis::DocSession;
use v2::readers::{BufferOverlay, MemReader};
use v2::_0_types::{Capture, Cursor, FilePath, OpEvidence, ParseSeg, ParseSite, RunId, SprfPath};
use v2::{Op, Operator, Pipeline};

// ---------------------------------------------------------------------------
// Cursor builders
// ---------------------------------------------------------------------------

fn empty_path() -> SprfPath {
    SprfPath(Arc::from(Vec::<v2::PathSeg>::new().into_boxed_slice()))
}

fn make_parse_site() -> Arc<ParseSite> {
    Arc::new(ParseSite {
        file:       Arc::from(PathBuf::from("<test>").as_path()),
        path:       Arc::from(vec![ParseSeg::Top { index: 0 }].into_boxed_slice()),
        byte_range: 0..1,
    })
}

fn cursor(repo: &str, rev: &str) -> Cursor {
    Cursor {
        run_id: RunId(0),
        repo:   Arc::from(repo),
        rev:    Arc::from(rev),
        ..Default::default()
    }
}

fn with_capture(mut c: Cursor, name: &str, val: &str) -> Cursor {
    c.captures.insert(Arc::from(name), Capture::new(Arc::from(val)));
    c
}

fn with_fs(mut c: Cursor, path: &str) -> Cursor {
    c.fs = Some(FilePath(Arc::from(PathBuf::from(path).as_path())));
    c
}

fn with_evidence(mut c: Cursor, op_name: &'static str, site: &Arc<ParseSite>, matched: &str) -> Cursor {
    c.evidence.push(OpEvidence {
        op_name,
        parse_site: site.clone(),
        matched:    Arc::from(matched),
        capture:    None,
    });
    c
}

// ---------------------------------------------------------------------------
// Helpers — lower a bare chain expression to an Arc<dyn Op>
// ---------------------------------------------------------------------------

fn make_config() -> Arc<v2::Config> {
    Arc::new(v2::Config {
        repos:        vec![],
        revs:         vec![],
        fs_exclude:   vec![],
        sprf_files:   vec![],
        shell_allow:  vec![],
        runtime: v2::RuntimeConfig {
            worker_threads:    1,
            buffer_size:       256,
            flush_interval_ms: 100,
            collect_witnesses: true,
            xref_cartesian_limit: 10_000,
        },
        content_hash: 0,
    })
}

fn make_registry() -> Arc<v2::OperatorRegistry> {
    Arc::new(default_registry())
}

/// Lower `src` (a single op expression, e.g. `repo($R)`) and return the first
/// Pipeline::Op arc. `src` must be a bare op expression with no rule wrapper.
/// We wrap it in a dummy rule body to satisfy host_parse, then lower_chain
/// the inner invocations only.
fn parse_single_op(src: &str) -> Arc<dyn Op> {
    // host_parse_brace gives us Vec<Pipe> from a brace body string.
    // We call it with an empty parent path and pull the first op.
    let pipes = v2::host_parse_brace(
        src,
        Arc::from(PathBuf::from("<test>").as_path()),
        &Arc::from(vec![ParseSeg::Top { index: 0 }].into_boxed_slice()),
    ).expect("parse_single_op: parse failed");

    let mut pctx = v2::ProgramCtx::new(make_config(), make_registry());
    let mut diags: Vec<Box<dyn v2::Diagnostic>> = Vec::new();
    let chain = v2::lower_chain(&pipes[0].ops, &mut pctx, &mut diags);
    assert!(diags.is_empty(), "diags: {:?}", diags.iter().map(|d| d.code()).collect::<Vec<_>>());
    match chain.into_iter().next().expect("empty chain") {
        Pipeline::Op(lop) => lop.op,
        other => panic!("expected Pipeline::Op, got {:?}", std::mem::discriminant(&other)),
    }
}

/// Parse and lower a full `rule(name){ body }` and return the Pipeline::Op(RuleOp) arc.
fn parse_rule_op(src: &str) -> Arc<dyn Op> {
    let invs = v2::host_parse(src, Arc::from(PathBuf::from("<test>").as_path()))
        .expect("host_parse failed");
    let pctx = v2::ProgramCtx::new(make_config(), make_registry());
    let outcome = v2::lower_rules(invs, pctx);
    assert!(
        outcome.diags.is_empty(),
        "diags: {:?}", outcome.diags.iter().map(|d| d.code()).collect::<Vec<_>>()
    );
    match &outcome.pipelines[0] {
        Pipeline::Op(lop) => lop.op.clone(),
        _ => panic!("expected Pipeline::Op for rule"),
    }
}

// ---------------------------------------------------------------------------
// repo
// ---------------------------------------------------------------------------

#[test]
fn repo_hover_capture_lists_slugs() {
    let cursors: Vec<Cursor> = vec![
        with_capture(cursor("myorg/alpha", "main"), "R", "myorg/alpha"),
        with_capture(cursor("myorg/beta",  "main"), "R", "myorg/beta"),
        with_capture(cursor("myorg/alpha", "main"), "R", "myorg/alpha"), // duplicate
    ];
    let op = parse_single_op("repo($R)");
    let result = op.hover_capture("R", &cursors);
    assert_eq!(
        result,
        Some("**`$R`** repos:\n\n- `myorg/alpha`\n- `myorg/beta`".to_string())
    );
}

#[test]
fn repo_hover_self_contains_src() {
    let op = parse_single_op("repo($R)");
    assert_eq!(op.hover_self(), "# repo($R)");

    let op2 = parse_single_op("repo(myorg/*)");
    assert_eq!(op2.hover_self(), "# repo(myorg/*)");
}

#[test]
fn repo_hover_match_lists_repos() {
    let op = parse_single_op("repo(myorg/*)");
    let site = op.parse_site().clone();
    let cursors: Vec<Cursor> = vec![
        with_evidence(cursor("myorg/alpha", "main"), "repo", &site, "myorg/alpha"),
        with_evidence(cursor("myorg/beta",  "main"), "repo", &site, "myorg/beta"),
        with_evidence(cursor("myorg/alpha", "main"), "repo", &site, "myorg/alpha"), // duplicate
    ];
    let result = op.hover_match(site.as_ref(), &cursors);
    // No cursor has fs → grouped helper takes the flat branch.
    assert_eq!(
        result,
        Some("matches:\n\n- `myorg/alpha`\n- `myorg/beta`".to_string())
    );
}

// ---------------------------------------------------------------------------
// rev
// ---------------------------------------------------------------------------

#[test]
fn rev_hover_capture_lists_revs() {
    let cursors: Vec<Cursor> = vec![
        with_capture(cursor("r", "main"),   "V", "main"),
        with_capture(cursor("r", "next"),   "V", "next"),
        with_capture(cursor("r", "main"),   "V", "main"), // duplicate
    ];
    let op = parse_single_op("rev($V)");
    let result = op.hover_capture("V", &cursors);
    assert_eq!(
        result,
        Some("**`$V`** revisions:\n\n- `main`\n- `next`".to_string())
    );
}

#[test]
fn rev_hover_match_lists_revs() {
    let op = parse_single_op("rev(main)");
    let site = op.parse_site().clone();
    let cursors: Vec<Cursor> = vec![
        with_evidence(cursor("r", "main"), "rev", &site, "main"),
        with_evidence(cursor("r", "next"), "rev", &site, "next"),
    ];
    let result = op.hover_match(site.as_ref(), &cursors);
    // No cursor has fs → flat branch.
    assert_eq!(
        result,
        Some("**rev** glob: `main`\n\n- `main`\n- `next`".to_string())
    );
}

// ---------------------------------------------------------------------------
// fs
// ---------------------------------------------------------------------------

#[test]
fn fs_hover_capture_makes_md_links_when_file_exists() {
    let dir = tempfile::tempdir().unwrap();
    let real_path = dir.path().join("Cargo.toml");
    std::fs::write(&real_path, "[package]").unwrap();
    let real_str = real_path.to_string_lossy().into_owned();
    let fake_str = "/nonexistent/path/ghost.rs".to_string();

    let cursors: Vec<Cursor> = vec![
        with_capture(
            with_fs(cursor("r", "main"), &real_str),
            "F", &real_str,
        ),
        with_capture(
            with_fs(cursor("r", "main"), &fake_str),
            "F", &fake_str,
        ),
    ];

    let op = parse_single_op("fs($F)");
    let result = op.hover_capture("F", &cursors).unwrap();
    // Grouped by (fs, rev). For fs-op the capture value == path, so heading
    // and bullet reference the same path. Existing absolute path → file://
    // link, missing path → inline code.
    assert_eq!(
        result,
        format!(
            "**`$F`** paths:\n\n\
             ### [{real_str}](file://{real_str})\n- [{real_str}](file://{real_str})\n\n\
             ### `{fake_str}`\n- `{fake_str}`"
        )
    );
}

#[test]
fn fs_hover_match_lists_paths_bare_when_not_on_disk() {
    let op = parse_single_op("fs(**)");
    let site = op.parse_site().clone();
    let cursors: Vec<Cursor> = vec![
        with_evidence(with_fs(cursor("r", "main"), "src/a.rs"), "fs", &site, "src/a.rs"),
        with_evidence(with_fs(cursor("r", "main"), "src/b.rs"), "fs", &site, "src/b.rs"),
        with_evidence(with_fs(cursor("r", "main"), "src/a.rs"), "fs", &site, "src/a.rs"), // dup
    ];
    let result = op.hover_match(site.as_ref(), &cursors);
    // Flat list of paths; paths are relative (not on disk) → inline code.
    // Previous rendering nested the same path as both heading and bullet.
    assert_eq!(
        result,
        Some(
            "**fs** glob: `**`\n\n- `src/a.rs`\n- `src/b.rs`".to_string()
        )
    );
}

// ---------------------------------------------------------------------------
// json
// ---------------------------------------------------------------------------

#[test]
fn json_hover_capture_lists_values() {
    let op = parse_single_op("json({ name: $N })");
    let site = op.parse_site().clone();
    let cursors: Vec<Cursor> = vec![
        with_capture(with_evidence(cursor("r", "main"), "json", &site, "alpha"), "N", "alpha"),
        with_capture(with_evidence(cursor("r", "main"), "json", &site, "beta"),  "N", "beta"),
        with_capture(with_evidence(cursor("r", "main"), "json", &site, "alpha"), "N", "alpha"), // dup
    ];
    let result = op.hover_capture("N", &cursors);
    assert_eq!(
        result,
        Some("**`$N`** values:\n\n- `alpha`\n- `beta`".to_string())
    );
}

#[test]
fn json_hover_match_returns_none_with_empty_cursors() {
    let op = parse_single_op("json({ name: $N })");
    let site = make_parse_site();
    let result = op.hover_match(&site, &[]);
    assert_eq!(result, Some("**json** pattern site\n\n(no matches yet)".to_string()));
}

// ---------------------------------------------------------------------------
// rule
// ---------------------------------------------------------------------------

#[test]
fn rule_hover_self_has_rule_name_header() {
    // collect_captures only picks up bare $CAP paren slots; json({ .. }) brace-pattern
    // captures are embedded in the brace src and are not bare paren captures.
    // rule(demo){ rev($N) > rev($V) } → captures = [N, V] (both bare paren captures).
    let op = parse_rule_op("rule(demo){ > rev($N) > rev($V) }");
    let hover = op.hover_self();
    assert_eq!(
        hover,
        "# rule: demo\n\n- `$N`\n- `$V`\n\n\
         <details><summary>pipeline shape</summary>\n\n\
         ```mermaid\n\
         flowchart LR\n\
         repo --> rev --> fs --> json\n\
         ```\n\n\
         </details>\n\n\
         <details><summary>columns</summary>\n\n\
         | col | kind |\n| --- | --- |\n\
         | `$N` | Both |\n\
         | `$V` | Both |\n\n\
         </details>"
    );
}

#[test]
fn rule_hover_self_no_captures_shows_placeholder() {
    let op = parse_rule_op("rule(empty){ > repo(myorg/*) }");
    let hover = op.hover_self();
    assert_eq!(
        hover,
        "# rule: empty\n\n_(no captures)_\n\n\
         <details><summary>pipeline shape</summary>\n\n\
         ```mermaid\n\
         flowchart LR\n\
         repo --> rev --> fs --> json\n\
         ```\n\n\
         </details>\n\n\
         <details><summary>columns</summary>\n\n\
         | col | kind |\n| --- | --- |\n\n\n\
         </details>"
    );
}

#[test]
fn rule_hover_capture_renders_var_name() {
    let cursors: Vec<Cursor> = vec![
        with_capture(cursor("r", "main"), "N", "sprefa"),
        with_capture(cursor("r", "main"), "N", "v2"),
    ];
    let op = parse_rule_op("rule(demo){ > rev($N) > rev($V) }");
    let result = op.hover_capture("N", &cursors);
    assert_eq!(
        result,
        Some("**`$N`** values:\n\n- `sprefa`\n- `v2`".to_string())
    );
}

// ---------------------------------------------------------------------------
// hover_self non-empty for all ops
// ---------------------------------------------------------------------------

#[test]
fn every_op_hover_self_is_non_empty() {
    let ops: Vec<Arc<dyn Op>> = vec![
        parse_single_op("repo($R)"),
        parse_single_op("rev($V)"),
        parse_single_op("fs(**)"),
        parse_single_op("json({ name: $N })"),
        // read is in a seq — parse separately via a two-op chain
        {
            let pipes = v2::host_parse_brace(
                "fs(**) > read",
                Arc::from(PathBuf::from("<test>").as_path()),
                &Arc::from(vec![ParseSeg::Top { index: 0 }].into_boxed_slice()),
            ).unwrap();
            let mut pctx = v2::ProgramCtx::new(make_config(), make_registry());
            let mut diags: Vec<Box<dyn v2::Diagnostic>> = Vec::new();
            let chain = v2::lower_chain(&pipes[0].ops, &mut pctx, &mut diags);
            assert!(diags.is_empty());
            match chain.into_iter().nth(1).unwrap() {
                Pipeline::Op(lop) => lop.op,
                _ => panic!("expected Pipeline::Op for read"),
            }
        },
        parse_rule_op("rule(demo){ > repo(myorg/*) > rev($N) }"),
    ];
    for op in &ops {
        let s = op.hover_self();
        assert!(!s.is_empty(), "hover_self empty for op={}", op.name());
    }
}

// ---------------------------------------------------------------------------
// completion_item round-trips
// ---------------------------------------------------------------------------

#[test]
fn completion_item_labels_match_op_names() {
    let pairs: Vec<(&dyn Operator, &str)> = vec![
        (&RuleFactory,  "rule"),
        (&RepoFactory,  "repo"),
        (&RevFactory,   "rev"),
        (&FsFactory,    "fs"),
        (&ReadFactory,  "read"),
        (&JsonFactory,  "json"),
        (&LineFactory,  "line"),
        (&MdFactory,    "md"),
    ];
    for (factory, expected_label) in pairs {
        let ci = factory.completion_item();
        assert_eq!(ci.label, expected_label, "label mismatch for {}", expected_label);
        assert!(!ci.detail.is_empty(), "detail empty for {}", expected_label);
        assert!(!ci.doc.is_empty(),    "doc empty for {}",    expected_label);
    }
}

// ---------------------------------------------------------------------------
// Fix 1 regression — nested $CAP collection from brace-pattern ops
// ---------------------------------------------------------------------------

#[test]
fn rule_hover_self_lists_captures_from_nested_json_body() {
    // json({ name: $NAME, age: $AGE }) — captures embedded in json brace pattern
    // must appear in rule hover_self even though they are not bare paren tokens.
    let op = parse_rule_op("rule(myrule){ > json({ name: $NAME, age: $AGE }) }");
    let hover = op.hover_self();
    assert!(hover.contains("$NAME"), "expected $NAME in: {hover}");
    assert!(hover.contains("$AGE"),  "expected $AGE in: {hover}");
}

#[test]
fn rule_hover_self_lists_captures_from_nested_json_with_multiple_ops() {
    // Mix of bare paren capture (rev($V)) and json brace captures ($PKG, $VER).
    // All three must appear in the output.
    let op = parse_rule_op("rule(pkgs){ > rev($V) > json({ name: $PKG, version: $VER }) }");
    let hover = op.hover_self();
    assert!(hover.contains("$V"),   "expected $V in: {hover}");
    assert!(hover.contains("$PKG"), "expected $PKG in: {hover}");
    assert!(hover.contains("$VER"), "expected $VER in: {hover}");
}

// ---------------------------------------------------------------------------
// Fix 2 regression — ParseSite structural equality in hover_match
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// cursor_ref hover delegation + field-segment hover
// ---------------------------------------------------------------------------

fn session_with_json_file(source: &str, repo: &str, rev: &str, path: &str, body: &[u8]) -> DocSession {
    let mem = Arc::new(
        MemReader::new(make_config())
            .with_repo(repo, &[rev])
            .with_files(repo, rev, &[path])
            .with_content(repo, rev, path, body),
    );
    let overlay = Arc::new(BufferOverlay::new(mem));
    DocSession::new(source.to_string(), overlay, make_registry())
}

#[test]
fn hover_delegates_capture_to_binding_op() {
    let src = r#"rule(R){ > repo(acme) > rev(main) > fs(**/x.json) > json({pkg:$PKG}) > &.$PKG }"#;
    let mut s = session_with_json_file(
        src, "acme", "main", "x.json", br#"{"pkg":"alpha"}"#,
    );
    s.ensure_run();

    // First $PKG occurrence is inside json({pkg:$PKG}); the second is inside &.$PKG.
    let first  = src.find("$PKG").unwrap();
    let second = src.rfind("$PKG").unwrap();
    assert_ne!(first, second, "test must hit two distinct sites");

    let upstream  = s.hover_at(first  + 1).expect("hover on json $PKG");
    let delegated = s.hover_at(second + 1).expect("hover on &.$PKG must delegate");
    assert_eq!(upstream, delegated,
        "hover on $PKG inside &.$PKG must equal upstream binder hover");
}

#[test]
fn hover_cursor_ref_field_fs() {
    let src = r#"rule(R){ > repo(acme) > rev(main) > fs(**/x.json) > &.fs }"#;
    let mut s = session_with_json_file(
        src, "acme", "main", "x.json", br#"{"k":1}"#,
    );
    s.ensure_run();

    // Hover inside the `fs` token of `&.fs` — second occurrence of "fs" in src.
    let first_fs  = src.find("fs(").unwrap();
    let second_fs = src[first_fs + 1..].find("fs").unwrap() + first_fs + 1;
    let md = s.hover_at(second_fs + 1).expect("hover on &.fs returns markdown");
    assert!(md.contains("x.json"),
        "expected resolved fs path in hover: {md}");
    assert!(md.contains("### "),
        "expected grouped header in hover: {md}");
    eprintln!("FIELD_FS_HOVER:\n{md}");
}

// ---------------------------------------------------------------------------
// line hover
// ---------------------------------------------------------------------------

#[test]
fn line_hover_capture_delegates_to_line_binding() {
    let op = parse_single_op("line(re:TODO (?P<MSG>.*))");
    let site = op.parse_site().clone();

    let mut c1 = with_fs(cursor("r", "main"), "src/a.txt");
    c1.captures.insert(Arc::from("MSG"), v2::_0_types::Capture::new(Arc::from("finish port")));
    c1.evidence.push(OpEvidence {
        op_name:    "line",
        parse_site: site.clone(),
        matched:    Arc::from("TODO finish port"),
        capture:    None,
    });

    let mut c2 = with_fs(cursor("r", "main"), "src/b.txt");
    c2.captures.insert(Arc::from("MSG"), v2::_0_types::Capture::new(Arc::from("fix login")));
    c2.evidence.push(OpEvidence {
        op_name:    "line",
        parse_site: site.clone(),
        matched:    Arc::from("TODO fix login"),
        capture:    None,
    });

    let result = op.hover_capture("MSG", &[c1, c2]).expect("hover_capture must return Some for bound capture");
    assert!(result.contains("MSG"), "hover must mention the capture name: {result}");
    assert!(result.contains("finish port"), "hover must include capture value: {result}");
    assert!(result.contains("fix login"), "hover must include capture value: {result}");
    assert!(result.contains("### "), "hover must be grouped by file: {result}");
}

// ---------------------------------------------------------------------------
// md hover
// ---------------------------------------------------------------------------

#[test]
fn md_hover_capture_grouped_markdown() {
    // hover on $SECTION inside md(## $SECTION) must return non-empty grouped markdown.
    let op = parse_single_op("md(## $SECTION)");
    let site = op.parse_site().clone();

    let mut c1 = with_fs(cursor("r", "main"), "docs/a.md");
    c1.captures.insert(Arc::from("SECTION"), v2::_0_types::Capture::new(Arc::from("Installation")));
    c1.evidence.push(OpEvidence {
        op_name:    "md",
        parse_site: site.clone(),
        matched:    Arc::from("## Installation"),
        capture:    None,
    });

    let mut c2 = with_fs(cursor("r", "main"), "docs/b.md");
    c2.captures.insert(Arc::from("SECTION"), v2::_0_types::Capture::new(Arc::from("Usage")));
    c2.evidence.push(OpEvidence {
        op_name:    "md",
        parse_site: site.clone(),
        matched:    Arc::from("## Usage"),
        capture:    None,
    });

    let result = op.hover_capture("SECTION", &[c1, c2])
        .expect("hover_capture must return Some for bound SECTION capture");
    assert!(!result.is_empty(), "hover must be non-empty");
    assert!(result.contains("SECTION"),       "hover must reference capture name: {result}");
    assert!(result.contains("Installation"), "hover must include first value: {result}");
    assert!(result.contains("Usage"),        "hover must include second value: {result}");
    assert!(result.contains("### "),         "hover must be grouped by file: {result}");
}

#[test]
fn json_hover_match_structural_eq() {
    let op = parse_single_op("json({ name: $N })");

    // Build a ParseSite value-equal to the op's parse_site, but via a fresh Arc.
    // With Arc::ptr_eq this would never match; with structural eq it must.
    let op_site: &ParseSite = op.parse_site().as_ref();
    let site_value = op_site.clone();
    let site_arc_fresh = Arc::new(site_value.clone());

    // Build a cursor carrying evidence from "json" at that site.
    let mut c = cursor("r", "main");
    c.evidence.push(OpEvidence {
        op_name:    "json",
        parse_site: site_arc_fresh,          // fresh Arc, not the op's own Arc
        matched:    Arc::from("hello"),
        capture:    None,
    });

    // No fs on cursor → grouped helper takes the flat branch.
    let result = op.hover_match(&site_value, &[c]);
    assert_eq!(
        result,
        Some("**json** pattern site\n\n- `hello`".to_string()),
        "hover_match must use structural eq, not pointer eq"
    );
}
