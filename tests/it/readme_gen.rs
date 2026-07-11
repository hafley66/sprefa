//! README generator e2e: comment_node-owned gallery notes round-trip through
//! named zones, preserve hand prose, and report both directions of drift.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("readme_gen_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("examples")).unwrap();
    fs::create_dir_all(dir.join("std")).unwrap();
    fs::write(dir.join("README.md"), "# Hand title\n\nhand prose before\n\n<!-- BEGIN: readme-gallery -->\n<!-- END: readme-gallery -->\n\nhand prose between\n\n<!-- BEGIN: stdlib -->\n<!-- END: stdlib -->\n\nhand prose after\n").unwrap();
    fs::write(dir.join("examples/query.dl"), "# README(gallery): Ask a real question: `dl examples/query.dl --no-daemon`\nrel answer(value: text).\nanswer(\"ok\").\n").unwrap();
    fs::write(dir.join("std/hello.dl"), "# Standard library: a greeting relation.\nrel hello(value: text).\n").unwrap();
    dir
}

fn dl(dir: &Path, args: &[&str]) -> (String, i32) {
    let program = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/gen-readme.dl")).unwrap();
    fs::write(dir.join("gen-readme.dl"), program).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("gen-readme.dl"))
        .args(["--db", dir.join("db").to_str().unwrap(), "--no-daemon"])
        .args(args)
        .env("SPREFA_CONFIG", "/nonexistent/x.toml")
        .env("DL_NO_DAEMON", "1")
        .current_dir(dir)
        .output().expect("run dl");
    (format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)), out.status.code().unwrap_or(-1))
}

#[test]
fn readme_zones_round_trip_and_detect_both_directions_of_drift() {
    let dir = sandbox("roundtrip");
    let (out, code) = dl(&dir, &[]);
    assert_eq!(code, 0, "generation failed:\n{out}");
    let generated = fs::read_to_string(dir.join("README.md")).unwrap();
    assert!(generated.contains("Ask a real question"), "gallery missing:\n{generated}");
    assert!(generated.contains("[`std/hello.dl`](std/hello.dl) | a greeting relation."), "stdlib missing:\n{generated}");
    for prose in ["hand prose before", "hand prose between", "hand prose after"] {
        assert!(generated.contains(prose), "hand-owned prose changed: {prose}\n{generated}");
    }
    let (out, code) = dl(&dir, &["--check"]);
    assert_eq!(code, 0, "clean check failed:\n{out}");

    fs::write(dir.join("examples/query.dl"), "# README(gallery): Ask a changed question: `dl examples/query.dl --no-daemon`\nrel answer(value: text).\nanswer(\"ok\").\n").unwrap();
    let (out, code) = dl(&dir, &["--check"]);
    assert_eq!(code, 2, "forward drift should exit 2:\n{out}");
    assert!(out.contains("readme-drift"), "missing drift rail:\n{out}");

    fs::write(dir.join("examples/query.dl"), "rel answer(value: text).\nanswer(\"ok\").\n").unwrap();
    let (out, code) = dl(&dir, &["--check"]);
    assert_eq!(code, 2, "orphan check should exit 2:\n{out}");
    assert!(out.contains("readme-orphan"), "missing orphan rail:\n{out}");
}
