//! Tier-1 snapshots via the uniform surface. ONE loop-driven test dispatches each
//! family under a single-family mask, flattens, sorts, diffs its committed `.snap`.
//! The sort makes the snapshot immune to traversal-order shifts. A roster test
//! pins the first-match routing. The 4 `.snap` files stay unchanged: the new path
//! reproduces the per-family output with zero regression (Epic U's golden).

use sprefa_extract::{dispatch, flatten_jsonl, source_for, FamilyMask, FamilyTag};

const FIXTURE: &[u8] = include_bytes!("fixtures/ts/sample.ts");

struct Case {
    tag: FamilyTag,
    mask: FamilyMask,
    snap: &'static str,
}

const CASES: &[Case] = &[
    Case {
        tag: FamilyTag::Cst,
        mask: FamilyMask {
            cst: true,
            types: false,
            call: false,
            df: false,
        },
        snap: "tests/fixtures/ts/sample.cstf.snap",
    },
    Case {
        tag: FamilyTag::Type,
        mask: FamilyMask {
            cst: false,
            types: true,
            call: false,
            df: false,
        },
        snap: "tests/fixtures/ts/sample.typef.snap",
    },
    Case {
        tag: FamilyTag::Call,
        mask: FamilyMask {
            cst: false,
            types: false,
            call: true,
            df: false,
        },
        snap: "tests/fixtures/ts/sample.callf.snap",
    },
    Case {
        tag: FamilyTag::Df,
        mask: FamilyMask {
            cst: false,
            types: false,
            call: false,
            df: true,
        },
        snap: "tests/fixtures/ts/sample.dff.snap",
    },
];

/// Epic U's golden: one `dispatch` + one `flatten_jsonl` path reproduces each
/// family's committed snapshot byte-for-byte (proves the uniform surface is a
/// zero-regression reorganization of the per-family output).
#[test]
fn ts_uniform_surface() {
    let update = std::env::var("UPDATE_SNAP").is_ok();
    for case in CASES {
        let out = dispatch("sample.ts", FIXTURE, case.mask).expect("a Source matches .ts");
        let actual = flatten_jsonl(&out).join("\n");

        // Regenerate the committed snapshot: `UPDATE_SNAP=1 cargo test`.
        if update {
            std::fs::write(case.snap, format!("{actual}\n")).expect("write snap");
            eprintln!("updated {}", case.snap);
            continue;
        }

        let expected = std::fs::read_to_string(case.snap).expect("snap missing");
        assert_eq!(
            actual,
            expected.trim_end(),
            "{:?} snapshot drifted. Regenerate with UPDATE_SNAP=1 cargo test, or overwrite \
             {} with:\n----\n{actual}\n----",
            case.tag,
            case.snap,
        );
    }
}

/// The first-match roster: a `.ts` routes to the lang-specific `TsSource`, a `.rs`
/// routes to the lang-specific `RustSource`, a `.go` routes to the lang-specific
/// `GoSource`, an unknown ext routes nowhere.
#[test]
fn roster_routes_by_extension() {
    assert_eq!(source_for("x.ts").expect(".ts").name(), "ts");
    assert_eq!(source_for("x.rs").expect(".rs").name(), "rust");
    assert_eq!(source_for("x.go").expect(".go").name(), "go");
    assert_eq!(source_for("x.dl6").expect(".dl6").name(), "dl6");
    assert_eq!(source_for("x.dl").expect(".dl").name(), "dl6");
    assert!(source_for("x.unknownext").is_none());
}
