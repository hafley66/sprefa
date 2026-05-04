// v4-proto demo bin. argv → Action → dispatch. Drives the Op pipelines
// from the v4 library lib.rs against `.rs` files.

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use ast_grep_language::SupportLang;
use tokio::sync::mpsc;

use v4::*;

async fn run_indexer(h: &Hooks, root: PathBuf) {
    let pipelines: Vec<Vec<Arc<dyn Op>>> = vec![
        vec![
            Arc::new(Fs::new(root.clone(), vec!["rs".into()])),
            Arc::new(AstNm::new("fn $NAME",  SupportLang::Rust, &["NAME"])),
            Arc::new(Fact { name: "fns".into() }),
        ],
        vec![
            Arc::new(Fs::new(root.clone(), vec!["rs".into()])),
            Arc::new(AstNm::new("use $PATH", SupportLang::Rust, &["PATH"])),
            Arc::new(Fact { name: "uses".into() }),
        ],
    ];
    let tasks: Vec<_> = pipelines.into_iter().map(|chain| {
        let h = h.clone();
        tokio::spawn(async move { drive(chain, h).await })
    }).collect();
    for t in tasks { let _ = t.await; }
}

async fn index_one(h: &Hooks, path: PathBuf) {
    let chains: Vec<Vec<Arc<dyn Op>>> = vec![
        vec![
            Arc::new(SinglePath { path: path.clone() }),
            Arc::new(AstNm::new("fn $NAME",  SupportLang::Rust, &["NAME"])),
            Arc::new(Fact { name: "fns".into() }),
        ],
        vec![
            Arc::new(SinglePath { path: path.clone() }),
            Arc::new(AstNm::new("use $PATH", SupportLang::Rust, &["PATH"])),
            Arc::new(Fact { name: "uses".into() }),
        ],
    ];
    for chain in chains { drive(chain, h.clone()).await; }
}

async fn dispatch(store: &Arc<dyn Store>, eff_tx: &mpsc::UnboundedSender<Effect>, action: Action) {
    let hooks = Hooks {
        store:    store.clone(),
        effects:  eff_tx.clone(),
        gen:      action.gen,
        lineage:  new_lineage(),
        tele:     Telemetry::new(),
        interner: Interner::new(),
    };
    match action.kind {
        ActionKind::Run { root } => {
            eprintln!("▶ Run @ gen {} root={}", action.gen, root.display());
            run_indexer(&hooks, root).await;
            store.commit(action.gen).await;
        }
        ActionKind::FileChanged { path } => {
            eprintln!("▶ FileChanged @ gen {} path={}", action.gen, path.display());
            let p = path.display().to_string();
            store.forget_by("fns",  "FS", &p, action.gen).await;
            store.forget_by("uses", "FS", &p, action.gen).await;
            index_one(&hooks, path).await;
            store.commit(action.gen).await;
        }
        ActionKind::Quit => { eprintln!("▶ Quit @ gen {}", action.gen); }
    }
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root: PathBuf = args.first().cloned().map(PathBuf::from).unwrap_or_else(|| ".".into());
    let changed: Option<PathBuf> = args.iter().position(|a| a == "--changed")
        .and_then(|i| args.get(i+1).cloned()).map(PathBuf::from);

    let store: Arc<dyn Store> = MemStore::new();

    store.define_rule("unused_fns", RuleBody::Antijoin {
        left: "fns".into(), right: "uses".into(), key: "NAME".into(),
    });
    store.define_rule("imports_used_2plus", RuleBody::GroupCount {
        src: "uses".into(), key: "PATH".into(), min: 2, count_term: "COUNT".into(),
    });

    let (eff_tx, mut eff_rx) = mpsc::unbounded_channel();
    let saga = tokio::spawn(async move {
        while let Some(e) = eff_rx.recv().await {
            match e { Effect::Print(s) => println!("{}", s) }
        }
    });

    {
        let h = Hooks {
            store: store.clone(), effects: eff_tx.clone(), gen: 0, lineage: 0,
            tele: Telemetry::new(), interner: Interner::new(),
        };
        let chain: Vec<Arc<dyn Op>> = vec![
            Arc::new(Select { name: "unused_fns".into() }),
            Arc::new(Print  { template: "  unused_fn  {SIGN} gen={GEN} NAME={NAME} FS={FS}".into() }),
        ];
        tokio::spawn(async move { drive(chain, h).await });
    }
    {
        let h = Hooks {
            store: store.clone(), effects: eff_tx.clone(), gen: 0, lineage: 0,
            tele: Telemetry::new(), interner: Interner::new(),
        };
        let chain: Vec<Arc<dyn Op>> = vec![
            Arc::new(Select { name: "imports_used_2plus".into() }),
            Arc::new(Print  { template: "  hot_use   {SIGN} gen={GEN} PATH={PATH} COUNT={COUNT}".into() }),
        ];
        tokio::spawn(async move { drive(chain, h).await });
    }

    let t0 = std::time::Instant::now();
    dispatch(&store, &eff_tx, new_action(ActionKind::Run { root }, None)).await;
    eprintln!("⏱  indexer round done in {:?}", t0.elapsed());

    if let Some(p) = changed {
        let parent_g = GEN.load(Ordering::SeqCst);
        let act = new_action(ActionKind::FileChanged { path: p }, Some((parent_g, 0)));
        dispatch(&store, &eff_tx, act).await;
    }

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let fns = store.snapshot("fns").await;
    let uses = store.snapshot("uses").await;
    let unused = store.snapshot("unused_fns").await;
    let hot = store.snapshot("imports_used_2plus").await;
    eprintln!("── snapshot: fns={} uses={} unused_fns={} hot_imports={}",
              fns.len(), uses.len(), unused.len(), hot.len());

    drop(eff_tx);
    let _ = tokio::time::timeout(std::time::Duration::from_millis(20), saga).await;
    Ok(())
}
