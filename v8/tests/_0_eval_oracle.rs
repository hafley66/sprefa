//! Oracle parity through the real binary. Every `oracle/eval/*.json` holds a
//! program and the closure v7 `evaluate/4` produced for it, frozen by
//! `oracle/eval/dump_eval.pl`. `dl8 eval` must print the same closure and the
//! same diagnostics.

use std::path::{Path, PathBuf};
use std::process::Command;

fn fixtures() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("oracle/eval");
    let mut out: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    out.sort();
    out
}

#[test]
fn every_oracle_fixture_matches_v7() {
    let fixtures = fixtures();
    assert!(!fixtures.is_empty(), "no oracle fixtures found");
    let mut failures = Vec::new();
    for path in &fixtures {
        let text = std::fs::read_to_string(path).unwrap();
        let fixture: serde_json::Value = serde_json::from_str(&text).unwrap();
        let program_path = std::env::temp_dir().join(format!(
            "dl8-oracle-{}-{}.json",
            std::process::id(),
            path.file_stem().unwrap().to_string_lossy()
        ));
        std::fs::write(
            &program_path,
            serde_json::to_string(&fixture["program"]).unwrap(),
        )
        .unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_dl8"))
            .arg("eval")
            .arg(&program_path)
            .output()
            .unwrap();
        let _ = std::fs::remove_file(&program_path);
        let got: serde_json::Value = match serde_json::from_slice(&output.stdout) {
            Ok(v) => v,
            Err(e) => {
                failures.push(format!(
                    "{}: no JSON on stdout ({e}); stderr: {}",
                    path.display(),
                    String::from_utf8_lossy(&output.stderr)
                ));
                continue;
            }
        };
        let want = &fixture["expected"];
        if got["closure"] != want["closure"] {
            failures.push(format!(
                "{}: closure differs\n  want: {}\n  got:  {}",
                path.display(),
                want["closure"],
                got["closure"]
            ));
        }
        if got["diagnostics"] != want["diagnostics"] {
            failures.push(format!(
                "{}: diagnostics differ\n  want: {}\n  got:  {}",
                path.display(),
                want["diagnostics"],
                got["diagnostics"]
            ));
        }
        let want_code = if want["diagnostics"].as_array().is_none_or(|d| d.is_empty()) {
            0
        } else {
            1
        };
        if output.status.code() != Some(want_code) {
            failures.push(format!(
                "{}: exit {:?}, want {want_code}",
                path.display(),
                output.status.code()
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
