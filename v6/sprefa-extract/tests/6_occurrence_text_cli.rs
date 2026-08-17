//! The `--occurrence-text` contract on the `--scip-facts` door.
//!
//! The scip_occurrence passthrough row grows an optional `text` field: the
//! source slice at the occurrence's byte span, lossy-utf8. It is the v6 answer
//! to v5's scip_binding `local_name` (the local spelling of an alias or default
//! import), which the canonical-only symbol drops. Field and flag are OPTIONAL:
//! flag off, the stream is byte-identical to before the field existed.

use std::fs;
use std::process::Command;

const SCIP_REL_ROOT: &str = "tests/fixtures/scip_rel";
const SCIP_REL_SOURCE: &str = "tests/fixtures/scip_rel/animal.ts";

fn run(args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .args(args)
        .output()
        .expect("extract binary runs");
    assert!(
        output.status.success(),
        "{args:?} exited {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("stdout is UTF-8")
}

fn scip_rel_facts(extra: &[&str]) -> String {
    let mut args = vec!["--scip-facts"];
    args.extend_from_slice(extra);
    args.extend_from_slice(&[
        "--project-root",
        SCIP_REL_ROOT,
        "--scip-build",
        SCIP_REL_SOURCE,
    ]);
    run(&args)
}

/// With `--occurrence-text`, every scip_occurrence row's `text` is the corpus
/// bytes at that row's span. The assertion reads the fixture from disk and
/// slices it, so it checks the actual slice, not a hand-written constant.
#[test]
fn occurrence_text_slices_the_corpus_at_each_span() {
    let source = fs::read(SCIP_REL_SOURCE).expect("the fixture source");
    let facts = scip_rel_facts(&["--occurrence-text"]);

    let mut occurrences = 0;
    for line in facts.lines() {
        let value: serde_json::Value = serde_json::from_str(line).expect("a JSONL row");
        if value["record"] != "scip_occurrence" {
            continue;
        }
        occurrences += 1;
        let start = value["start"].as_u64().expect("start") as usize;
        let end = value["end"].as_u64().expect("end") as usize;
        let expected = String::from_utf8_lossy(&source[start..end]).into_owned();
        let actual = value["text"]
            .as_str()
            .expect("text is present with the flag");
        assert_eq!(
            actual, expected,
            "occurrence {start}..{end} must carry the source slice"
        );
    }
    assert!(
        occurrences > 0,
        "the fixture must emit scip_occurrence rows"
    );
}

/// Flag off, no scip_occurrence row carries a `text` key anywhere. The field
/// must be JSON-absent (not null, not empty), so a plain `--scip-facts` run
/// stays byte-identical to before the field existed.
#[test]
fn occurrence_text_is_absent_without_the_flag() {
    let facts = scip_rel_facts(&[]);
    for line in facts
        .lines()
        .filter(|line| line.contains("\"record\":\"scip_occurrence\""))
    {
        assert!(
            !line.contains("\"text\""),
            "flag off must not emit text on an occurrence row: {line}"
        );
    }
}
