//! Tier-1 snapshots: parse the TS fixture, project each family, flatten, diff
//! against the committed `.snap`. The `.snap` is the deterministic (sorted)
//! JSONL; the sort makes the snapshot immune to ast-grep/tree-sitter/oxc
//! traversal-order shifts.

use sprefa_extract::{
    dispatch_cst, dispatch_type, flatten_cst_jsonl, flatten_type_jsonl, AstGrepParser,
    CstProjector, OxcParser, TypeProjector,
};

const CST_SNAP: &str = "tests/fixtures/ts/sample.cstf.snap";
const TYPE_SNAP: &str = "tests/fixtures/ts/sample.typef.snap";

#[test]
fn ts_cstf_snapshot() {
    let content = include_bytes!("fixtures/ts/sample.ts");
    let (bundle, strings) =
        dispatch_cst("sample.ts", content, &AstGrepParser, &CstProjector).expect("parse");
    let actual = flatten_cst_jsonl(&bundle, &strings).join("\n");

    // Regenerate the committed snapshot: `UPDATE_SNAP=1 cargo test`.
    if std::env::var("UPDATE_SNAP").is_ok() {
        std::fs::write(CST_SNAP, format!("{actual}\n")).expect("write snap");
        eprintln!("updated {CST_SNAP}");
        return;
    }

    let expected = include_str!("fixtures/ts/sample.cstf.snap");
    assert_eq!(
        actual,
        expected.trim_end(),
        "CstF snapshot drifted. Regenerate with UPDATE_SNAP=1 cargo test, or overwrite \
         {CST_SNAP} with:\n----\n{actual}\n----"
    );
}

#[test]
fn ts_typef_snapshot() {
    let content = include_bytes!("fixtures/ts/sample.ts");
    let (bundle, strings) =
        dispatch_type("sample.ts", content, &OxcParser, &TypeProjector).expect("parse");
    let actual = flatten_type_jsonl(&bundle, &strings).join("\n");

    if std::env::var("UPDATE_SNAP").is_ok() {
        std::fs::write(TYPE_SNAP, format!("{actual}\n")).expect("write snap");
        eprintln!("updated {TYPE_SNAP}");
        return;
    }

    let expected = include_str!("fixtures/ts/sample.typef.snap");
    assert_eq!(
        actual,
        expected.trim_end(),
        "TypeF snapshot drifted. Regenerate with UPDATE_SNAP=1 cargo test, or overwrite \
         {TYPE_SNAP} with:\n----\n{actual}\n----"
    );
}
