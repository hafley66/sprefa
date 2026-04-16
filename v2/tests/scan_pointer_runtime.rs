//! Content-side stamping — walker ops stamp `scan_pointer` + `Tri::Claimed`
//! on Captures whose var is named in a `$$sigil($VAR)` annotation.
//!
//! This is the scout half. A separate checker pass downgrades unverified
//! claims. Non-annotated captures stay `scan_pointer: None`, default Tri.

use v2::ops::default_registry;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use futures::executor::block_on;
use futures_util::stream::StreamExt;

use v2::_0_types::{Cursor, Tri};
use v2::{
    host_parse, lower_rules, run_scan_loop, Config, Diagnostic, DiagSink, EventSink, MemReader,
    MemWriter, OpCtx, OpId, OperatorRegistry, ProgramCtx, ResultStore, RunId, RuntimeConfig,
    DEFAULT_DEPTH,
};

fn make_config() -> Arc<Config> {
    Arc::new(Config {
        repos:        vec![],
        revs:         vec![],
        fs_exclude:   vec![],
        sprf_files:   vec![],
        shell_allow:  vec![],
        runtime: RuntimeConfig {
            worker_threads:       1,
            buffer_size:          256,
            flush_interval_ms:    100,
            collect_witnesses:    true,
            xref_cartesian_limit: 10_000,
        },
        content_hash: 0,
    })
}

fn make_registry() -> Arc<OperatorRegistry> {
    Arc::new(default_registry())
}

fn run_rule(src: &str, reader: Arc<MemReader>) -> Vec<Cursor> {
    let cfg = reader.config.clone();
    let invs = host_parse(src, Arc::from(PathBuf::from("<test>").as_path())).unwrap();
    let pctx = ProgramCtx::new(cfg.clone(), make_registry());
    let outcome = lower_rules(invs, pctx);
    assert!(
        outcome.diags.is_empty(),
        "lower diags: {:?}",
        outcome.diags.iter().map(|d| d.code()).collect::<Vec<_>>()
    );

    let writer = Arc::new(MemWriter::new());
    let (result_store, xref_seen) = OpCtx::fresh_xref_state();
    let ctx = OpCtx {
        run_id: RunId(1),
        op_id:  OpId(0),
        reader: reader.clone(),
        writer: writer.clone(),
        config: cfg.clone(),
        diags:  DiagSink(Arc::new(|_| {})),
        events: EventSink(Arc::new(|_| {})),
        result_store,
        xref_seen,
    };
    let empty: futures_core::stream::BoxStream<'static, Arc<[Cursor]>> =
        futures_util::stream::iter(Vec::<Arc<[Cursor]>>::new()).boxed();
    let batches: Vec<Arc<[Cursor]>> =
        block_on(outcome.pipelines[0].run(empty, ctx).collect());
    batches.into_iter()
        .flat_map(|b| b.iter().cloned().collect::<Vec<_>>())
        .collect()
}

#[test]
fn json_annotated_capture_gets_claimed_stamp() {
    let cfg = make_config();
    let doc = r#"{"image":{"repository":"myorg/auth","tag":"v1.2.3"},"name":"svc"}"#;
    let reader = Arc::new(
        MemReader::new(cfg)
            .with_repo("consumer", &["main"])
            .with_files("consumer", "main", &["deploy/services.yaml"])
            .with_content("consumer", "main", "deploy/services.yaml", doc.as_bytes()),
    );

    // $$repo($R) annotates — stamped. $NAME unannotated — bare.
    let src = r#"
        rule(svc) {
            > repo(consumer) > rev(main) > fs(**/services.yaml)
              > json({ image: { repository: $$repo($R) }, name: $NAME })
        };
    "#;
    let out = run_rule(src, reader);
    assert_eq!(out.len(), 1);

    let c = &out[0];

    let r_cap = c.captures.get("R").expect("$R must be bound");
    assert_eq!(r_cap.value.as_ref(), "myorg/auth");
    assert_eq!(
        r_cap.scan_pointer.as_deref(),
        Some("repo"),
        "annotated capture must carry sigil"
    );
    assert_eq!(
        r_cap.verified,
        Tri::Claimed,
        "annotated capture must be Claimed"
    );

    let n_cap = c.captures.get("NAME").expect("$NAME must be bound");
    assert_eq!(n_cap.value.as_ref(), "svc");
    assert!(
        n_cap.scan_pointer.is_none(),
        "unannotated capture must not carry a sigil"
    );
    // Default Tri is Claimed for value-only captures; walker stamping must
    // not change that path. What matters: no scan_pointer on unannotated.
}

// ---------------------------------------------------------------------------
// Checker pass — manually-built ResultStore, no walker dependency.
// ---------------------------------------------------------------------------

mod checker {
    use std::sync::Arc;

    use v2::_0_types::{Capture, Tri};
    use v2::_12_result_store::{CaptureMap, ResultStore};
    use v2::{check_scan_pointers, Config, RuntimeConfig};

    fn mk_config(repos: &[&str], revs: &[&str]) -> Config {
        Config {
            repos: repos.iter().map(|s| Arc::<str>::from(*s)).collect(),
            revs:  revs.iter().map(|s| Arc::<str>::from(*s)).collect(),
            fs_exclude:  vec![],
            sprf_files:  vec![],
            shell_allow: vec![],
            runtime: RuntimeConfig {
                worker_threads:       1,
                buffer_size:          64,
                flush_interval_ms:    100,
                collect_witnesses:    false,
                xref_cartesian_limit: 10_000,
            },
            content_hash: 0,
        }
    }

    fn claimed(value: &str, sigil: &str) -> Capture {
        Capture::new(Arc::from(value)).with_scan(Arc::from(sigil), Tri::Claimed)
    }

    fn row_with(var: &str, cap: Capture) -> CaptureMap {
        let mut m = CaptureMap::new();
        m.insert(Arc::<str>::from(var), cap);
        m
    }

    #[test]
    fn claimed_repo_in_config_flips_to_verified() {
        let mut store = ResultStore::new();
        let rule: Arc<str> = Arc::from("r");
        store.append(&rule, row_with("R", claimed("org/svc", "repo")));
        store.mark_complete(&rule);

        let cfg = mk_config(&["org/svc"], &[]);
        let diags = check_scan_pointers(&mut store, &cfg);

        assert!(diags.is_empty(),
            "expected no diags, got {:?}",
            diags.iter().map(|d| d.code()).collect::<Vec<_>>());

        let rows = store.rows_of(&rule).unwrap();
        assert_eq!(rows[0].get("R").unwrap().verified, Tri::Verified);
        assert!(store.unscanned().is_empty());
    }

    #[test]
    fn claimed_repo_not_in_config_flips_to_missing_and_warns() {
        let mut store = ResultStore::new();
        let rule: Arc<str> = Arc::from("r");
        store.append(&rule, row_with("R", claimed("org/missing", "repo")));
        store.mark_complete(&rule);

        let cfg = mk_config(&["org/svc"], &[]);
        let diags = check_scan_pointers(&mut store, &cfg);

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code(), "scan-pointer/unverified");

        let rows = store.rows_of(&rule).unwrap();
        assert_eq!(rows[0].get("R").unwrap().verified, Tri::Missing);
    }

    #[test]
    fn unscanned_returns_deduped_missing_pairs() {
        let mut store = ResultStore::new();
        let rule: Arc<str> = Arc::from("r");
        store.append(&rule, row_with("R", claimed("org/ghost", "repo")));
        store.append(&rule, row_with("R", claimed("org/ghost", "repo")));
        store.append(&rule, row_with("V", claimed("deadbeef", "rev")));
        store.mark_complete(&rule);

        let cfg = mk_config(&[], &[]);
        let _ = check_scan_pointers(&mut store, &cfg);

        let mut un = store.unscanned();
        un.sort();
        let as_strs: Vec<(String, String)> = un.iter()
            .map(|(s, v)| (s.to_string(), v.to_string()))
            .collect();
        assert_eq!(as_strs, vec![
            ("repo".to_string(), "org/ghost".to_string()),
            ("rev".to_string(),  "deadbeef".to_string()),
        ]);
    }
}

// ---------------------------------------------------------------------------
// Depth-4 diamond E2E
//
// Five repos A..E at rev=main. Each repo has a deps.json file whose content
// claims downstream repos via `$$repo($DEP)`. Chain + diagonals:
//
//     A ──► B ──► C ──► D ──► E
//     │    │    ▲      ▲
//     └────▼────┘      │
//          C ──────────┘  (diagonal dupe: C claimed by A and B, D by B and C)
//
// Seed cfg.repos = ["A"]. Walker seed narrows to cfg. Each pass discovers the
// next hop only once the prior hop is in config — real demand-driven chain.
//
// Expected: 4 passes, final cfg covers A..E, store's final state holds every
// (R, DEP) edge so a downstream consumer can reconstruct the graph.
// ---------------------------------------------------------------------------

fn mk_cfg(repos: &[&str]) -> Arc<Config> {
    Arc::new(Config {
        repos:        repos.iter().map(|s| Arc::<str>::from(*s)).collect(),
        revs:         vec![],
        fs_exclude:   vec![],
        sprf_files:   vec![],
        shell_allow:  vec![],
        runtime: RuntimeConfig {
            worker_threads:       1,
            buffer_size:          256,
            flush_interval_ms:    100,
            collect_witnesses:    true,
            xref_cartesian_limit: 10_000,
        },
        content_hash: 0,
    })
}

fn dep_doc(dep: &str) -> String {
    format!(r#"{{"dep":"{dep}"}}"#)
}

fn build_diamond_reader() -> Arc<MemReader> {
    // Five repos. Each has one or two deps.json files naming the next hops.
    let cfg = mk_cfg(&[]); // reader config separate from loop config
    let r = MemReader::new(cfg)
        // A → B, A → C
        .with_repo("A", &["main"])
        .with_files("A", "main", &["dep1.json", "dep2.json"])
        .with_content("A", "main", "dep1.json", dep_doc("B").as_bytes())
        .with_content("A", "main", "dep2.json", dep_doc("C").as_bytes())
        // B → C, B → D
        .with_repo("B", &["main"])
        .with_files("B", "main", &["dep1.json", "dep2.json"])
        .with_content("B", "main", "dep1.json", dep_doc("C").as_bytes())
        .with_content("B", "main", "dep2.json", dep_doc("D").as_bytes())
        // C → D
        .with_repo("C", &["main"])
        .with_files("C", "main", &["dep1.json"])
        .with_content("C", "main", "dep1.json", dep_doc("D").as_bytes())
        // D → E
        .with_repo("D", &["main"])
        .with_files("D", "main", &["dep1.json"])
        .with_content("D", "main", "dep1.json", dep_doc("E").as_bytes())
        // E clean
        .with_repo("E", &["main"])
        .with_files("E", "main", &[]);
    Arc::new(r)
}

#[test]
fn scan_loop_depth_4_diamond_converges_and_records_edges() {
    let reader = build_diamond_reader();

    let src = r#"
        rule(dep) {
            > repo($R) > rev(main) > fs(dep*.json)
              > json({ dep: $$repo($DEP) })
        };
    "#;
    let invs = host_parse(src, Arc::from(PathBuf::from("<test>").as_path())).unwrap();
    let pctx = ProgramCtx::new(mk_cfg(&[]), make_registry());
    let outcome = lower_rules(invs, pctx);
    assert!(
        outcome.diags.is_empty(),
        "lower diags: {:?}",
        outcome.diags.iter().map(|d| d.code()).collect::<Vec<_>>()
    );
    let pipeline = Arc::new(outcome.pipelines.into_iter().next().unwrap());

    let store = Arc::new(ResultStore::new());
    let initial_cfg = mk_cfg(&["A"]);
    let writer = Arc::new(MemWriter::new());

    // run_pass: drive the one pipeline under the current cfg + shared store.
    let pipeline_r = pipeline.clone();
    let writer_r   = writer.clone();
    let reader_r   = reader.clone();
    let run_pass = move |cfg: Arc<Config>, shared_store: Arc<ResultStore>| {
        let (_fresh, xref_seen) = OpCtx::fresh_xref_state();
        let ctx = OpCtx {
            run_id: RunId(1),
            op_id:  OpId(0),
            reader: reader_r.clone(),
            writer: writer_r.clone(),
            config: cfg,
            diags:  DiagSink(Arc::new(|_| {})),
            events: EventSink(Arc::new(|_| {})),
            result_store: shared_store.clone(),
            xref_seen,
        };
        let empty: futures_core::stream::BoxStream<'static, Arc<[Cursor]>> =
            futures_util::stream::iter(Vec::<Arc<[Cursor]>>::new()).boxed();
        let _: Vec<Arc<[Cursor]>> = block_on(pipeline_r.run(empty, ctx).collect());
        let _ = shared_store;
        Vec::<Box<dyn Diagnostic>>::new()
    };

    let res = run_scan_loop(store.clone(), initial_cfg, DEFAULT_DEPTH, run_pass);

    // Depth-4 chain: A -> (B,C) -> D -> E. Four passes + fixed point emits
    // passes=4 (the fixed-point pass still counts).
    assert_eq!(res.passes, 4, "expected 4 passes for depth-4 chain");
    assert!(!res.depth_exhausted, "loop should converge, not cap");

    // Final cfg carries every discovered repo.
    let mut repos: Vec<&str> = res.config.repos.iter().map(|r| r.as_ref()).collect();
    repos.sort();
    assert_eq!(repos, vec!["A", "B", "C", "D", "E"]);

    // Final-pass store: every cursor row binds (R, DEP). A consumer walking
    // this gets the full edge set of the dep graph.
    let rows = store.rows_of(&Arc::<str>::from("dep")).unwrap();
    let mut edges: Vec<(String, String)> = rows.iter()
        .filter_map(|row| {
            let r = row.get("R")?;
            let d = row.get("DEP")?;
            Some((r.value.to_string(), d.value.to_string()))
        })
        .collect();
    edges.sort();

    assert_eq!(edges, vec![
        ("A".into(), "B".into()),
        ("A".into(), "C".into()),
        ("B".into(), "C".into()),
        ("B".into(), "D".into()),
        ("C".into(), "D".into()),
        ("D".into(), "E".into()),
    ], "edge set must reconstruct the diamond DAG");

    // Diagonal dupe: C is claimed by both A and B; D by both B and C.
    let claimed_by: HashMap<&str, Vec<&str>> = {
        let mut m: HashMap<&str, Vec<&str>> = HashMap::new();
        for (r, d) in &edges {
            m.entry(d.as_str()).or_default().push(r.as_str());
        }
        m
    };
    assert_eq!(claimed_by["C"], vec!["A", "B"], "C must be dup-claimed");
    assert_eq!(claimed_by["D"], vec!["B", "C"], "D must be dup-claimed");

    // Every DEP in the final pass is Verified (all names made it into cfg).
    for row in rows {
        let d = row.get("DEP").unwrap();
        assert_eq!(d.verified, Tri::Verified,
            "DEP {:?} should be Verified in final pass", d.value);
        assert_eq!(d.scan_pointer.as_deref(), Some("repo"));
    }
}
