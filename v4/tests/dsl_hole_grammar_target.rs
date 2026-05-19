//! Target tests for the universal `${NAME?}` / `${NAME}` hole grammar.
//!
//! Per the spec: every backtick DSL accepts `${NAME?}` (bind) and
//! `${NAME}` (read). Each DSL chooses how to lower the binding into its
//! native surface (ast: synthetic `$SPRFSLOTN` + named regex; glob:
//! `(?P<X>[^/]*)`; re: `(?P<X>.*?)`; json: brace-pattern capture;
//! sql/sql_where: column reference on `"input"`).
//!
//! `$$${...}` is the per-DSL "looser form" carveout. Most DSLs use it
//! for a wider single-vs-multi match (ast: sibling sequence; glob:
//! multi-segment; re: loose `.*?`). `json` uses it for the recursive
//! path-capture: `$$${PATH?}` in KEY position is the v1/v2 `**:`
//! "match any sub-object arbitrarily deep, binding the traversed
//! path" idea. `json` single `${NAME?}` keeps its single-key bind.
//! sql / sql_where have no meaningful single-vs-multi distinction at
//! the hole position, so they have no `$$$` form.

use std::sync::Arc;

use effect_runtime::v2::{expand, ExpandOpts, FactStore, MemFactStore, MemQueue, QueueBackend};

use v4::compile::parse::host_parse;
use v4::compile::walk::walk_program;
use v4::lower::{default_registry, LowerCtx};
use v4::Cursor;

/// Run the program against `root` and return the populated store. Asserts
/// parse + walk are clean. Caller asserts row contents.
fn run_in(root: &std::path::Path, src: &str) -> Arc<dyn FactStore<Cursor>> {
    let (program, parse_diags) = host_parse(src);
    assert!(parse_diags.is_empty(), "parse: {:?}", parse_diags);
    let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let reg = default_registry();
    let mut ctx = LowerCtx::new(store.clone(), root.to_path_buf());
    let (pipes, walk_diags) = walk_program(&program, &reg, &mut ctx);
    assert!(
        walk_diags.is_empty(),
        "walk: {:?}",
        walk_diags
            .iter()
            .map(|d| (d.code.as_ref(), d.message.as_str()))
            .collect::<Vec<_>>()
    );
    let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
    for fused in pipes {
        let inst = fused.into_pipe().into_instance();
        expand(
            &inst,
            queue.clone(),
            vec![Arc::new(Cursor::default())],
            ExpandOpts::default(),
        );
    }
    store
}

/// Run the program against `root` and return walk diagnostics. Used to
/// assert that a given DSL surface is rejected with a focused error.
fn walk_diags(root: &std::path::Path, src: &str) -> Vec<(String, String)> {
    let (program, parse_diags) = host_parse(src);
    assert!(parse_diags.is_empty(), "parse: {:?}", parse_diags);
    let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let reg = default_registry();
    let mut ctx = LowerCtx::new(store, root.to_path_buf());
    let (_pipes, diags) = walk_program(&program, &reg, &mut ctx);
    diags
        .iter()
        .map(|d| (d.code.to_string(), d.message.clone()))
        .collect()
}

// ─── ast ──────────────────────────────────────────────────────────────────

/// `ast`'s `${NAME?}` carveout binds via `$SPRFSLOTN` + named regex.
/// Smoke-test by re-using the dedicated ast carveout target shape.
#[test]
fn ast_hole_binds_via_sprfslot() {
    use ast_grep_language::SupportLang;
    use v4::cst::diag::SilentSink;
    use v4::cst::dsl::{CaptureKind, Dsl, VecCaptureSink};
    use v4::cst::dsls::ast::AstDsl;

    let dsl = AstDsl::new(SupportLang::TypeScript);
    let compiled = dsl
        .compile(b"use${NAME?}Query($$$)", &SilentSink)
        .expect("compile");
    let target = b"useFooQuery(args); useBarQuery();";
    let mut sink = VecCaptureSink::new();
    compiled.match_into(target, 0, &mut sink);
    let names: Vec<&str> = sink
        .rows
        .iter()
        .filter(|r| &*r.name == "NAME")
        .filter_map(|r| match &r.kind {
            CaptureKind::Literal { value } => std::str::from_utf8(value).ok(),
            _ => None,
        })
        .collect();
    assert!(names.contains(&"Foo"), "names = {:?}", names);
    assert!(names.contains(&"Bar"), "names = {:?}", names);
}

// ─── glob ─────────────────────────────────────────────────────────────────

/// `${NAME?}` in glob binds one path segment. Pure regression — already
/// covered by glob_capture_smoke; this file owns the contract version.
#[test]
fn glob_hole_binds_path_segment() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("alpha.txt"), b"x").unwrap();
    let store = run_in(
        tmp.path(),
        // Rule = function: declaring it no longer runs the body
        // (auto_run removed). The `bits?(…)` call invokes it.
        "rule(:bits, STEM?, EXT?) { fs > glob`${STEM?}.${EXT?}` }; \
         bits();",
    );
    let rows = store.rows_of("bits");
    assert_eq!(rows.len(), 1);
    let stem = rows[0].get("STEM").unwrap_or("").to_string();
    let ext = rows[0].get("EXT").unwrap_or("").to_string();
    assert!(stem.ends_with("alpha"), "stem = {:?}", stem);
    assert_eq!(ext, "txt");
}

/// `$$${X?}` in glob is the alias form of `**` (multi-segment greedy
/// capture). The 3 leading `$` chars are consumed; the interp lowers to
/// `(?P<X>.*)` instead of the single-segment `[^/]*`. ast-grep is not
/// the only DSL with triple-dollar — glob has this carveout.
#[test]
fn glob_triple_dollar_is_multi_segment_alias() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("src/sub/deep")).unwrap();
    std::fs::write(tmp.path().join("src/sub/deep/leaf.rs"), b"x").unwrap();

    let store = run_in(
        tmp.path(),
        "rule(:tree, MID?, FILE?) { fs > glob`$$${MID?}/${FILE?}.rs` }; \
         tree();",
    );
    let rows = store.rows_of("tree");
    assert_eq!(rows.len(), 1);
    let mid = rows[0].get("MID").unwrap_or("").to_string();
    let file = rows[0].get("FILE").unwrap_or("").to_string();
    assert!(
        mid.ends_with("src/sub/deep"),
        "expected MID to greedy-match across segments, got {:?}",
        mid
    );
    assert_eq!(file, "leaf");
}

// ─── json ─────────────────────────────────────────────────────────────────

/// `${NAME?}` in json binds the value at that key into the cursor.
#[test]
fn json_hole_binds_value() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("doc.json"),
        br#"{"name": "alice", "age": 30}"#,
    )
    .unwrap();
    let store = run_in(
        tmp.path(),
        "rule(:hits, N?) { fs > glob`**/*.json` > read > json`{ name: ${N?} }` }; \
         hits();",
    );
    let rows = store.rows_of("hits");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("N").unwrap_or(""), "alice");
}

/// `$$${PATH?}` in json KEY position is the recursive path-capture: it
/// descends arbitrarily deep (like `**`) and binds the dot-joined
/// traversed key path into `PATH`. This is the v1/v2 `**:` "arbitrary
/// json path down" idea, surfaced as json's `$$$` carveout. Single
/// `${NAME?}` keeps its single-key bind (covered by `json_hole_binds_value`).
#[test]
fn json_triple_dollar_is_recursive_path() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("compose.json"),
        br#"{"services":{"web":{"image":"nginx"}}}"#,
    )
    .unwrap();
    let store = run_in(
        tmp.path(),
        "rule(:imgs, PATH?, IMG?) { fs > glob`**/*.json` > read \
         > json`{ $$${PATH?}: { image: ${IMG?} } }` }; \
         imgs();",
    );
    let rows = store.rows_of("imgs");
    assert_eq!(rows.len(), 1, "rows = {:?}", rows.len());
    assert_eq!(rows[0].get("IMG").unwrap_or(""), "nginx");
    // Path traversed from doc root to the object holding `image`.
    assert_eq!(rows[0].get("PATH").unwrap_or(""), "services.web");
}

// ─── re ───────────────────────────────────────────────────────────────────

/// `${NAME?}` in re lowers to `(?P<NAME>\w+)` (tight word-form default).
/// Same semantics as the existing `$NAME` sugar; the universal surface
/// keeps it consistent.
#[test]
fn re_hole_binds_match() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("notes.txt"),
        b"TODO(alice): polish\nTODO(bob): ship\n",
    )
    .unwrap();
    let store = run_in(
        tmp.path(),
        r#"rule(:hits, WHO?) { fs > glob`**/*.txt` > read > re`TODO\(${WHO?}\)` }; hits();"#,
    );
    let rows = store.rows_of("hits");
    // Order is not guaranteed; collect into a sorted set.
    let mut who: Vec<String> = rows
        .iter()
        .map(|c| c.get("WHO").unwrap_or("").to_string())
        .collect();
    who.sort();
    assert_eq!(who, vec!["alice".to_string(), "bob".to_string()]);
}

/// `$$${NAME?}` in re is the loose escape hatch — lowers to `(?P<NAME>.*?)`.
/// Use when the default word-form `\w+` is too restrictive (e.g. names
/// containing spaces or punctuation).
#[test]
fn re_triple_dollar_is_loose_any_char() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("notes.txt"),
        b"NOTE: hello world is here\n",
    )
    .unwrap();
    let store = run_in(
        tmp.path(),
        r#"rule(:hits, MSG?) { fs > glob`**/*.txt` > read > re`NOTE: $$${MSG?} is` }; hits();"#,
    );
    let rows = store.rows_of("hits");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("MSG").unwrap_or(""), "hello world");
}

// ─── sql ──────────────────────────────────────────────────────────────────

/// `${NAME?}` in sql is accepted at lower time. The `?` is the universal
/// bind marker; sql semantics treat it identically to `${NAME}` Read —
/// both lower to `"input"."NAME"`. Walk must not surface a "bind
/// interpolation not valid" error (the pre-fix behaviour).
#[test]
fn sql_hole_binds_parameter() {
    let tmp = tempfile::tempdir().unwrap();
    let diags = walk_diags(
        tmp.path(),
        "rule(:hits, V?) { sql`SELECT ${V?} AS value FROM input` };",
    );
    let bind_rejected = diags
        .iter()
        .any(|(_, m)| m.contains("bind interpolation is not valid"));
    assert!(
        !bind_rejected,
        "sql must accept `${{X?}}` Bind interp; got diags {:?}",
        diags
    );
}

// ─── sql_where ────────────────────────────────────────────────────────────

/// `${NAME?}` in a `where` predicate is just a cursor-term read (the
/// predicate has no notion of introducing a new binding). After the
/// lexer strip-`?` fix, `${X?}` and `${X}` both resolve via cursor `X`.
#[test]
fn sql_where_hole_binds_parameter() {
    use v4::compile::lower::where_eval::parse_predicate;

    let bind = parse_predicate("${X?} = 'foo'").expect("bind form");
    let read = parse_predicate("${X} = 'foo'").expect("read form");
    // Both forms parse to the same AST shape (Hole("X") on the left).
    assert_eq!(format!("{:?}", bind), format!("{:?}", read));
}

// ─── triple-dollar negative sweep ─────────────────────────────────────────

/// Triple-dollar is a per-DSL carveout for the LOOSER form. Today:
/// - ast (sibling sequence, native ast-grep)
/// - glob (multi-segment, alias of `**`)
/// - re (loose `.*?` escape hatch)
/// - json (recursive path-capture in KEY position — the v1/v2 `**:`
///   "arbitrary json path down" idea)
/// sql, sql_where have no meaningful single-vs-multi distinction at the
/// hole position, so they have no `$$$` form.
#[test]
fn triple_dollar_only_where_single_vs_multi_makes_sense() {
    use v4::cst::diag::SilentSink;
    use v4::cst::dsl::Dsl;

    // json: `$$${PATH?}` in KEY position is the recursive path-capture
    // carveout — it must COMPILE (it no longer rejects). Single-key
    // `${N?}` value bind is unaffected (covered elsewhere).
    let json = v4::cst::dsls::json::JsonDsl::new();
    assert!(
        json.compile(b"{ $$${PATH?}: { name: ${N?} } }", &SilentSink)
            .is_ok(),
        "json must accept `$$${{PATH?}}` as recursive path-capture",
    );
    // The looser `$$$` form is only meaningful in KEY position; in value
    // position json has no single-vs-multi distinction, so `$$$` there
    // is still not a special form.

    // sql / sql_where: the host pre-pass produces one Term interp for
    // the inner `${V?}`; the leading two `$` characters stay as literal
    // bytes. No DSL silently promotes triple-dollar to a special form
    // there. We don't run those through the runtime here — the contract
    // is "no per-DSL carveout", verified by reading source.
}

/// `re`${X?}|${Y?}`` — a literal `|` between capture holes is a literal
/// delimiter, NOT regex alternation. The escaped form `${X?}\|${Y?}`
/// lowers to the same pattern. A no-hole body stays raw regex.
#[test]
fn re_hole_literal_pipe_is_delimiter_not_alternation() {
    let tmp = tempfile::tempdir().unwrap();

    // Unescaped pipe: both holes capture, split on the literal `|`.
    let store = run_in(
        tmp.path(),
        "rule(:hits, X?, Y?) { `app|auth` > re`${X?}|${Y?}` }; hits();",
    );
    let rows = store.rows_of("hits");
    assert_eq!(rows.len(), 1, "rows = {rows:?}");
    assert_eq!(rows[0].get("X").unwrap_or(""), "app");
    assert_eq!(
        rows[0].get("Y").unwrap_or(""),
        "auth",
        "literal `|` must be a delimiter, not `X` OR `Y` (Y empty)"
    );

    // Escaped pipe lowers to the SAME pattern.
    let store_esc = run_in(
        tmp.path(),
        "rule(:h2, X?, Y?) { `app|auth` > re`${X?}\\|${Y?}` }; h2();",
    );
    let r2 = store_esc.rows_of("h2");
    assert_eq!(r2.len(), 1, "escaped form rows = {r2:?}");
    assert_eq!(r2[0].get("X").unwrap_or(""), "app");
    assert_eq!(r2[0].get("Y").unwrap_or(""), "auth");

    // No holes ⇒ raw regex: `a|z` alternation still matches a bare `z`.
    let store_raw = run_in(
        tmp.path(),
        "rule(:h3, M?) { `z` > re`(?P<M>a|z)` }; h3();",
    );
    let r3 = store_raw.rows_of("h3");
    assert_eq!(r3.len(), 1, "raw-regex alternation rows = {r3:?}");
    assert_eq!(r3[0].get("M").unwrap_or(""), "z");
}
