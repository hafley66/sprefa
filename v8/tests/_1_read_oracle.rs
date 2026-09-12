//! Reader parity through the real binary. Every `oracle/read/*.json` holds one
//! input and the forms, source rows and diagnostics v7 `read_dl7/5` produced.

use std::path::{Path, PathBuf};
use std::process::Command;

fn fixtures() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("oracle/read");
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
        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
        let input_path =
            std::env::temp_dir().join(format!("dl8-read-{}-{}.dl7", std::process::id(), stem));
        std::fs::write(&input_path, fixture["input"].as_str().unwrap()).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_dl8"))
            .arg("read")
            .arg(&input_path)
            .env("DL8_READ_PATH", fixture["path"].as_str().unwrap())
            .output()
            .unwrap();
        let _ = std::fs::remove_file(&input_path);
        let got: serde_json::Value = match serde_json::from_slice(&output.stdout) {
            Ok(v) => v,
            Err(e) => {
                failures.push(format!(
                    "{stem}: no JSON on stdout ({e}); stderr: {}",
                    String::from_utf8_lossy(&output.stderr)
                ));
                continue;
            }
        };
        let want = &fixture["expected"];
        for key in ["forms", "source_rows", "diagnostics"] {
            if got[key] != want[key] {
                failures.push(format!(
                    "{stem}: {key} differ\n  want: {}\n  got:  {}",
                    want[key], got[key]
                ));
            }
        }
        let want_code = if want["diagnostics"].as_array().is_some_and(|d| d.is_empty()) {
            0
        } else {
            1
        };
        if output.status.code() != Some(want_code) {
            failures.push(format!(
                "{stem}: exit {:?}, want {want_code}",
                output.status.code()
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
