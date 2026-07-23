//! Tier-1 snapshot: parse the TS fixture, project CstF, flatten, diff against
//! the committed `.snap`. The `.snap` is the deterministic (sorted) JSONL; the
//! sort makes the snapshot immune to ast-grep/tree-sitter traversal-order shifts.

use sprefa_extract::{dispatch_cst, flatten_cst_jsonl, AstGrepParser, CstProjector};

const SNAP_PATH: &str = "tests/fixtures/ts/sample.cstf.snap";

#[test]
fn ts_cstf_snapshot() {
    let content = include_bytes!("fixtures/ts/sample.ts");
    let (bundle, strings) =
        dispatch_cst("sample.ts", content, &AstGrepParser, &CstProjector).expect("parse");
    let actual = flatten_cst_jsonl(&bundle, &strings).join("\n");

    // Regenerate the committed snapshot: `UPDATE_SNAP=1 cargo test`.
    if std::env::var("UPDATE_SNAP").is_ok() {
        std::fs::write(SNAP_PATH, format!("{actual}\n")).expect("write snap");
        eprintln!("updated {SNAP_PATH}");
        return;
    }

    let expected = include_str!("fixtures/ts/sample.cstf.snap");
    assert_eq!(
        actual,
        expected.trim_end(),
        "CstF snapshot drifted. Regenerate with UPDATE_SNAP=1 cargo test, or overwrite \
         tests/fixtures/ts/sample.cstf.snap with:\n----\n{actual}\n----"
    );
}
