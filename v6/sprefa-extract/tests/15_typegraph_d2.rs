//! A board that does not compile is not a diagram. Every rule below is checked
//! by running the real `d2` binary; a missing `d2` fails loudly, never skips.

use std::path::{Path, PathBuf};
use std::process::Command;

/// At most this many shapes per board. Over budget SPLITS; it never crams.
const SHAPE_BUDGET: usize = 24;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "sprefa-typegraph-d2-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn run_example(root: &str, entry: &str, out: &Path) -> String {
    let output = Command::new(env!("CARGO"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args([
            "run",
            "--quiet",
            "--example",
            "typegraph_d2",
            "--",
            "--root",
            root,
            "--entry",
            entry,
            "--out",
            &out.to_string_lossy(),
        ])
        .output()
        .expect("cargo run");
    assert!(
        output.status.success(),
        "the example exited {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn boards(dir: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("out dir")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "d2"))
        .collect();
    found.sort();
    found
}

/// `direction` is a reserved keyword, not a shape; every other declaration line
/// in an emitted board is one shape, by construction (one line per shape).
fn shape_count(board: &Path) -> usize {
    std::fs::read_to_string(board)
        .expect("board")
        .lines()
        .filter(|line| !line.starts_with("direction:"))
        .filter(|line| {
            line.split_once(':').is_some_and(|(head, _)| {
                !head.is_empty()
                    && head
                        .chars()
                        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '.' || ch == '-')
            })
        })
        .count()
}

/// THE FOUR BOARD RULES, on real extractor output over this crate's own `src`.
///
/// 1. every board compiles under the real `d2`;
/// 2. the rendered viewBox is wider than it is tall;
/// 3. no board carries more than `SHAPE_BUDGET` shapes;
/// 4. the entrypoint appears on the first board.
///
/// NOT covered by this corpus: the over-budget chunk split. No hop band in
/// `src` reaches 24 nodes, so that branch is unexercised here rather than
/// asserted green.
#[test]
fn every_emitted_board_compiles_and_reads_wide() {
    let out = scratch("src");
    let stdout = run_example("src", "src/types.rs::ExtractOutput", &out);

    let files = boards(&out);
    assert!(
        !files.is_empty(),
        "the example wrote no board; stdout was: {stdout}"
    );

    let first = std::fs::read_to_string(&files[0]).expect("first board");
    assert!(
        first.contains(": ExtractOutput {"),
        "the entrypoint must be on the first board:\n{first}"
    );

    for board in &files {
        let svg = board.with_extension("svg");
        let render = Command::new("d2")
            .arg(board)
            .arg(&svg)
            .output()
            .unwrap_or_else(|err| {
                panic!("d2 is not on PATH and this gate never fakes green: {err}")
            });
        assert!(
            render.status.success(),
            "{} did not compile: {}",
            board.display(),
            String::from_utf8_lossy(&render.stderr)
        );

        let text = std::fs::read_to_string(&svg).expect("rendered svg");
        let view = text
            .split("viewBox=\"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .expect("a viewBox on the rendered svg");
        let dims: Vec<f64> = view
            .split_whitespace()
            .filter_map(|n| n.parse().ok())
            .collect();
        assert_eq!(dims.len(), 4, "viewBox was {view:?}");
        assert!(
            dims[2] > dims[3],
            "{} rendered {}x{}, taller than wide; the board must read wide",
            board.display(),
            dims[2],
            dims[3]
        );

        let shapes = shape_count(board);
        assert!(
            shapes <= SHAPE_BUDGET,
            "{} carries {shapes} shapes, over the budget of {SHAPE_BUDGET}; split it",
            board.display()
        );
    }
}

/// An entrypoint that names no type node is a nonzero exit with a message, not
/// an empty board.
#[test]
fn an_unknown_entrypoint_exits_nonzero() {
    let out = scratch("missing");
    let output = Command::new(env!("CARGO"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args([
            "run",
            "--quiet",
            "--example",
            "typegraph_d2",
            "--",
            "--root",
            "src",
            "--entry",
            "src/types.rs::NoSuchTypeAnywhere",
            "--out",
            &out.to_string_lossy(),
        ])
        .output()
        .expect("cargo run");
    assert!(!output.status.success(), "an unknown entrypoint must fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("NoSuchTypeAnywhere"),
        "the message must name what was not found: {stderr}"
    );
}
