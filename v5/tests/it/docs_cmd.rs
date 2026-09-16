//! `dl docs [topic]` — the embedded-guide reader (src/docs_cmd.rs). Drives the
//! built binary the same way the other subcommand tests do (env-provided path,
//! std::process::Command). Asserts the bare topic list, a reference page by a
//! stable heading, the book index + a chapter by both addressing schemes, and
//! the unknown-topic exit code.

use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn run(args: &[&str]) -> (i32, String, String) {
    let out = Command::new(DL).args(args).output().expect("run dl docs");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// Bare `dl docs` prints the topic list and exits 0.
#[test]
fn docs_lists_topics() {
    let (code, out, err) = run(&["docs"]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("REFERENCE"), "reference header missing: {out}");
    assert!(out.contains("syntax"), "syntax topic missing: {out}");
    assert!(out.contains("THE BOOK"), "book header missing: {out}");
    assert!(
        out.contains("dl docs book 3"),
        "addressing hint missing: {out}"
    );
}

/// A reference page prints to stdout with its stable top heading.
#[test]
fn docs_reference_page_prints_heading() {
    let (code, out, err) = run(&["docs", "syntax"]);
    assert_eq!(code, 0, "{err}");
    assert!(
        out.contains("# Syntax & semantics"),
        "syntax heading missing: {out}"
    );
}

/// `dl docs book` prints the book index.
#[test]
fn docs_book_index() {
    let (code, out, err) = run(&["docs", "book"]);
    assert_eq!(code, 0, "{err}");
    assert!(
        out.contains("# Reactive Facts"),
        "book index heading missing: {out}"
    );
}

/// A chapter is reachable by number (`book N`) and by slug — both print it.
#[test]
fn docs_chapter_by_number_and_slug() {
    let (code_num, out_num, err_num) = run(&["docs", "book", "3"]);
    assert_eq!(code_num, 0, "{err_num}");
    assert!(
        out_num.contains("# 3. The cycle problem"),
        "chapter-by-number missing: {out_num}"
    );

    let (code_slug, out_slug, err_slug) = run(&["docs", "03-the-cycle-problem"]);
    assert_eq!(code_slug, 0, "{err_slug}");
    assert_eq!(
        out_num, out_slug,
        "number and slug should print the same chapter"
    );
}

/// An unknown topic re-lists the topics and exits 1.
#[test]
fn docs_unknown_topic_exits_one() {
    let (code, out, err) = run(&["docs", "nonsense-topic"]);
    assert_eq!(code, 1, "expected exit 1 for unknown topic");
    assert!(
        err.contains("unknown topic"),
        "no unknown-topic message: {err}"
    );
    assert!(
        out.contains("REFERENCE"),
        "topic list not re-printed: {out}"
    );
}
