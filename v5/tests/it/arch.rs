//! `std/arch.dl` — architecture self-documentation from ARCH JSON markers in
//! comments. Proves marker detection, jsonp url extraction, parent/order
//! derivation, extra-field jsonp over arch_json, and string-literal safety
//! (the comment_node guarantee).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");
const REPO: &str = env!("CARGO_MANIFEST_DIR");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("arch_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    let std_src = Path::new(REPO).join("std");
    let std_dst = dir.join("std");
    fs::create_dir_all(&std_dst).unwrap();
    for e in fs::read_dir(&std_src).unwrap() {
        let e = e.unwrap();
        if e.path().extension().is_some_and(|x| x == "dl") {
            fs::copy(e.path(), std_dst.join(e.file_name())).unwrap();
        }
    }
    dir
}

fn run(dir: &Path, prog: &str) -> String {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--db", dir.join("db").to_str().unwrap(), "--no-daemon"])
        .current_dir(dir)
        .output()
        .expect("run dl");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        panic!("dl failed:\nstdout:\n{stdout}\nstderr:\n{stderr}");
    }
    stdout
}

/// Drive the arch lib: scan a Rust file with ARCH markers at various depths
/// and verify url extraction, parent/order derivation, and string-literal
/// safety (line 8 has ARCH inside a string — must NOT produce a row).
#[test]
fn marker_detection_parent_order_string_safety() {
    let d = sandbox("core");
    fs::write(
        d.join("src/lib.rs"),
        "// ARCH {\"url\":\"app/cli/00-main\"}\n\
         fn main() {}\n\
         // ARCH {\"url\":\"app/cli/01-parse\"}\n\
         fn parse() {}\n\
         // ARCH {\"url\":\"app/engine/00-tick\"}\n\
         // ARCH {\"url\":\"app/engine/01-dispatch/00-route\",\"role\":\"entry\"}\n\
         fn route() {}\n\
         const FAKE: &str = \"// ARCH {\\\"url\\\":\\\"fake\\\"}\";\n",
    )
    .unwrap();

    let prog = r#"
use "std/arch.dl".
rel seen(p: file).
seen(p) <- scan("WORK", "src/**/*.rs", p, rev).

? arch_node(path, line, url).
? arch_parent(url, parent).
? arch_order(url, order).
? arch_last(url, seg).
"#;
    let out = run(&d, prog);

    // arch_node: exactly the 4 real markers (line 8 string literal excluded).
    assert!(out.contains("app/cli/00-main"), "url 1:\n{out}");
    assert!(out.contains("app/cli/01-parse"), "url 2:\n{out}");
    assert!(out.contains("app/engine/00-tick"), "url 3:\n{out}");
    assert!(
        out.contains("app/engine/01-dispatch/00-route"),
        "url 4:\n{out}"
    );
    assert!(!out.contains("fake"), "string literal leaked:\n{out}");

    // arch_parent: each multi-segment url's parent is the prefix minus last seg.
    assert!(
        out.contains("app/cli/00-main\tapp/cli"),
        "parent shallow:\n{out}"
    );
    assert!(
        out.contains("app/engine/01-dispatch/00-route\tapp/engine/01-dispatch"),
        "parent deep:\n{out}"
    );

    // arch_order: 00-main -> 0, 01-parse -> 1, 00-tick -> 0, 00-route -> 0.
    assert!(out.contains("app/cli/00-main\t0"), "order main=0:\n{out}");
    assert!(out.contains("app/cli/01-parse\t1"), "order parse=1:\n{out}");

    // arch_last: last segment of the deep url.
    assert!(
        out.contains("app/engine/01-dispatch/00-route\t00-route"),
        "last seg:\n{out}"
    );
}

/// Extra JSON fields are reachable via arch_json + declarative json patterns
/// and via jsonp. Both work over the raw payload string.
#[test]
fn extra_json_fields_via_json_pattern() {
    let d = sandbox("jsonpat");
    fs::write(
        d.join("src/lib.rs"),
        "// ARCH {\"url\":\"x/00-y\",\"role\":\"entry\",\"layer\":\"cli\"}\n\
         fn y() {}\n",
    )
    .unwrap();

    let prog = r#"
use "std/arch.dl".
rel seen(p: file).
seen(p) <- scan("WORK", "src/**/*.rs", p, rev).
# declarative json: one pattern binds multiple fields
rel via_json(path: file, line: int, url: text, role: text).
via_json(p, l, url, role) <- arch_json(p, l, j), json(j, q:{ "url": $url, "role": $role }).
# jsonp: independent single-field extraction
rel via_jsonp(path: file, line: int, layer: text).
via_jsonp(p, l, layer) <- arch_json(p, l, j), jsonp(j, "layer", layer).
? via_json(path, line, url, role).
? via_jsonp(path, line, layer).
"#;
    let out = run(&d, prog);
    assert!(out.contains("entry"), "role via json pattern:\n{out}");
    assert!(out.contains("cli"), "layer via jsonp:\n{out}");
}

/// A `#` comment (Python-style) also works — comment_node is grammar-agnostic.
#[test]
fn hash_comment_python() {
    let d = sandbox("py");
    fs::write(
        d.join("src/app.py"),
        "# ARCH {\"url\":\"py/00-main\"}\n\
         def main():\n    pass\n",
    )
    .unwrap();

    let prog = r#"
use "std/arch.dl".
rel seen(p: file).
seen(p) <- scan("WORK", "src/**/*.py", p, rev).
? arch_node(path, line, url).
"#;
    let out = run(&d, prog);
    assert!(out.contains("py/00-main"), "py url:\n{out}");
}
