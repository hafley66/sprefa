// Re (regex) + Sh (shell) ops. Cover query / filter / side-effect.

use std::sync::Arc;
use std::path::PathBuf;
use std::io::Write;
use futures::stream::StreamExt;
use tokio::sync::mpsc;
use v4::*;

fn hooks() -> Hooks {
    let (eff_tx, _eff_rx) = mpsc::unbounded_channel();
    Hooks {
        store: MemStore::new(),
        effects: eff_tx,
        gen: 0,
        lineage: new_lineage(),
        tele: Telemetry::new(),
        interner: Interner::new(),
    }
}

static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
fn tempdir() -> PathBuf {
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let p = std::env::temp_dir().join(format!("v4_resh_{}_{}_{}", pid, nanos, seq));
    std::fs::create_dir_all(&p).unwrap();
    p
}

async fn drain(chain: Vec<Arc<dyn Op>>, h: Hooks, seed: Vec<Vec<Cursor>>) -> Vec<Cursor> {
    let input = futures::stream::iter(seed).boxed();
    let mut s = chain.into_iter().fold(input, |acc, op| op.run(h.clone(), acc));
    let mut out = Vec::new();
    while let Some(b) = s.next().await { out.extend(b); }
    out
}

#[tokio::test]
async fn re_matches_against_fs_file_with_captures() {
    let dir = tempdir();
    let p = dir.join("a.rs");
    {
        let mut f = std::fs::File::create(&p).unwrap();
        writeln!(f, "let x = 42; let y = 99; let z = 1234;").unwrap();
    }

    let chain: Vec<Arc<dyn Op>> = vec![
        Arc::new(Fs::new(dir.clone(), vec!["rs".into()])),
        Arc::new(Re::new(r"let (\w+) = (\d+);", &["NAME", "VAL"])),
    ];
    let out = drain(chain, hooks(), vec![]).await;

    assert_eq!(out.len(), 3, "three let-bindings");
    let names: Vec<_> = out.iter().map(|c| c.get("NAME").unwrap().to_string()).collect();
    assert_eq!(names, vec!["x", "y", "z"]);
    let vals: Vec<_> = out.iter().map(|c| c.get("VAL").unwrap().to_string()).collect();
    assert_eq!(vals, vec!["42", "99", "1234"]);
    // Every cursor carries CONTENT_HASH (file source) and LO/HI byte ranges.
    for c in &out {
        assert!(c.get("CONTENT_HASH").is_some());
        assert!(c.get("LO").is_some());
        assert!(c.get("HI").is_some());
        assert!(c.get("MATCH").is_some());
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn re_matches_against_term() {
    // on_term mode: match against an inline cursor term, not the file.
    let mut c = Cursor::default();
    c.set("LINE", "user-42 wrote ticket-7");
    let seed = vec![vec![c]];

    let chain: Vec<Arc<dyn Op>> = vec![
        Arc::new(Re::new(r"(\w+)-(\d+)", &["KIND", "NUM"]).on_term("LINE")),
    ];
    let out = drain(chain, hooks(), seed).await;

    assert_eq!(out.len(), 2, "two matches: user_42 and ticket-7");
    assert_eq!(out[0].get("KIND"), Some("user"));
    assert_eq!(out[0].get("NUM"),  Some("42"));
    assert_eq!(out[1].get("KIND"), Some("ticket"));
    assert_eq!(out[1].get("NUM"),  Some("7"));
    // CONTENT_HASH only stamped when reading FS.
    assert!(out[0].get("CONTENT_HASH").is_none());
}

#[tokio::test]
async fn sh_emit_lines_per_stdout_line() {
    let mut c = Cursor::default();
    c.set("BASE", "alpha beta gamma");
    let seed = vec![vec![c]];

    // tr ' ' '\n' on the BASE term → 3 lines.
    let chain: Vec<Arc<dyn Op>> = vec![
        Arc::new(Sh::emit_lines("printf '%s' ${BASE} | tr ' ' '\\n'")),
    ];
    let out = drain(chain, hooks(), seed).await;

    assert_eq!(out.len(), 3);
    let lines: Vec<_> = out.iter().map(|c| c.get("LINE").unwrap().to_string()).collect();
    assert_eq!(lines, vec!["alpha", "beta", "gamma"]);
    // Carry-forward: input term BASE preserved on every emitted cursor.
    for c in &out { assert_eq!(c.get("BASE"), Some("alpha beta gamma")); }
}

#[tokio::test]
async fn sh_filter_by_exit_keeps_zero_drops_nonzero() {
    let mut c1 = Cursor::default(); c1.set("X", "5");
    let mut c2 = Cursor::default(); c2.set("X", "0");
    let mut c3 = Cursor::default(); c3.set("X", "9");
    let seed = vec![vec![c1, c2, c3]];

    // [ "${X}" -gt 0 ] → keeps non-zero values.
    let chain: Vec<Arc<dyn Op>> = vec![
        Arc::new(Sh::filter("[ ${X} -gt 0 ]")),
    ];
    let out = drain(chain, hooks(), seed).await;

    assert_eq!(out.len(), 2);
    let xs: Vec<_> = out.iter().map(|c| c.get("X").unwrap().to_string()).collect();
    assert_eq!(xs, vec!["5", "9"]);
}

#[tokio::test]
async fn sh_capture_stdout_binds_to_term() {
    let mut c = Cursor::default(); c.set("WHO", "world");
    let seed = vec![vec![c]];

    let chain: Vec<Arc<dyn Op>> = vec![
        Arc::new(Sh::capture("printf 'hello %s' ${WHO}", "GREETING")),
    ];
    let out = drain(chain, hooks(), seed).await;

    assert_eq!(out.len(), 1);
    assert_eq!(out[0].get("GREETING"), Some("hello world"));
    assert_eq!(out[0].get("WHO"), Some("world"));
}

#[tokio::test]
async fn sh_bang_runs_side_effect_and_passes_through() {
    // Touch a file as a side-effect; cursor passes through.
    let dir = tempdir();
    let touch = dir.join("touched");
    let mut c = Cursor::default(); c.set("PATH", touch.display().to_string().as_str());
    let seed = vec![vec![c]];

    let chain: Vec<Arc<dyn Op>> = vec![
        Arc::new(ShBang::new("touch ${PATH}")),
    ];
    let out = drain(chain, hooks(), seed).await;

    assert_eq!(out.len(), 1);
    assert!(touch.exists(), "side-effect should have created the file");
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn sh_pure_sh_bang_impure_purity_split() {
    let s_emit:    Arc<dyn Op> = Arc::new(Sh::emit_lines("echo hi"));
    let s_filter:  Arc<dyn Op> = Arc::new(Sh::filter("true"));
    let s_capture: Arc<dyn Op> = Arc::new(Sh::capture("echo hi", "OUT"));
    let s_bang:    Arc<dyn Op> = Arc::new(ShBang::new("touch /tmp/whatever"));
    assert!(s_emit.is_pure());
    assert!(s_filter.is_pure());
    assert!(s_capture.is_pure());
    assert!(!s_bang.is_pure());
}

#[tokio::test]
async fn sh_quotes_against_injection() {
    // A term containing shell metacharacters must not break out of the
    // quoted argument.
    let mut c = Cursor::default();
    c.set("EVIL", "; echo PWNED; #");
    let seed = vec![vec![c]];

    let chain: Vec<Arc<dyn Op>> = vec![
        Arc::new(Sh::capture("printf '%s' ${EVIL}", "OUT")),
    ];
    let out = drain(chain, hooks(), seed).await;
    assert_eq!(out.len(), 1);
    // The literal payload should round-trip, not get evaluated.
    assert_eq!(out[0].get("OUT"), Some("; echo PWNED; #"));
}

#[tokio::test]
async fn re_then_sh_compose_query_filter() {
    // Build a pipe: file → regex extract numbers → sh filter (>10) → count.
    let dir = tempdir();
    let p = dir.join("nums.txt");
    {
        let mut f = std::fs::File::create(&p).unwrap();
        writeln!(f, "1 5 11 99 3 22 7 100").unwrap();
    }

    let chain: Vec<Arc<dyn Op>> = vec![
        Arc::new(Fs::new(dir.clone(), vec!["txt".into()])),
        Arc::new(Re::new(r"(\d+)", &["N"])),
        Arc::new(Sh::filter("[ ${N} -gt 10 ]")),
    ];
    let out = drain(chain, hooks(), vec![]).await;
    let kept: Vec<_> = out.iter().map(|c| c.get("N").unwrap().to_string()).collect();
    assert_eq!(kept, vec!["11", "99", "22", "100"]);
    let _ = std::fs::remove_dir_all(&dir);
}
