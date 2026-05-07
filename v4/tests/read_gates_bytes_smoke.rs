//! `read` gates byte access for matchers. v4 substrate purification:
//!   • `read` is the ONE op allowed to materialize file bytes.
//!   • `re` / `ast` / `ast_nm` / `json` consume `cursor.value` bytes
//!     that arrived via an upstream `read`.
//!
//! Without `read` upstream, matchers must NOT auto-load file bytes.
//! cursor.value still carries the path (set by `fs`/`glob`); the
//! matcher sees a path string as the haystack, which doesn't match
//! the patterns under test.
//!
//! The four cases assert:
//!   1. `fs > glob > read > re`     — match fires (bytes loaded by read)
//!   2. `fs > glob > re`             — no match (matcher sees path text)
//!   3. `fs > glob > read > ast(:c)` — match fires
//!   4. `fs > glob > read > json`    — match fires

use std::sync::Arc;
use effect_runtime::v2::{
    expand, ExpandOpts, FactStore, MemFactStore, MemQueue, QueueBackend,
};
use v4::Cursor;
use v4::compile::parse::host_parse;
use v4::compile::walk::walk_program;
use v4::lower::{default_registry, LowerCtx};

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

/// `fs > glob > read > re` — green. read materializes bytes, re matches
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

/// `fs > glob > re` — RED before purification, green after. Without
/// `read` upstream, re must NOT auto-load file bytes. cursor.value
/// holds a path string after `fs`/`glob`; the regex doesn't match the
/// path so zero matches are emitted.
#[test]
fn re_without_read_does_not_auto_load_bytes() {
    let tmp = tempfile::tempdir().unwrap();
    // Path doesn't contain "hello"; only the file content does.
    std::fs::write(tmp.path().join("a.txt"), b"prefix hello world suffix").unwrap();

    let store = run_in(
        tmp.path(),
        "rule(:nope) { fs > glob`**/*.txt` > re`hello` };",
    );
    assert_eq!(
        store.len("nope"), 0,
        "re must not auto-read file bytes; path text doesn't match `hello`",
    );
}

/// `fs > glob > read > ast(:c)` — green. ast walks bytes loaded by read.
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

/// `fs > glob > read > json` — green. json walks bytes loaded by read.
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
