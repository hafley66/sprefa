//! Pipey-not-sinky: `expect_zero` and `expect_match` as uniform rows-in →
//! rows-out ops. Neither is positional; both compose anywhere in the pipe.
//!
//! `expect_zero(:c)`m``  — drops every input row, one Error diag per row.
//! `expect_match(:c)`m`` — pass-through; one Warn diag at run-end iff no
//!                         row ever arrived (Barrier `complete()` hook).

use std::sync::{Arc, Mutex};

use effect_runtime::v2::{
    expand, Diag, DiagSink, ExpandOpts, FactStore, MemFactStore, MemQueue, QueueBackend, Severity,
};

use v4::compile::parse::host_parse;
use v4::compile::walk::walk_program;
use v4::lower::{default_registry, LowerCtx};
use v4::Cursor;

struct CollectSink {
    rows: Mutex<Vec<Diag>>,
}
impl DiagSink for CollectSink {
    fn emit(&self, d: Diag) {
        self.rows.lock().unwrap().push(d);
    }
}

/// Compile + run `src` end-to-end. Returns (terminal rows enqueued past
/// the last component, diagnostics emitted). The terminal row count
/// matches what would land in a downstream sink, so tests can assert
/// "expect_zero left zero rows behind", "expect_match passed N through",
/// etc.
fn run(src: &str) -> (u64, Vec<Diag>) {
    let (program, parse_diags) = host_parse(src);
    assert!(parse_diags.is_empty(), "parse diags: {parse_diags:?}");
    assert_eq!(program.len(), 1, "expected one pipe");

    let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let reg = default_registry();
    let mut ctx = LowerCtx::new(store, std::env::temp_dir());

    let (pipes, walk_diags) = walk_program(&program, &reg, &mut ctx);
    assert!(
        walk_diags.is_empty(),
        "walk diags: {:?}",
        walk_diags
            .iter()
            .map(|d| (d.code.as_ref(), d.message.as_str()))
            .collect::<Vec<_>>()
    );
    assert_eq!(pipes.len(), 1);

    let sink = Arc::new(CollectSink {
        rows: Mutex::new(Vec::new()),
    });
    let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
    let inst = pipes.into_iter().next().unwrap().into_pipe().into_instance();
    let stats = expand(
        &inst,
        queue,
        vec![Arc::new(Cursor::default())],
        ExpandOpts::default().with_diag(sink.clone()),
    );
    let diags = sink.rows.lock().unwrap().clone();
    (stats.terminal, diags)
}

// ─── expect_zero ──────────────────────────────────────────────────────────

#[test]
fn expect_zero_silent_on_empty_input() {
    // re's pattern `xyzzy` matches nothing on the empty cursor → upstream
    // emits zero rows → expect_zero is silent.
    let src = "`anything` > re`xyzzy` > expect_zero(:no-eval)`eval is forbidden`;";
    let (_terminal, diags) = run(src);
    assert!(
        diags.is_empty(),
        "empty input must be silent, got {diags:?}",
    );
}

#[test]
fn expect_zero_errors_per_input_row() {
    // re emits two captures of `a` → expect_zero emits 2 Error diags.
    let src = "`abcabc` > re`(?P<X>a)` > expect_zero(:no-a)`a is forbidden ${X}`;";
    let (_terminal, diags) = run(src);
    let errs: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error && d.code.as_ref() == "no-a")
        .collect();
    assert_eq!(
        errs.len(),
        2,
        "two matches should produce two Error diags, got {diags:?}",
    );
}

#[test]
fn expect_zero_composes_mid_pipe() {
    // expect_zero drops every row → expect_match sees zero rows →
    // expect_match emits one Warn diag at run-end. The key test: no
    // "must be tail" error. expect_match can sit after expect_zero.
    let src = "`abcabc` > re`(?P<X>a)` \
               > expect_zero(:c)`m` \
               > expect_match(:d)`n`;";
    let (_terminal, diags) = run(src);

    let no_tail_err = diags.iter().any(|d| {
        d.severity == Severity::Error
            && d.code.as_ref().contains("expect-zero-must-be-tail")
    });
    assert!(
        !no_tail_err,
        "no positional 'must be tail' diag should exist: {diags:?}",
    );

    let warn_match = diags
        .iter()
        .find(|d| d.severity == Severity::Warn && d.code.as_ref() == "d");
    assert!(
        warn_match.is_some(),
        "expect_match should fire its warn (saw zero rows after expect_zero), got {diags:?}",
    );
}

// ─── expect_match ─────────────────────────────────────────────────────────

#[test]
fn expect_match_warns_when_zero_rows() {
    let src = "`abc` > re`xyzzy` > expect_match(:no-match)`no match here`;";
    let (_terminal, diags) = run(src);
    let warns: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Warn && d.code.as_ref() == "no-match")
        .collect();
    assert_eq!(
        warns.len(),
        1,
        "zero rows must produce one warn diag, got {diags:?}",
    );
}

#[test]
fn expect_match_silent_when_any_row() {
    let src = "`abc` > re`(?P<X>a)` > expect_match(:any)`m`;";
    let (_terminal, diags) = run(src);
    assert!(
        diags.is_empty(),
        "matching input must be silent, got {diags:?}",
    );
}

#[test]
fn expect_match_passes_rows_through() {
    // Terminal rows downstream of expect_match equal the rows that
    // arrived. Use re to emit 3 matches, then expect_match, then nothing.
    // expand's `stats.terminal` counts rows that ran past the last
    // component.
    let src = "`aaa` > re`(?P<X>a)` > expect_match(:m)`x`;";
    let (terminal_with, _diags_with) = run(src);

    let src_without = "`aaa` > re`(?P<X>a)`;";
    let (terminal_without, _diags_without) = run(src_without);

    assert_eq!(
        terminal_with, terminal_without,
        "expect_match must not change downstream row count",
    );
    assert!(terminal_with > 0, "expected nonzero rows for sanity");
}

// ─── meta: positional/name-list-free ──────────────────────────────────────

#[test]
fn no_positional_gate_in_engine() {
    // Grep the live source for the rejected positional machinery. None
    // of these tokens should be present anywhere in v4/src — `expect_zero`
    // and `expect_match` get their semantics from `OperatorDef`, never
    // from a name list or a "must be tail" walk-time guard.
    let banned = [
        "STREAM_OP_NAMES",
        "tail_is_expect_zero",
        "expect-zero-must-be-tail",
        "is_stream_op_name",
        "collect_stream_probes",
        "emit_zero_match_if_empty",
        "StreamOpProbe",
    ];

    let manifest = env!("CARGO_MANIFEST_DIR");
    let src_root = std::path::Path::new(manifest).join("src");

    let mut offenders: Vec<(std::path::PathBuf, &'static str)> = Vec::new();
    for entry in walkdir(&src_root) {
        if entry.extension().and_then(|s| s.to_str()) != Some("rs") {
            continue;
        }
        let body = match std::fs::read_to_string(&entry) {
            Ok(s) => s,
            Err(_) => continue,
        };
        for token in &banned {
            if body.contains(token) {
                offenders.push((entry.clone(), *token));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "positional gate machinery should not exist on main: {offenders:?}",
    );
}

fn walkdir(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in rd.flatten() {
            let p = entry.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.push(p);
            }
        }
    }
    out
}
