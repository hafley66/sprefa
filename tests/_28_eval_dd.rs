//! Oracle parity through the real binary with `DL8_ENGINE=dd`.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Fixtures the DD engine must keep passing.
const FLOOR: usize = 11;

fn fixtures() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("oracle/eval");
    let mut out: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect();
    out.sort();
    out
}

#[test]
fn dd_engine_matches_v7_oracles() {
    let fixtures = fixtures();
    assert!(!fixtures.is_empty(), "no eval oracle fixtures found");
    let mut pass = 0usize;
    let mut failures = Vec::new();
    for path in &fixtures {
        let text = std::fs::read_to_string(path).unwrap();
        let fixture: serde_json::Value = serde_json::from_str(&text).unwrap();
        if fixture.get("expected").is_none() {
            continue;
        }
        let program_path = std::env::temp_dir().join(format!(
            "dl8-dd-{}-{}.json",
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
            .env("DL8_ENGINE", "dd")
            .output()
            .unwrap();
        let _ = std::fs::remove_file(&program_path);
        let got: serde_json::Value = match serde_json::from_slice(&output.stdout) {
            Ok(value) => value,
            Err(error) => {
                failures.push(format!(
                    "{}: no JSON on stdout ({error}); stderr: {}",
                    path.display(),
                    String::from_utf8_lossy(&output.stderr)
                ));
                continue;
            }
        };
        let want = &fixture["expected"];
        let same = got["closure"] == want["closure"]
            && got["diagnostics"] == want["diagnostics"]
            && output.status.code()
                == Some(
                    if want["diagnostics"]
                        .as_array()
                        .is_none_or(|diagnostics| diagnostics.is_empty())
                    {
                        0
                    } else {
                        1
                    },
                );
        if same {
            pass += 1;
        } else {
            failures.push(path.display().to_string());
        }
    }
    let total = fixtures
        .iter()
        .filter(|path| {
            serde_json::from_str::<serde_json::Value>(
                &std::fs::read_to_string(path).unwrap_or_default(),
            )
            .map(|value| value.get("expected").is_some())
            .unwrap_or(false)
        })
        .count();
    println!("dd oracles {pass}/{total}");
    for failure in &failures {
        println!("failed: {failure}");
    }
    assert!(
        pass >= FLOOR,
        "dd oracles {pass}/{total} fell below the floor {FLOOR}: {}",
        failures.join(", ")
    );
}
