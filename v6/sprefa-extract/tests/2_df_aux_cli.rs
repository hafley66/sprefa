use std::process::Command;

struct Case {
    source: &'static str,
    golden: &'static str,
}

const CASES: &[Case] = &[
    Case {
        source: "tests/fixtures/ts/sample.ts",
        golden: include_str!("fixtures/df_aux/ts.jsonl"),
    },
    Case {
        source: "tests/fixtures/rust/sample.rs",
        golden: include_str!("fixtures/df_aux/rust.jsonl"),
    },
    Case {
        source: "tests/fixtures/go/sample.go",
        golden: include_str!("fixtures/df_aux/go.jsonl"),
    },
    Case {
        source: "tests/fixtures/kotlin/sample.kt",
        golden: include_str!("fixtures/df_aux/kotlin.jsonl"),
    },
];

#[test]
fn df_aux_cli_goldens_cover_all_projectors() {
    for case in CASES {
        let output = Command::new(env!("CARGO_BIN_EXE_extract"))
            .args(["--family", "df", case.source])
            .output()
            .expect("extract binary runs");
        assert!(
            output.status.success(),
            "{} stderr: {}",
            case.source,
            String::from_utf8_lossy(&output.stderr)
        );
        let aux = output
            .stdout
            .split(|byte| *byte == b'\n')
            .filter(|line| {
                line.starts_with(b"{\"record\":\"param\"")
                    || line.starts_with(b"{\"record\":\"arg\"")
            })
            .map(|line| String::from_utf8(line.to_vec()).expect("JSONL is UTF-8"))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(
            format!("{aux}\n"),
            case.golden,
            "{} aux records",
            case.source
        );
    }
}
