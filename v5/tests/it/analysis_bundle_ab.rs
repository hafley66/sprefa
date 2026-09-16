//! Opt-in production-corpus A/B probe for bundled analysis extraction.
//! Run directly under `/usr/bin/time -l`; ignored in the normal suite.

use sprefa_v5::{db, engine::Engine, lex, parse};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

const PROG: &str = r#"
rel seen(path: file).
seen(p) <- scan("WORK", "src/**/*.rs", p, rev), match(p, rev, /fn/, line).
? type_entity(repo, sym, name, kind, parent, file, line).
? call_def(repo, sym, kind, file, line, end).
? df_node(id, kind, var, fn, file, line, _).
"#;

fn relation_fingerprint(eng: &Engine) -> (usize, String) {
    let mut rows = Vec::new();
    for table in ["rel_type_entity_txt", "rel_call_def_txt", "rel_df_node_txt"] {
        for row in eng
            .query_sql(&format!("SELECT * FROM {table}"), &[])
            .unwrap()
        {
            rows.push(format!("{table}:{}", serde_json::to_string(&row).unwrap()));
        }
    }
    rows.sort_unstable();
    let digest = blake3::hash(rows.join("\n").as_bytes())
        .to_hex()
        .to_string();
    (rows.len(), digest)
}

#[test]
#[ignore = "production-corpus performance probe; requires DL_AB_ROOT and DL_AB_DB"]
fn production_corpus_full_warm_incremental() {
    let stop_rss = Arc::new(AtomicBool::new(false));
    let max_rss_kb = Arc::new(AtomicUsize::new(0));
    let monitor = {
        let stop = Arc::clone(&stop_rss);
        let max = Arc::clone(&max_rss_kb);
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                if let Ok(out) = Command::new("ps")
                    .args(["-o", "rss=", "-p", &std::process::id().to_string()])
                    .output()
                {
                    if let Ok(kb) = String::from_utf8_lossy(&out.stdout).trim().parse::<usize>() {
                        max.fetch_max(kb, Ordering::Relaxed);
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        })
    };
    let root = PathBuf::from(std::env::var("DL_AB_ROOT").expect("DL_AB_ROOT"));
    let db_path = std::env::var("DL_AB_DB").expect("DL_AB_DB");
    let separate = std::env::var("DL_AB_MODE").ok().as_deref() == Some("separate");
    let prog = parse::parse(lex::lex(PROG).unwrap()).unwrap();
    let conn = db::open(Some(&db_path)).unwrap();
    let mut eng = Engine::new(conn, root.clone());
    eng.force_separate_analysis_extractors.set(separate);

    let t = Instant::now();
    eng.tick(&prog, true).unwrap();
    let cold_ms = t.elapsed().as_secs_f64() * 1000.0;
    let cold_parses = eng.extract_files_parsed.get();
    let cold_fingerprint = relation_fingerprint(&eng);

    let t = Instant::now();
    eng.tick(&prog, true).unwrap();
    let warm_ms = t.elapsed().as_secs_f64() * 1000.0;
    let warm_parses = eng.extract_files_parsed.get() - cold_parses;

    let changed = root.join("src/analysis_bundle_ab_target.rs");
    fs::write(
        &changed,
        "pub fn analysis_bundle_ab_target(x: i64) -> i64 { let y = x + 1; y }\n",
    )
    .unwrap();
    let before_incremental = eng.extract_files_parsed.get();
    let t = Instant::now();
    eng.tick_paths(&prog, std::slice::from_ref(&changed), true)
        .unwrap();
    let incremental_ms = t.elapsed().as_secs_f64() * 1000.0;
    let incremental_parses = eng.extract_files_parsed.get() - before_incremental;
    let incremental_fingerprint = relation_fingerprint(&eng);
    fs::remove_file(&changed).unwrap();
    stop_rss.store(true, Ordering::Relaxed);
    monitor.join().unwrap();

    println!(
        "AB_RESULT {}",
        serde_json::json!({
            "mode": if separate { "separate" } else { "bundled" },
            "cold_ms": cold_ms,
            "cold_parses": cold_parses,
            "warm_ms": warm_ms,
            "warm_parses": warm_parses,
            "incremental_ms": incremental_ms,
            "incremental_parses": incremental_parses,
            "cold_rows": cold_fingerprint.0,
            "cold_digest": cold_fingerprint.1,
            "incremental_rows": incremental_fingerprint.0,
            "incremental_digest": incremental_fingerprint.1,
            "max_rss_kb": max_rss_kb.load(Ordering::Relaxed),
        })
    );
}
