//! Proof of the built-in `type_edge` relation: a syn-backed Rust extractor emits
//! deterministic type graph edges, and `closure(type_edge)` walks the first two
//! columns while preserving `kind` for direct queries.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("type_graph_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(dir: &Path, prog: &str, extra: &[&str]) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args([
            "--root",
            dir.to_str().unwrap(),
            "--db",
            dir.join("db").to_str().unwrap(),
        ])
        .args(extra)
        .output()
        .expect("run dl");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

const PROG: &str = r#"
rel seen(path: file).
seen(path) <- scan("WORK", "src/**/*.rs", path, rev), match(path, rev, /./, line).
rel type_reaches(a: text, b: text).
type_reaches(a, b) <- closure(type_edge).
? type_edge(f, t, k).
? type_reaches(a, b).
"#;

#[test]
fn rust_type_edges_and_closure() {
    let d = sandbox("rust");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(
        d.join("src/lib.rs"),
        r#"
trait Identity {}
trait Store {}
struct Id;
struct Meta<T>(T);
struct User<T: Identity> {
    id: Id,
    meta: Option<Meta<T>>,
}
enum Event {
    Created(User<Id>),
    Deleted { id: Id },
}
impl<T: Identity> Store for User<T> {}
"#,
    )
    .unwrap();

    let (code, out, err) = run(&d, PROG, &[]);
    assert_eq!(code, 0, "run failed:\nstdout={out}\nstderr={err}");
    assert!(out.contains("User\tId\tfield"), "struct field edge: {out}");
    assert!(
        out.contains("User\tIdentity\tgeneric"),
        "generic bound edge: {out}"
    );
    assert!(
        out.contains("Event\tEvent::Created\tvariant"),
        "enum variant edge: {out}"
    );
    assert!(
        out.contains("Event::Created\tUser\tfield"),
        "variant payload edge: {out}"
    );
    assert!(out.contains("User\tStore\timpl"), "trait impl edge: {out}");

    let reaches = out.split("? type_reaches").nth(1).unwrap_or("");
    assert!(
        reaches.contains("Event\tUser"),
        "closure should walk variant payload edge: {out}"
    );
}

#[test]
fn declaring_type_edge_errors() {
    let d = sandbox("collision");
    let prog = "rel type_edge(a: text, b: text).\n";
    let (code, _, err) = run(&d, prog, &[]);
    assert_ne!(code, 0, "redeclaring `type_edge` must fail");
    assert!(
        err.contains("built-in type-graph relation"),
        "expected type-edge error, got: {err}"
    );
}
