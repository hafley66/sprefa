//! Target tests for the universal `${NAME?}` / `${NAME}` hole grammar.
//!
//! Per the spec: every backtick DSL accepts `${NAME?}` (bind) and
//! `${NAME}` (read). Each DSL chooses how to lower the binding into its
//! native surface (ast: synthetic `$SPRFSLOTN` + named regex; glob:
//! `(?P<X>[^/]*)`; re: `(?P<X>.*?)`; json: brace-pattern capture;
//! sql/sql_where: column reference on `"input"`).
//!
//! `$$${...}` is **ast-grep only**. Every other DSL rejects (or
//! literalises) it.

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
        "rule(:bits, STEM?, EXT?) { fs > glob`${STEM?}.${EXT?}` };",
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
        "rule(:tree, MID?, FILE?) { fs > glob`$$${MID?}/${FILE?}.rs` };",
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
        "rule(:hits, N?) { fs > glob`**/*.json` > read > json`{ name: ${N?} }` };",
    );
    let rows = store.rows_of("hits");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("N").unwrap_or(""), "alice");
}

// ─── re ───────────────────────────────────────────────────────────────────

/// `${NAME?}` in re lowers to `(?P<NAME>.*?)`. Same semantics as the
/// existing `$NAME` sugar; the universal surface keeps it consistent.
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
        r#"rule(:hits, WHO?) { fs > glob`**/*.txt` > read > re`TODO\(${WHO?}\)` };"#,
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

/// Triple-dollar is ast-grep native + glob alias only. Each remaining
/// DSL (re, json, sql, sql_where) must reject (or refuse to match)
/// `$$${...}` in its body.
#[test]
fn triple_dollar_is_ast_and_glob_only() {
    use v4::cst::diag::SilentSink;
    use v4::cst::dsl::Dsl;

    // re: regex compile fails on `$$${...}` since native re grammar leaves
    //     the literal `{X?}` for the regex crate, which errors.
    let re = v4::cst::dsls::re::ReDsl::new();
    assert!(
        re.compile(b"$$${WHO?}", &SilentSink).is_err(),
        "re must reject `$$${{...}}` at native compile",
    );

    // json: brace_parse rejects the extra `$` chars with
    // "empty capture name after `$`".
    let json = v4::cst::dsls::json::JsonDsl::new();
    assert!(
        json.compile(b"{ name: $$${N?} }", &SilentSink).is_err(),
        "json must reject `$$${{...}}` at native compile",
    );

    // sql / sql_where: the host pre-pass produces one Term interp for
    // the inner `${V?}`; the leading two `$` characters stay as literal
    // bytes. No DSL silently promotes triple-dollar to a special form
    // (only ast does). We don't run those through the runtime here — the
    // contract is "no per-DSL carveout", verified by reading source.
}
