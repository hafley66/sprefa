//! Kitchen-sink for $rule + $repo + $rev. No fs, no file reads.

use std::path::PathBuf;
use std::sync::Arc;

use futures::executor::block_on;
use futures_util::stream::StreamExt;

use v2::{
    Config, MemReader, MemWriter,
    OpCtx, OperatorRegistry, ProgramCtx, host_parse, lower_rules,
};
use v2::ops::{FsFactory, RepoFactory, RevFactory, RuleFactory};
use v2::_0_types::Cursor;

fn make_config() -> Arc<Config> {
    Arc::new(Config::test_default())
}

fn make_registry() -> Arc<OperatorRegistry> {
    let mut r = OperatorRegistry::new();
    r.register(Arc::new(RuleFactory));
    r.register(Arc::new(RepoFactory));
    r.register(Arc::new(RevFactory));
    r.register(Arc::new(FsFactory));
    Arc::new(r)
}

#[test]
fn rule_repo_rev_end_to_end() {
    let cfg = make_config();

    let reader = Arc::new(
        MemReader::new(cfg.clone())
            .with_repo("myorg/alpha", &["main", "next"])
            .with_repo("myorg/beta",  &["main"])
            .with_repo("other/gamma", &["main"])
    );
    let writer = Arc::new(MemWriter::new());

    let src = r#"
        # `;` distributes the parent — both pipes run against the rule's seed,
        # results unioned into the demo table. rev($V) bind was banned as
        # unbounded-capture; fork over concrete revs instead.
        rule(demo) {
            > repo(myorg/*) { > rev(main); > rev(next); };
            > repo(other/*) > rev(main);
        };
    "#;

    let invs = host_parse(src, Arc::from(PathBuf::from("<test>").as_path())).unwrap();
    let pctx = ProgramCtx::new(cfg.clone(), make_registry());
    let outcome = lower_rules(invs, pctx);
    assert!(outcome.diags.is_empty(), "lower diags: {:?}", outcome.diags.iter().map(|d| d.code()).collect::<Vec<_>>());
    assert_eq!(outcome.pipelines.len(), 1);
    let ctx = OpCtx::for_test(cfg.clone(), reader.clone(), writer.clone());

    // Drive the top pipeline with an empty seed; rule is a source and ignores input.
    let empty: futures_core::stream::BoxStream<'static, Arc<[Cursor]>> =
        futures_util::stream::iter(Vec::<Arc<[Cursor]>>::new()).boxed();
    let out_stream = outcome.pipelines[0].run(empty, ctx);
    let batches: Vec<Arc<[Cursor]>> = block_on(out_stream.collect());
    let out: Vec<Cursor> = batches.into_iter()
        .flat_map(|b| b.iter().cloned().collect::<Vec<_>>())
        .collect();

    let repos: Vec<&str> = out.iter().map(|c| c.repo.as_ref()).collect();
    let revs:  Vec<&str> = out.iter().map(|c| c.rev.as_ref()).collect();

    // Fork: pipe1 → 3 myorg cursors, pipe2 → 1 other cursor, unioned = 4.
    assert_eq!(out.len(), 4, "repos={repos:?} revs={revs:?}");
    assert_eq!(repos.iter().filter(|r| r.starts_with("myorg/")).count(), 3);
    assert_eq!(repos.iter().filter(|r| r.starts_with("other/")).count(), 1);

    let rows = writer.snapshot_rows();
    let demo = rows.get::<Arc<str>>(&Arc::from("__demo")).expect("demo table");
    assert_eq!(demo.len(), 4);

    println!("rows: {}", demo.len());
    for r in demo {
        println!("  {} @ {}", r.repo, r.rev);
    }

    // Path-tagging sanity: every cursor should have at least
    // [Op(rule), ForkArm(0|1), Op(repo), Op(rev)].
    for c in &out {
        let segs: Vec<String> = c.path.0.iter().map(|s| match s {
            v2::PathSeg::Op       { name, step, .. } => format!("Op({name},step={step})"),
            v2::PathSeg::ForkArm  { index, .. }       => format!("ForkArm({index})"),
            v2::PathSeg::Named    { name, key, .. }   => format!("Named({name},{key})"),
            v2::PathSeg::SwitchArm{ pat, .. }         => format!("SwitchArm({pat})"),
            v2::PathSeg::LeafArm  { key, .. }         => format!("LeafArm({key})"),
            v2::PathSeg::Iter     { index }           => format!("Iter({index})"),
        }).collect();
        println!("  path: {}", segs.join(" / "));
    }

    // Evidence sanity: each cursor passed through repo + rev (both report).
    // rule op does not witness, so exactly two entries per cursor here.
    for c in &out {
        let ops: Vec<&str> = c.evidence.iter().map(|e| e.op_name).collect();
        assert_eq!(ops, vec!["repo", "rev"], "evidence ops for cursor {}", c.repo);
        // Both filter-mode now — rev bind was banned.
        assert_eq!(c.evidence[0].capture, None);
        assert_eq!(c.evidence[1].capture, None);
        assert_eq!(c.evidence[0].matched.as_ref(), c.repo.as_ref());
        assert_eq!(c.evidence[1].matched.as_ref(), c.rev.as_ref());
    }
}

#[test]
fn fs_fanout_and_evidence() {
    let cfg = make_config();

    // Different file lists per (repo, rev) — proves fs honors rev channel.
    let reader = Arc::new(
        MemReader::new(cfg.clone())
            .with_repo("myorg/alpha", &["main", "v1"])
            .with_repo("myorg/beta",  &["main"])
            .with_files("myorg/alpha", "main", &["src/a.rs", "src/b.rs"])
            .with_files("myorg/alpha", "v1",   &["lib/legacy.rs"])
            .with_files("myorg/beta",  "main", &["src/c.rs"])
    );
    let writer = Arc::new(MemWriter::new());

    let src = r#"
        # rev($V) is banned; fork over concrete revs (main, v1) instead.
        # Rule body is a fork: each arm is a full pipe ending in fs(**).
        rule(files_demo) {
            > repo(myorg/*) {
                > rev(main) > fs(**);
                > rev(v1)   > fs(**);
            }
        };
        rule(bind_demo) {
            > repo(myorg/beta) > rev(main) > fs($F)
        };
    "#;

    let invs = host_parse(src, Arc::from(PathBuf::from("<test>").as_path())).unwrap();
    let pctx = ProgramCtx::new(cfg.clone(), make_registry());
    let outcome = lower_rules(invs, pctx);
    assert!(outcome.diags.is_empty(),
        "lower diags: {:?}", outcome.diags.iter().map(|d| d.code()).collect::<Vec<_>>());
    assert_eq!(outcome.pipelines.len(), 2);
    let ctx = OpCtx::for_test(cfg.clone(), reader.clone(), writer.clone());

    // Drive files_demo.
    let empty: futures_core::stream::BoxStream<'static, Arc<[Cursor]>> =
        futures_util::stream::iter(Vec::<Arc<[Cursor]>>::new()).boxed();
    let batches: Vec<Arc<[Cursor]>> =
        block_on(outcome.pipelines[0].run(empty, ctx.clone()).collect());
    let out: Vec<Cursor> = batches.into_iter()
        .flat_map(|b| b.iter().cloned().collect::<Vec<_>>())
        .collect();

    // (alpha,main)×2 + (alpha,v1)×1 + (beta,main)×1 = 4.
    assert_eq!(out.len(), 4);

    // Rev channel honored: alpha v1 yields lib/legacy.rs, never src/a.rs.
    let v1_cursors: Vec<_> = out.iter().filter(|c| c.rev.as_ref() == "v1").collect();
    assert_eq!(v1_cursors.len(), 1);
    assert_eq!(v1_cursors[0].fs.as_ref().unwrap().0.to_string_lossy(), "lib/legacy.rs");

    // Evidence chain on each cursor: repo, rev, fs — three ops, all witnessing.
    for c in &out {
        let ops: Vec<&str> = c.evidence.iter().map(|e| e.op_name).collect();
        assert_eq!(ops, vec!["repo", "rev", "fs"],
            "evidence chain for {}/{}/{:?}", c.repo, c.rev, c.fs);
        // Throwaway `**` glob still records the concrete matched path.
        let fs_ev = &c.evidence[2];
        assert_eq!(fs_ev.capture, None);
        assert_eq!(fs_ev.matched.as_ref(), c.fs.as_ref().unwrap().0.to_string_lossy());
        // Concrete rev filter — capture is None.
        assert_eq!(c.evidence[1].capture, None);
    }

    // Drive bind_demo — fs($F) captures filename.
    let empty: futures_core::stream::BoxStream<'static, Arc<[Cursor]>> =
        futures_util::stream::iter(Vec::<Arc<[Cursor]>>::new()).boxed();
    let batches2: Vec<Arc<[Cursor]>> =
        block_on(outcome.pipelines[1].run(empty, ctx).collect());
    let out2: Vec<Cursor> = batches2.into_iter()
        .flat_map(|b| b.iter().cloned().collect::<Vec<_>>())
        .collect();
    assert_eq!(out2.len(), 1);
    let c = &out2[0];
    assert_eq!(c.captures.get("F").map(|cap| cap.value.as_ref()), Some("src/c.rs"));
    let fs_ev = c.evidence.iter().find(|e| e.op_name == "fs").unwrap();
    assert_eq!(fs_ev.capture.as_ref().map(|a| a.as_ref()), Some("F"));
    assert_eq!(fs_ev.matched.as_ref(), "src/c.rs");
}
