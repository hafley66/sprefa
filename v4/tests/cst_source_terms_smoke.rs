use std::sync::{Arc, Mutex};

use effect_runtime::v2::{
    expand, Component, ExpandOpts, FactStore, MemFactStore, MemQueue, Node, PipeInstance,
    QueueBackend, RenderCtx,
};
use v4::compile::parse::host_parse;
use v4::compile::walk::walk_program;
use v4::lower::{default_registry, LowerCtx};
use v4::store::SprfStore;
use v4::term::Term;
use v4::{Coord, Cursor, WhereBytesId};

#[derive(Clone)]
struct Sink {
    rows: Arc<Mutex<Vec<Cursor>>>,
}

impl Component for Sink {
    type Next = Cursor;

    fn render(&self, _ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
        self.rows.lock().unwrap().push(c.clone());
        Node::Done
    }
}

fn run_collect(root: &std::path::Path, src: &str) -> (Arc<SprfStore>, Vec<Cursor>) {
    let facts: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let store = SprfStore::new(facts.clone());
    let (program, parse_diags) = host_parse(src);
    assert!(parse_diags.is_empty(), "parse diags: {parse_diags:?}");

    let reg = default_registry();
    let mut ctx = LowerCtx::new(facts, root.to_path_buf()).with_sprf_store(store.clone());
    let (pipes, walk_diags) = walk_program(&program, &reg, &mut ctx);
    assert!(walk_diags.is_empty(), "walk diags: {walk_diags:?}");

    let rows = Arc::new(Mutex::new(Vec::new()));
    let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
    for pipe in pipes {
        let mut steps: Vec<Arc<dyn Component<Next = Cursor>>> =
            pipe.steps.iter().cloned().collect();
        steps.push(Arc::new(Sink { rows: rows.clone() }));
        expand(
            &PipeInstance::new(steps),
            queue.clone(),
            vec![Arc::new(Cursor::default().with_store(&store))],
            ExpandOpts::default(),
        );
    }
    store.flush();
    let out = rows.lock().unwrap().clone();
    (store, out)
}

#[test]
fn term_bind_preserves_focal_source_metadata() {
    let facts: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let store = SprfStore::new(facts);
    let coord = Coord {
        repo: 0,
        rev: 0,
        fs: store.intern_file(b"let s = \"alpha\";", "a.rs"),
        lo: 8,
        hi: 15,
    };
    let mut seed = Cursor::default().with_store(&store);
    seed.set_at("MATCH", "\"alpha\"", coord, &store);
    let match_term = seed
        .terms
        .iter()
        .find(|t| t.name.as_ref() == "MATCH")
        .unwrap();
    let match_value = match_term.value.clone();
    let match_at = match_term.at;
    let match_cursor_value = match_term.cursor_value;
    seed.set_focal_at(match_value, match_at, match_cursor_value);

    let rows = Arc::new(Mutex::new(Vec::new()));
    let pipe = PipeInstance::new(vec![
        Arc::new(Term::bind("S")) as Arc<dyn Component<Next = Cursor>>,
        Arc::new(Sink { rows: rows.clone() }),
    ]);
    let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
    expand(&pipe, queue, vec![Arc::new(seed)], ExpandOpts::default());

    let got = rows.lock().unwrap();
    let s = got[0]
        .terms
        .iter()
        .find(|t| t.name.as_ref() == "S")
        .unwrap();
    assert_eq!(s.value.as_ref(), "\"alpha\"");
    assert_eq!(s.at, match_at);
    assert_eq!(got[0].get("S.lo"), Some("8"));
    assert_eq!(got[0].get("S.hi"), Some("15"));
}

#[test]
fn cst_capture_interpolation_binds_source_bearing_term() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("a.rs"),
        "fn main() { let s = \"alpha\"; }\n",
    )
    .unwrap();

    let (store, rows) = run_collect(
        dir.path(),
        r#"
        fs(glob`**/*.rs`)
          > cst(:rust)`(string_literal) @${S?}`;
    "#,
    );

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("S"), Some("\"alpha\""));
    assert_eq!(rows[0].get("S.lo"), Some("20"));
    assert_eq!(rows[0].get("S.hi"), Some("27"));
    assert!(rows[0].get("S.fs").unwrap().ends_with("a.rs"));
    let s = rows[0]
        .terms
        .iter()
        .find(|t| t.name.as_ref() == "S")
        .unwrap();
    let where_bytes = store
        .where_bytes_of(WhereBytesId::from(s.at))
        .expect("S term has where-bytes");
    assert_eq!(
        store.lookup_string(where_bytes.string).as_deref(),
        Some("\"alpha\"")
    );
}

#[test]
fn cst_capture_interpolation_can_transform_before_binding() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("a.rs"),
        "fn main() { let s = \"alpha beta\"; }\n",
    )
    .unwrap();

    let (_store, rows) = run_collect(
        dir.path(),
        r#"
        fs(glob`**/*.rs`)
          > cst(:rust)`(string_literal) @${ split(WORD?)` ` > WORD? }`;
    "#,
    );

    let values: Vec<&str> = rows.iter().filter_map(|r| r.get("WORD")).collect();
    assert_eq!(values, vec!["\"alpha", "beta\""]);
}

#[test]
fn strings_wrapper_emits_string_terms_consumable_by_rule_ops() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("a.rs"),
        "fn main() { let s = \"alpha\"; }\n",
    )
    .unwrap();

    let (store, _rows) = run_collect(
        dir.path(),
        r#"
        rule(:string_hits, STRING?, LO?, FS?);
        rule(:collect_strings, STRING?) {
          fs(glob`**/*.rs`)
            > strings(:rust)
            > rule(:string_hits, STRING: STRING, LO: STRING.lo, FS: STRING.fs)
        };

        collect_strings(STRING?);
    "#,
    );

    let hits = store.inner().rows_of("string_hits");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].get("STRING"), Some("\"alpha\""));
    assert_eq!(hits[0].get("LO"), Some("20"));
    assert!(hits[0].get("FS").unwrap().ends_with("a.rs"));
}
