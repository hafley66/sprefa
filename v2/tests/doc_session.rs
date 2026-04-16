//! DocSession integration tests.
//!
//! Uses MemReader as inner, wrapped in BufferOverlay, driven from inline .sprf strings.
//! Positions are byte offsets into the source string.

use v2::ops::default_registry;

use std::sync::Arc;

use v2::analysis::DocSession;
use v2::readers::{BufferOverlay, MemReader};
use v2::{Config, OperatorRegistry, RuntimeConfig};

// ---------------------------------------------------------------------------
// Shared setup
// ---------------------------------------------------------------------------

fn make_config() -> Arc<Config> {
    Arc::new(Config {
        repos:        vec![],
        revs:         vec![],
        fs_exclude:   vec![],
        sprf_files:   vec![],
        shell_allow:  vec![],
        runtime: RuntimeConfig {
            worker_threads:    1,
            buffer_size:       256,
            flush_interval_ms: 100,
            collect_witnesses: true,
            xref_cartesian_limit: 10_000,
        },
        content_hash: 0,
    })
}

fn make_registry() -> Arc<OperatorRegistry> {
    Arc::new(default_registry())
}

/// Build a DocSession with a bare MemReader (no repos), wrapping in BufferOverlay.
fn session(source: &str) -> DocSession {
    let mem = Arc::new(MemReader::new(make_config()));
    let overlay = Arc::new(BufferOverlay::new(mem));
    DocSession::new(source.to_string(), overlay, make_registry())
}

/// Build a DocSession whose MemReader has one repo "acme" at rev "main".
fn session_with_repo(source: &str) -> DocSession {
    let mem = Arc::new(MemReader::new(make_config()).with_repo("acme", &["main"]));
    let overlay = Arc::new(BufferOverlay::new(mem));
    DocSession::new(source.to_string(), overlay, make_registry())
}

/// Return the byte offset of the first occurrence of `needle` in `haystack`.
/// Panics if not found.
fn pos_of(haystack: &str, needle: &str) -> usize {
    haystack.find(needle).unwrap_or_else(|| panic!("needle {:?} not in source", needle))
}

// ---------------------------------------------------------------------------
// Test 1 — hover on op name returns hover_self
// ---------------------------------------------------------------------------

#[test]
fn doc_session_hover_on_op_name_returns_op_hover_self() {
    let src = r#"rule(test_rule){ > repo("*") > rev("main") }"#;
    let mut s = session_with_repo(src);

    // `repo` starts at some position inside the brace body.
    // OpName spans cover the op identifier. Find byte offset of "repo" inside braces.
    // The brace body starts after `rule(test_rule){` — `repo` is the first op in body.
    // Because brace body ops are lowered internally and their ParseSites use
    // BraceChild indices, the span we emit is at inv.parse_site.byte_range.start
    // which equals the byte offset of the 'r' in `repo`.
    let repo_pos = pos_of(src, "repo");
    let result = s.hover_at(repo_pos);
    assert_eq!(result, Some(r#"# repo("*")"#.to_string()));
}

// ---------------------------------------------------------------------------
// Test 2 — hover on $REPO capture lists cursors
// ---------------------------------------------------------------------------

#[test]
fn doc_session_hover_on_capture_returns_op_hover_capture() {
    let src = "rule(test_rule){ > repo($REPO) > rev($REV) }";
    let mut s = session_with_repo(src);

    s.ensure_run();

    // After ensure_run with "acme"/"main" in reader, captures should be populated.
    // Hover on `$REPO` token inside paren.
    let dollar_pos = pos_of(src, "$REPO");
    let result = s.hover_at(dollar_pos);

    // hover_capture on repo($REPO) with one cursor (repo="acme") → lists "acme"
    assert_eq!(
        result,
        Some("**`$REPO`** repos:\n\n- `acme`".to_string())
    );
}

// ---------------------------------------------------------------------------
// Test 3 — cross-ref hover delegates to target rule (ignored: needs cross-ref runtime)
// ---------------------------------------------------------------------------

#[test]
fn doc_session_hover_on_crossref_delegates_to_target() {
    let src = "rule(alpha){ > repo($A) }\nrule(beta){ > repo(\"${alpha.$A}\") }";
    let mut s = session_with_repo(src);
    s.ensure_run();

    let xref_pos = pos_of(src, "${alpha.$A}");
    let result = s.hover_at(xref_pos);
    // Should delegate to alpha's repo.hover_capture("A", cursors_for_alpha)
    assert!(result.is_some());
    let md = result.unwrap();
    assert!(md.contains("acme"), "expected acme in cross-ref hover: {md}");
}

// ---------------------------------------------------------------------------
// Test 4 — completions at top of line lists all ops
// ---------------------------------------------------------------------------

#[test]
fn doc_session_completions_at_top_of_line_lists_ops() {
    let src = "rule(demo){ > repo(*) }";
    let mut s = session(src);

    let items = s.completions_at(0);
    let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
    for expected in &["repo", "rev", "fs", "read", "json", "rule"] {
        assert!(
            labels.contains(expected),
            "missing op {:?} in completions; got {:?}", expected, labels
        );
    }
}

// ---------------------------------------------------------------------------
// Test 5 — rerun picks up source change
// ---------------------------------------------------------------------------

#[test]
fn doc_session_rerun_picks_up_source_change() {
    let src1 = "rule(r1){ > repo($R) }";
    let mut s = session_with_repo(src1);
    s.ensure_run();

    let dollar_pos = pos_of(src1, "$R");
    let result1 = s.hover_at(dollar_pos);
    assert_eq!(result1, Some("**`$R`** repos:\n\n- `acme`".to_string()));

    // Now change source to use a different capture name.
    let src2 = "rule(r1){ > repo($SLUG) }".to_string();
    let dollar_pos2 = pos_of(&src2, "$SLUG");
    s.on_source_change(src2);
    let result2 = s.hover_at(dollar_pos2);
    assert_eq!(result2, Some("**`$SLUG`** repos:\n\n- `acme`".to_string()));
}

// ---------------------------------------------------------------------------
// Test 6 — diagnostics surface from run (ignored: needs an op that emits diags)
// ---------------------------------------------------------------------------

#[test]
fn doc_session_diagnostics_surface_from_run() {
    // Runtime diagnostic: json op runs against a package.json that has no
    // `dependencies` key; partial-match semantics emit `json/partial-match` (hint).
    // Asserts on s.diagnostics() (run-time surface), not parse_diagnostics.
    let src = r#"rule(r){ > repo(acme) > rev(main) > json({ dependencies: { $DEP: $_ } }) }"#;
    let mem = Arc::new(
        MemReader::new(make_config())
            .with_repo("acme", &["main"])
            .with_files("acme", "main", &["package.json"])
            .with_content("acme", "main", "package.json", br#"{"name":"foo"}"#),
    );
    let overlay = Arc::new(BufferOverlay::new(mem));
    let mut s = DocSession::new(src.to_string(), overlay, make_registry());
    s.ensure_run();

    let codes: Vec<&str> = s.diagnostics().iter().map(|d| d.code()).collect();
    assert!(
        codes.iter().any(|c| *c == "json/partial-match"),
        "expected json/partial-match in run diagnostics, got: {:?}",
        codes,
    );
}

// ---------------------------------------------------------------------------
// Test 7 — hover on MatchSite renders matches (ignored: needs literal paren + cursors)
// ---------------------------------------------------------------------------

#[test]
fn doc_session_hover_on_match_span_renders_matches() {
    let src = r#"rule(r){ > repo(acme) }"#;
    let mut s = session_with_repo(src);
    s.ensure_run();
    // Hover on the literal `acme` inside the paren.
    let pos = pos_of(src, "acme");
    let result = s.hover_at(pos);
    assert!(result.is_some());
    let md = result.unwrap();
    assert!(md.contains("acme"), "expected acme in match hover: {md}");
}

// ---------------------------------------------------------------------------
// Test 8 — hover outside any span returns None
// ---------------------------------------------------------------------------

#[test]
fn doc_session_hover_outside_any_span_returns_none() {
    // Whitespace between ops has no span.
    let src = "rule(demo){ > repo(*) }";
    let mut s = session(src);
    // Position at the space between `{` and `repo` — or the closing `}`.
    // The closing `}` is definitely outside any registered span.
    let close_brace_pos = src.rfind('}').unwrap();
    let result = s.hover_at(close_brace_pos);
    assert_eq!(result, None);
}

#[test]
fn doc_session_hover_on_deeply_nested_ops_resolves() {
    let src = r#"rule(foo) { > repo($R) { > rev(HEAD) { > fs(**/x.yaml); }; }; };"#;
    let mut s = session(src);
    s.ensure_run();
    let rev_pos = src.find("HEAD").unwrap() + 1;
    let fs_pos  = src.find("**/x.yaml").unwrap() + 1;
    assert!(s.hover_at(rev_pos).is_some(), "depth-3 rev hover must resolve");
    assert!(s.hover_at(fs_pos).is_some(), "depth-4 fs hover must resolve");
}

// ---------------------------------------------------------------------------
// Completion — in-paren partial evaluation
// ---------------------------------------------------------------------------
//
// Exercise the DocSession::complete_in_paren boundary with no tower-lsp.
// The pipeline for each case is truncated+substituted with the op's
// wildcard_instance, run through run_pipelines, and witnesses are
// harvested into CompletionSuggestion.

fn session_with_files(source: &str) -> DocSession {
    let mem = Arc::new(
        MemReader::new(make_config())
            .with_repo("acme", &["main"])
            .with_files("acme", "main", &["src/lib.rs", "src/main.rs", "README.md"]),
    );
    let overlay = Arc::new(BufferOverlay::new(mem));
    DocSession::new(source.to_string(), overlay, make_registry())
}

#[test]
fn doc_session_completion_repo_paren_lists_repos() {
    let src = "rule(r){ > repo() }";
    let mut s = session_with_repo(src);

    // Cursor inside `repo(|)`.
    let pos = pos_of(src, "repo(") + "repo(".len();
    let items = s.completions_at(pos);

    let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
    assert!(
        labels.contains(&"acme"),
        "expected 'acme' in repo completions; got {:?}", labels,
    );
}

#[test]
fn doc_session_completion_rev_paren_lists_revs() {
    let src = "rule(r){ > repo(acme) > rev() }";
    let mut s = session_with_repo(src);

    let pos = pos_of(src, "rev(") + "rev(".len();
    let items = s.completions_at(pos);

    let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
    assert!(
        labels.contains(&"main"),
        "expected 'main' in rev completions; got {:?}", labels,
    );
}

#[test]
fn doc_session_completion_fs_paren_lists_paths() {
    let src = "rule(r){ > repo(acme) > rev(main) > fs() }";
    let mut s = session_with_files(src);

    let pos = pos_of(src, "fs(") + "fs(".len();
    let items = s.completions_at(pos);

    let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
    for expected in &["src/lib.rs", "src/main.rs", "README.md"] {
        assert!(
            labels.iter().any(|l| l == expected),
            "expected {expected:?} in fs completions; got {:?}", labels,
        );
    }
}

#[test]
#[ignore = "tolerant parse at top level works, but RuleFactory::parse still uses strict \
            host_parse_arm_brace for rule bodies — completion inside a mid-edit rule \
            body needs ProgramCtx.parse_mode plumbing through op-owned parse. \
            Closed-paren cases (repo/rev/fs above) cover the runner integration."]
fn doc_session_completion_rev_partial_mid_typing_tolerant_parse() {
    let src = "rule(r){ > repo(acme) > rev(ma";
    let mut s = session_with_repo(src);

    let pos = src.len();
    let items = s.completions_at(pos);

    let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
    assert!(
        labels.contains(&"main"),
        "expected 'main' in tolerant-parse rev completions; got {:?}", labels,
    );
}

// ---------------------------------------------------------------------------
// Completion — pattern filter parity
// ---------------------------------------------------------------------------
//
// These exercise the completion filter path where the partial typed inside
// the paren is itself a pattern. Before the CompiledPattern promotion the
// completion filter used either `contains` or a hand-rolled glob_match, so
// `re:` regex, `{a,b}` braces, and `|` alternation all silently returned
// nothing. With CompiledPattern wired end-to-end the filter must honor
// every shape the fs op honors at runtime.
//
// No tower-lsp in this path: DocSession::completions_at drives
// analysis::complete_in_paren directly against MemReader.

#[test]
fn doc_session_completion_fs_filter_glob_double_star() {
    // Cursor is positioned at END of the typed partial so loc.partial is
    // `src/**/*.rs`. The filter must recognize the glob and exclude
    // non-matching files (pre-regression the filter was substring-only).
    let partial = "src/**/*.rs";
    let src = format!("rule(r){{ > repo(acme) > rev(main) > fs({partial}) }}");
    let mem = Arc::new(
        MemReader::new(make_config())
            .with_repo("acme", &["main"])
            .with_files("acme", "main", &["src/lib.rs", "src/main.rs", "README.md"]),
    );
    let overlay = Arc::new(BufferOverlay::new(mem));
    let mut s = DocSession::new(src.clone(), overlay, make_registry());
    let pos = pos_of(&src, "fs(") + "fs(".len() + partial.len();
    let items = s.completions_at(pos);

    let labels: Vec<String> = items.iter().map(|i| i.label.clone()).collect();
    assert!(
        labels.iter().any(|l| l == "src/lib.rs")
            && labels.iter().any(|l| l == "src/main.rs"),
        "expected src/lib.rs and src/main.rs; got {:?}", labels,
    );
    assert!(
        !labels.iter().any(|l| l == "README.md"),
        "README.md must be filtered out; got {:?}", labels,
    );
}

// `|` alternation, `re:` regex, unclosed char class `[`, and segment-capture
// `$NAME` partials interact with the top-level sprf tokenizer in ways that
// can wedge mid-typing parses (those chars have meaning outside the paren).
// Op-runtime parity for `re:` / `$NAME` / `{a,b}` / `[abc]` is pinned by the
// fs-op unit tests in ops/_3_fs.rs; the LSP integration layer here asserts
// the completion filter honors the shapes safe in mid-typing source: plain
// globs with `*`/`**`.

#[test]
fn doc_session_completion_kind_hint_matches_op_name() {
    let src = "rule(r){ > repo() }";
    let mut s = session_with_repo(src);

    let pos = pos_of(src, "repo(") + "repo(".len();
    let items = s.completions_at(pos);

    let acme = items.iter().find(|i| i.label == "acme")
        .expect("expected 'acme' suggestion");
    assert_eq!(acme.kind_hint.as_deref(), Some("repo"));
}
