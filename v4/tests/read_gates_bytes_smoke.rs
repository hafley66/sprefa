//! Source-aware matcher smoke tests. File/tree matchers can consume either:
//!   - materialized bytes in cursor.value from an upstream `read`
//!   - path/rev cursors, loading bytes internally through the source layer
//!
//! The cases assert:
//!   1. `fs > glob > read > re`     - match fires (bytes loaded by read)
//!   2. `fs > glob > re`            - no match (regex sees path text)
//!   3. `fs > glob > read > ast`    - match fires
//!   4. `fs > glob > ast`           - match source-reads path cursor
//!   5. `fs > glob > read > json`   - match fires
//!   6. `fs > glob > json`          - match source-reads path cursor
//!   7. `path`...` > read`          - path expands against run root

use std::sync::Arc;
use effect_runtime::v2::{
    expand, ExpandOpts, FactStore, MemFactStore, MemQueue, QueueBackend,
};
use v4::Cursor;
use v4::compile::parse::host_parse;
use v4::compile::walk::walk_program;
use v4::lower::{default_registry, LowerCtx};
use v4::source::SourceReader;

fn run_in(root: &std::path::Path, src: &str) -> Arc<dyn FactStore<Cursor>> {
    let (program, parse_diags) = host_parse(src);
    assert!(parse_diags.is_empty(), "parse: {:?}", parse_diags);
    let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let reg = default_registry();
    let mut ctx = LowerCtx::new(store.clone(), root.to_path_buf());
    let (pipes, walk_diags) = walk_program(&program, &reg, &mut ctx);
    assert!(walk_diags.is_empty(), "walk: {:?}",
        walk_diags.iter().map(|d| (d.code.as_ref(), d.message.as_str())).collect::<Vec<_>>());
    let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
    for pipe in pipes {
        let inst = pipe.into_instance();
        expand(&inst, queue.clone(), vec![Arc::new(Cursor::default())], ExpandOpts::default());
    }
    store
}

#[test]
fn source_reader_preserves_relative_cursor_path_identity() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("a.txt"), b"hello").unwrap();

    let mut cursor = Cursor::default();
    cursor.value = Arc::from("a.txt");

    let source = SourceReader::new(tmp.path(), None, None)
        .read_cursor(&cursor)
        .expect("relative path cursor should read through root");
    assert_eq!(source.bytes, b"hello");
    assert_eq!(source.path.as_ref(), "a.txt");
}

/// `fs > glob > read > re` - green. read materializes bytes, re matches
/// against cursor.value bytes.
#[test]
fn re_matches_after_read_loads_bytes() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("a.txt"), b"prefix hello world suffix").unwrap();

    let store = run_in(
        tmp.path(),
        "rule(:hits) { fs > glob`**/*.txt` > read > re`hello` };",
    );
    assert_eq!(
        store.len("hits"), 1,
        "read makes bytes available to re; one match expected",
    );
}

/// `fs > glob > re` - re is a string matcher. It does not read files
/// implicitly when cursor.value still carries a path.
#[test]
fn re_without_read_does_not_auto_load_bytes() {
    let tmp = tempfile::tempdir().unwrap();
    // Path doesn't contain "hello"; only the file content does.
    std::fs::write(tmp.path().join("a.txt"), b"prefix hello world suffix").unwrap();

    let store = run_in(
        tmp.path(),
        "rule(:hits) { fs > glob`**/*.txt` > re`hello` };",
    );
    assert_eq!(
        store.len("hits"), 0,
        "re should only inspect cursor.value; explicit read is required",
    );
}

/// `path`...` > read` - path resolves relative to the run root and
/// then read materializes that file.
#[test]
fn path_literal_resolves_before_read() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("cfg")).unwrap();
    std::fs::write(
        tmp.path().join("cfg/sprefa.toml"),
        br#"repos = [{ slug = "demo", root = "/tmp/demo", remote = "origin" }]"#,
    ).unwrap();

    let store = run_in(
        tmp.path(),
        r#"rule(:repos, SLUG?, ROOT?, REMOTE?);
           path`cfg/sprefa.toml`
             > read
             > json(:toml)`{ repos: [ ... { slug: ${SLUG?}, root: ${ROOT?}, remote: ${REMOTE?} } ] }`
             > repos(SLUG, ROOT, REMOTE);"#,
    );
    assert_eq!(store.len("repos"), 1);
    let rows = store.rows_of("repos");
    assert_eq!(rows[0].get("SLUG"), Some("demo"));
    assert_eq!(rows[0].get("ROOT"), Some("/tmp/demo"));
    assert_eq!(rows[0].get("REMOTE"), Some("origin"));
}

/// `fs > glob > read > ast(:c)` - green. ast walks bytes loaded by read.
#[test]
fn ast_matches_after_read_loads_bytes() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("k.c"),
        b"int main(void) { printk(\"hi\"); return 0; }\n",
    ).unwrap();

    let store = run_in(
        tmp.path(),
        "rule(:hits) { fs > glob`**/*.c` > read > ast(:c)`printk($$$)` };",
    );
    assert_eq!(
        store.len("hits"), 1,
        "ast over read bytes; one printk call site",
    );
}

/// `fs > glob > ast(:c)` - ast source-reads from path cursors without
/// requiring an explicit `read`.
#[test]
fn ast_without_read_source_reads_path_cursor() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("k.c"),
        b"int main(void) { printk(\"hi\"); return 0; }\n",
    ).unwrap();

    let store = run_in(
        tmp.path(),
        "rule(:hits) { fs > glob`**/*.c` > ast(:c)`printk($$$)` };",
    );
    assert_eq!(
        store.len("hits"), 1,
        "ast should source-read file bytes from the path cursor",
    );
}

/// `fs > glob > read > json` - green. json walks bytes loaded by read.
#[test]
fn json_matches_after_read_loads_bytes() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("u.json"),
        br#"{"name":"alice","age":30}"#,
    ).unwrap();

    let store = run_in(
        tmp.path(),
        "rule(:hits) { fs > glob`**/*.json` > read > json`{ name: $N, age: $AGE }` };",
    );
    assert_eq!(
        store.len("hits"), 1,
        "json over read bytes; one matching object",
    );
    let rows = store.rows_of("hits");
    assert_eq!(rows[0].get("N"),   Some("alice"));
    assert_eq!(rows[0].get("AGE"), Some("30"));
}

/// `fs > glob > json` - json source-reads from path cursors without
/// requiring an explicit `read`.
#[test]
fn json_without_read_source_reads_path_cursor() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("u.json"),
        br#"{"name":"alice","age":30}"#,
    ).unwrap();

    let store = run_in(
        tmp.path(),
        "rule(:hits) { fs > glob`**/*.json` > json`{ name: $N, age: $AGE }` };",
    );
    assert_eq!(
        store.len("hits"), 1,
        "json should source-read file bytes from the path cursor",
    );
    let rows = store.rows_of("hits");
    assert_eq!(rows[0].get("N"),   Some("alice"));
    assert_eq!(rows[0].get("AGE"), Some("30"));
}
