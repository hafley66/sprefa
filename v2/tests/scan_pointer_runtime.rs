//! Content-side stamping — walker ops stamp `scan_pointer` + `Tri::Claimed`
//! on Captures whose var is named in a `$$sigil($VAR)` annotation.
//!
//! This is the scout half. A separate checker pass downgrades unverified
//! claims. Non-annotated captures stay `scan_pointer: None`, default Tri.

use std::path::PathBuf;
use std::sync::Arc;

use futures::executor::block_on;
use futures_util::stream::StreamExt;

use v2::_0_types::{Cursor, Tri};
use v2::ops::{FsFactory, JsonFactory, ReadFactory, RepoFactory, RevFactory, RuleFactory};
use v2::{
    host_parse, lower_rules, Config, DiagSink, EventSink, MemReader, MemWriter, OpCtx, OpId,
    OperatorRegistry, ProgramCtx, RunId, RuntimeConfig,
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
    let mut r = OperatorRegistry::new();
    r.register(Arc::new(RuleFactory));
    r.register(Arc::new(RepoFactory));
    r.register(Arc::new(RevFactory));
    r.register(Arc::new(FsFactory));
    r.register(Arc::new(ReadFactory));
    r.register(Arc::new(JsonFactory));
    Arc::new(r)
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
    let empty: futures_core::stream::BoxStream<'static, Cursor> =
        futures_util::stream::iter(Vec::<Cursor>::new()).boxed();
    block_on(outcome.pipelines[0].run(empty, ctx).collect())
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
