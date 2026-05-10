use std::sync::Arc;

use effect_runtime::v2::{
    expand, ExpandOpts, FactStore, MemFactStore, MemQueue, QueueBackend,
};

use v4::compile::parse::host_parse;
use v4::compile::walk::walk_program;
use v4::lower::{default_registry, LowerCtx};
use v4::Cursor;

fn run_in(root: &std::path::Path, src: &str) -> Arc<dyn FactStore<Cursor>> {
    let (program, parse_diags) = host_parse(src);
    assert!(parse_diags.is_empty(), "parse: {parse_diags:?}");

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
    for pipe in pipes {
        expand(
            &pipe.into_instance(),
            queue.clone(),
            vec![Arc::new(Cursor::default())],
            ExpandOpts::default(),
        );
    }

    store
}

#[test]
fn ast_yaml_rulecore_matches_and_rewrites_sprf_carveout_captures() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("lib.rs"),
        r#"
fn keep() {
    value.to_string();
}

mod tests {
    fn skip() {
        value.to_string();
    }
}
"#,
    )
    .unwrap();

    let store = run_in(
        tmp.path(),
        r#"
rule(:hits, X?);

fs
  > glob`**/*.rs`
  > ast_yaml(:rs)`
      rule:
        pattern: "${X?}.to_string()"
        not:
          inside:
            kind: mod_item
    `
  > hits.(X);
"#,
    );

    let rows = store.rows_of("hits");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("X"), Some("value"));
}

#[test]
fn ast_yaml_bare_pattern_body_wraps_under_rule() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("lib.rs"), "fn alpha() {}\n").unwrap();

    let store = run_in(
        tmp.path(),
        r#"
rule(:hits, NAME?);

fs
  > glob`**/*.rs`
  > ast_yaml(:rs)`pattern: "fn ${NAME?}() {}"`
  > hits.(NAME);
"#,
    );

    let rows = store.rows_of("hits");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("NAME"), Some("alpha"));
}

#[test]
fn ast_yaml_can_capture_function_name_from_preceded_doc_comment() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("lib.rs"),
        r#"
/// Builds the user service.
fn documented() {}
"#,
    )
    .unwrap();

    let store = run_in(
        tmp.path(),
        r#"
rule(:docs, NAME?, DOC?);

fs
  > glob`**/*.rs`
  > ast_yaml(:rs)`
      rule:
        kind: line_comment
        regex: "^///"
        precedes:
          pattern: "fn ${NAME?}() {}"
    `
  > term_bind(:DOC)
  > docs.(NAME, DOC);
"#,
    );

    let rows = store.rows_of("docs");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("NAME"), Some("documented"));
    assert_eq!(rows[0].get("DOC"), Some("/// Builds the user service.\n"));
}
