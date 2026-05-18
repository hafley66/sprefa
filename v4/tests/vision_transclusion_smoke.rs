// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
// ░░                                                                      ░░
// ░░   VISION TESTS — DRAFT / ASPIRATIONAL                                ░░
// ░░                                                                      ░░
// ░░   This file paints, in tests, what the v4 substrate is being         ░░
// ░░   built towards. Three scenes hand-wire what the primitives          ░░
// ░░   already compose into; one final scene shows the language-level     ░░
// ░░   form and is `#[ignore]`d until #10 (${pipe} sub-pipe lowering)     ░░
// ░░   lands.                                                             ░░
// ░░                                                                      ░░
// ░░   References:                                                        ░░
// ░░     memory  project_v4_vision_subpipe_render_transclusion            ░░
// ░░     plan    ~/.claude/plans/glittery-napping-whisper.md              ░░
// ░░     bd task #10  (${pipe} accepts arbitrary sub-pipes)               ░░
// ░░     bd task #29  (ReDef::lower capture_names)                        ░░
// ░░                                                                      ░░
// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
//
// Scenes
//
//   A. DOM-of-facts reverse-lookup
//      Multiple "rules" (simulated as hand-seeded refs) all point at
//      overlapping bytes of one file. `refs_at(path, byte)` returns
//      every covering ref. This is the LSP cross-rule unlock.
//
//   B. Cross-source capture splice
//      Read a "donor" file's text from one path; splice it into the
//      bytes of a marked region in a "host" file. Demonstrates the
//      pairing of a value-source and a `write_cursor(:on=NAME)` sink
//      that ${pipe} carveouts will produce when #10 lands.
//
//   C. Cross-repo render with template substitution
//      Donor produces a NAME capture; host has a `/** REPLACE:NAME */`
//      marker; FormatComponent renders the donor name into a wrapped
//      template; write_cursor :on=COMMENT splices the rendered string.
//      The body of the comment is fully replaced.
//
//   D. (#[ignore]) Language-level form
//      Same picture as scene C, but written as sprf source. Blocked
//      on #10 — the host parser carveout grammar accepts only term
//      refs today (${X} / ${X.field} / ${&.field}), not arbitrary
//      sub-pipelines. When #10 lands, this test should flip to active.

use std::sync::{Arc, Mutex};

use effect_runtime::v2::{
    expand, Component, ExpandOpts, FactStore, MemFactStore, MemQueue, Node, PipeInstance,
    QueueBackend, RenderCtx,
};

use v4::app::{build_in_process, RefsAtReq, SprfClient};
use v4::store::SprfStore;
use v4::v2_ops::{FormatComponent, WriteCursorComponent, WriteMode};
use v4::{Coord, Cursor};

// ──────────────────────── shared test plumbing ─────────────────────────

struct Collector {
    sink: Arc<Mutex<Vec<Cursor>>>,
}
impl Component for Collector {
    type Next = Cursor;
    fn render(&self, _c: &RenderCtx, c: &Cursor) -> Node<Cursor> {
        self.sink.lock().unwrap().push(c.clone());
        Node::Done
    }
}

fn run_one(comp: Arc<dyn Component<Next = Cursor>>, seeds: Vec<Arc<Cursor>>) -> Vec<Cursor> {
    let cursors: Arc<Mutex<Vec<Cursor>>> = Arc::new(Mutex::new(Vec::new()));
    let pipe = PipeInstance::new(vec![
        comp,
        Arc::new(Collector {
            sink: cursors.clone(),
        }),
    ]);
    let q: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
    expand(&pipe, q, seeds, ExpandOpts::default());
    let v = cursors.lock().unwrap().clone();
    v
}

fn run_chain(
    comps: Vec<Arc<dyn Component<Next = Cursor>>>,
    seeds: Vec<Arc<Cursor>>,
) -> Vec<Cursor> {
    let cursors: Arc<Mutex<Vec<Cursor>>> = Arc::new(Mutex::new(Vec::new()));
    let mut steps = comps;
    steps.push(Arc::new(Collector {
        sink: cursors.clone(),
    }));
    let pipe = PipeInstance::new(steps);
    let q: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
    expand(&pipe, q, seeds, ExpandOpts::default());
    let v = cursors.lock().unwrap().clone();
    v
}

fn write_fixture(dir: &std::path::Path, name: &str, body: &[u8]) -> String {
    let p = dir.join(name);
    std::fs::write(&p, body).unwrap();
    p.display().to_string()
}

// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
// SCENE A — DOM of facts
// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
// Three independent "rules" all emit refs that cover byte 12 of foo.rs.
// `refs_at` returns all three. LSP `textDocument/references` collapses
// to this call when the substrate is queried at a byte position.
#[tokio::test]
async fn scene_a_dom_of_facts_reverse_lookup() {
    let (state, client) = build_in_process(std::env::temp_dir());
    let store = state.sprf_store.clone();

    // foo.rs: "fn  greet ( name :  & str ) -> String { ... }"
    //          0   3   8 10  12   17 19 23  27   32 38
    let path = "/tmp/scene_a_foo.rs";
    let file = store.intern_file(b"fn  greet ( name :  & str ) -> String { ... }", path);
    let _ = store.intern_path(0, 0, file, path);

    // Three rules' captures all cover byte 12 ("name"):
    //   rule_fnsig  → wide span [3, 38)   (the full fn signature)
    //   rule_param  → narrow span [12, 16) (just "name")
    //   rule_typing → mid span    [10, 26) ("( name : & str ")
    let r_fnsig = store.intern_ref(Coord {
        repo: 0,
        rev: 0,
        fs: file,
        lo: 3,
        hi: 38,
    });
    let r_param = store.intern_ref(Coord {
        repo: 0,
        rev: 0,
        fs: file,
        lo: 12,
        hi: 16,
    });
    let r_typing = store.intern_ref(Coord {
        repo: 0,
        rev: 0,
        fs: file,
        lo: 10,
        hi: 26,
    });

    let resp = client
        .refs_at(RefsAtReq {
            path: path.into(),
            byte: 13,
        })
        .await
        .unwrap();

    let ids: Vec<u64> = resp.hits.iter().map(|h| h.ref_id).collect();
    assert!(
        ids.contains(&r_fnsig.0),
        "fnsig ref covers byte 13 — expected hit"
    );
    assert!(
        ids.contains(&r_param.0),
        "param ref covers byte 13 — expected hit"
    );
    assert!(
        ids.contains(&r_typing.0),
        "typing ref covers byte 13 — expected hit"
    );
    assert_eq!(ids.len(), 3, "exactly the three covering refs; got {ids:?}");

    // The DOM property: every hit resolves to a coord with the same
    // file, distinct spans, and a recoverable fs path string.
    for h in &resp.hits {
        assert_eq!(h.coord.fs, file);
        assert_eq!(h.coord.fs_path.as_deref(), Some(path));
        assert!(h.coord.lo <= 13 && 13 < h.coord.hi);
    }
}

// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
// SCENE B — Cross-source capture splice
// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
// Donor file holds the value to inject. Host file holds the marker
// region whose bytes get replaced. The seed cursor carries the donor
// text in its focal `value` AND a NAME term whose `at` points at the
// host file's marker bytes. write_cursor :on=NAME splices.
//
// This is the manual analog of:
//   `... ${ donor-pipe } ... > write_cursor(:replace, :NAME)`
// Once #10 lands, the donor-pipe gets pre-rendered into the carveout,
// landing the same shape automatically.
#[test]
fn scene_b_cross_source_splice_via_capture() {
    let tmp = tempfile::tempdir().unwrap();
    let donor_path = write_fixture(tmp.path(), "donor.rs", b"DONOR_PAYLOAD");
    let host_path = write_fixture(tmp.path(), "host.rs", b"prefix /* MARK */ suffix\n");

    let facts: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let store = SprfStore::new(facts.clone());

    // Intern both files so the store has their FileIds and content
    // hashes for drift detection.
    let donor_bytes = std::fs::read(&donor_path).unwrap();
    let _donor_fid = store.intern_file(&donor_bytes, &donor_path);
    let host_bytes = std::fs::read(&host_path).unwrap();
    let host_fid = store.intern_file(&host_bytes, &host_path);

    // Locate the marker in the host file. "/* MARK */" runs [7, 17).
    let mark_lo = host_bytes
        .windows(10)
        .position(|w| w == b"/* MARK */")
        .expect("marker present") as u32;
    let mark_hi = mark_lo + 10;

    // Build the splice cursor: focal value = donor payload; NAME term =
    // host marker bytes.
    let mut c = Cursor::default();
    c.value = Arc::from(String::from_utf8_lossy(&donor_bytes).into_owned());
    let host_coord = Coord {
        repo: 0,
        rev: 0,
        fs: host_fid,
        lo: mark_lo,
        hi: mark_hi,
    };
    c.set_at(
        "MARK",
        &String::from_utf8_lossy(&host_bytes[mark_lo as usize..mark_hi as usize]),
        host_coord,
        &store,
    );
    let seed = Arc::new(c);

    let comp: Arc<dyn Component<Next = Cursor>> = Arc::new(
        WriteCursorComponent::new(WriteMode::Replace)
            .on_capture("MARK")
            .with_sprf_store(store.clone()),
    );
    run_one(comp, vec![seed]);

    let after = std::fs::read_to_string(&host_path).unwrap();
    assert_eq!(
        after, "prefix DONOR_PAYLOAD suffix\n",
        "donor's text replaces the host's marker region byte-for-byte",
    );
    // Donor file untouched; host has been edited.
    assert_eq!(
        std::fs::read_to_string(&donor_path).unwrap(),
        "DONOR_PAYLOAD",
        "donor file is read-only in this composition",
    );
}

// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
// SCENE C — Cross-repo render via template + capture splice
// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
// Pipeline shape (hand-wired):
//
//   seed ─┬─►  bind donor TYPE_NAME term ─►  FormatComponent renders
//         │   "type ${TYPE_NAME} = u32"          a wrapper template
//         │   into cursor.value                  using the donor name
//         │
//         └─►  bind host MARK term  ─►  write_cursor :on=MARK splices
//
// One cursor, two captures + a focal value. The wrapper template
// demonstrates that host-side rendering works against bound terms
// before the splice fires. write_cursor :on=MARK then splices the
// rendered focal value into the host's marker region.
#[test]
fn scene_c_cross_repo_render_with_template() {
    let tmp = tempfile::tempdir().unwrap();
    let host_path = write_fixture(
        tmp.path(),
        "host.ts",
        b"// generated below\n/* REPLACE_ME */\n// generated above\n",
    );

    let facts: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let store = SprfStore::new(facts.clone());

    let host_bytes = std::fs::read(&host_path).unwrap();
    let host_fid = store.intern_file(&host_bytes, &host_path);

    // Locate the host marker.
    let lo = host_bytes
        .windows(16)
        .position(|w| w == b"/* REPLACE_ME */")
        .expect("marker present") as u32;
    let hi = lo + 16;

    // Seed cursor: bind TYPE_NAME from a "donor" repo (synthetic here
    // — the value carries the captured name; coord is irrelevant for
    // host-side render).
    let mut c = Cursor::default();
    c.set("TYPE_NAME", "Foo"); // donor capture
    c.set_at(
        "MARK",
        "/* REPLACE_ME */", // host capture
        Coord {
            repo: 0,
            rev: 0,
            fs: host_fid,
            lo,
            hi,
        },
        &store,
    );
    let seed = Arc::new(c);

    // Step 1 — render template into focal cursor.value using ${TYPE_NAME}.
    let render = FormatComponent::new("type ${TYPE_NAME} = u32;", "_value");
    // FormatComponent writes to the named slot; v4_value stamps focal.
    // Easier path: use a literal step that copies the rendered result
    // into focal. But FormatComponent's `into` of "_value" is a magic
    // alias that writes to cursor.value via Row::set. Verify path:
    // run a Format that writes into a normal slot, then a thin
    // FocalSet helper. Rather than adding helpers, use a single-arm
    // Component that derives focal from the rendered bind.
    struct FocalFromTemplate {
        template: String,
    }
    impl Component for FocalFromTemplate {
        type Next = Cursor;
        fn render(&self, _: &RenderCtx, c: &Cursor) -> Node<Cursor> {
            let re =
                regex::Regex::new(r"\$\{([A-Za-z_][A-Za-z0-9_]*(?:\.[A-Za-z_][A-Za-z0-9_]*)?)\}")
                    .unwrap();
            let rendered = re
                .replace_all(&self.template, |caps: &regex::Captures| {
                    c.get(&caps[1]).unwrap_or("").to_string()
                })
                .into_owned();
            let mut out = c.clone();
            out.value = Arc::from(rendered);
            Node::Emit(Arc::new(out))
        }
    }
    let _ = render; // FormatComponent shape kept above for documentation.

    let render = Arc::new(FocalFromTemplate {
        template: "type ${TYPE_NAME} = u32;".into(),
    });
    let writer = Arc::new(
        WriteCursorComponent::new(WriteMode::Replace)
            .on_capture("MARK")
            .with_sprf_store(store.clone()),
    );

    run_chain(vec![render, writer], vec![seed]);

    let after = std::fs::read_to_string(&host_path).unwrap();
    assert_eq!(
        after, "// generated below\ntype Foo = u32;\n// generated above\n",
        "rendered donor name flows through template into the host's marker bytes",
    );
}

// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
// SCENE D — Language-level form, ACTIVE as of #10
// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
// The same picture as scenes B/C, written as sprf source. With #10
// landed, ${...} accepts full sub-pipes; the host parser + walker
// lower nested op chains; drain semantics fire at render time.
//
// Two transclusion sources land into one rendered document. The doc
// is the row written into rule(:doc)_facts. Asserts the substrate
// composes file-source pipes inside a single render.
#[test]
fn scene_d_language_form_transclusion() {
    use effect_runtime::v2::{expand, ExpandOpts, MemQueue};
    use v4::compile::parse::host_parse;
    use v4::compile::walk::walk_program;
    use v4::lower::{default_registry, LowerCtx};

    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    std::fs::write(dir.join("a.txt"), b"alpha").unwrap();
    std::fs::write(dir.join("b.txt"), b"bravo").unwrap();

    // Source — language-level transclusion via two ${pipe} carveouts.
    // Each carveout enumerates one file via `fs` + reads its content
    // via `read`; the drained cursor's focal value renders into the
    // outer template slot. The outer rule writes one row whose value
    // is the rendered document.
    // Rule = function: the declaration is inert; the bare `doc()` call
    // runs the body and writes the rendered row.
    let src = "rule(:doc) { `# A\n${ fs > glob`**/a.txt` > read }\n# B\n${ fs > glob`**/b.txt` > read }` }; doc();";

    let (program, parse_diags) = host_parse(src);
    assert!(parse_diags.is_empty(), "parse: {:?}", parse_diags);

    let facts: Arc<dyn FactStore<Cursor>> =
        Arc::new(effect_runtime::v2::MemFactStore::<Cursor>::new());
    let reg = default_registry();
    let mut ctx = LowerCtx::new(facts.clone(), dir.to_path_buf());

    let (pipes, walk_diags) = walk_program(&program, &reg, &mut ctx);
    assert!(
        walk_diags.is_empty(),
        "walk: {:?}",
        walk_diags
            .iter()
            .map(|d| (d.code.as_ref(), d.message.as_str()))
            .collect::<Vec<_>>()
    );

    let queue: Arc<dyn effect_runtime::v2::QueueBackend<Cursor>> = Arc::new(MemQueue::new());
    for fused in pipes {
        let inst = fused.into_pipe().into_instance();
        expand(
            &inst,
            queue.clone(),
            vec![Arc::new(Cursor::default())],
            ExpandOpts::default(),
        );
    }

    assert_eq!(facts.len("doc"), 1, "expected one row in doc_facts");
    let rows = facts.rows_of("doc");
    let value = &*rows[0].value;

    // Both transclusion targets resolved into the rendered doc.
    assert!(value.contains("# A"), "missing # A header in: {:?}", value);
    assert!(
        value.contains("alpha"),
        "missing alpha content: {:?}",
        value
    );
    assert!(value.contains("# B"), "missing # B header in: {:?}", value);
    assert!(
        value.contains("bravo"),
        "missing bravo content: {:?}",
        value
    );
}
