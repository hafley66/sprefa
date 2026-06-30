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
? type_edge(f, t, k, _).
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
fn type_edges_carry_rev() {
    let d = sandbox("rev");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(
        d.join("src/lib.rs"),
        "struct Id;\nstruct User { id: Id }\n",
    )
    .unwrap();

    // A fresh (non-git) root scans everything as the WORK rev, so the rev-aware
    // table tags edges WORK and a WORK-filtered relation recovers them.
    let prog = r#"
rel seen(path: file).
seen(path) <- scan("WORK", "src/**/*.rs", path, rev), match(path, rev, /./, line).
rel work_type(a: text, b: text).
work_type(a, b) <- type_edge_rev(a, b, _, "WORK", _).
? type_edge_rev(f, t, k, rev, _).
? work_type(a, b).
"#;
    let (code, out, err) = run(&d, prog, &[]);
    assert_eq!(code, 0, "run failed:\nstdout={out}\nstderr={err}");
    assert!(
        out.contains("User\tId\tfield\tWORK"),
        "rev-tagged edge: {out}"
    );
    let work = out.split("? work_type").nth(1).unwrap_or("");
    assert!(
        work.contains("User\tId"),
        "WORK-filtered relation recovers the edge: {out}"
    );
}

/// Two config repos that happen to declare a same-named type (`Auth -> Id`,
/// `field`) used to collapse into ONE `type_edge` row when scanned together
/// in a single engine instance — the bare name carried no repo tag, so
/// closure/fan-out queries silently merged unrelated trees that share
/// vocabulary (e.g. comparing a crate against an old fork of itself). The
/// trailing `repo` column keeps them apart without disturbing cols[0]/[1]
/// (`from`/`to`), so `closure(type_edge)`/`scc(type_edge)` are unaffected.
#[test]
fn type_edge_distinguishes_repos() {
    let d = sandbox("two_repos");
    let _ = fs::remove_dir_all(&d);
    for slug in ["ra", "rb"] {
        let r = d.join(slug);
        fs::create_dir_all(r.join("src")).unwrap();
        fs::write(r.join("src/lib.rs"), "struct Id;\nstruct Auth { id: Id }\n").unwrap();
    }
    fs::write(
        d.join("cfg.toml"),
        format!(
            "[[repos]]\nslug = \"ra\"\nroot = \"{a}\"\n[[repos]]\nslug = \"rb\"\nroot = \"{b}\"\n",
            a = d.join("ra").display(),
            b = d.join("rb").display()
        ),
    )
    .unwrap();
    fs::write(
        d.join("p.dl"),
        "\
        rel seen(path: file).\n\
        seen(path) <- scan(\"*\", \"WORK\", \"src/**/*.rs\", path, rev), match(path, rev, /./, line).\n\
        ? type_edge(f, t, k, r).\n",
    )
    .unwrap();

    let out = Command::new(DL)
        .arg(d.join("p.dl"))
        .args(["--root", d.join("ra").to_str().unwrap(), "--db", d.join("db").to_str().unwrap()])
        .env("SPREFA_CONFIG", d.join("cfg.toml"))
        .output()
        .expect("run dl");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "run failed: {stdout}\n{}", String::from_utf8_lossy(&out.stderr));

    let rows: Vec<&str> = stdout
        .split("? type_edge")
        .nth(1)
        .unwrap_or("")
        .lines()
        .filter(|l| l.contains("Auth\tId\tfield"))
        .collect();
    assert_eq!(rows.len(), 2, "one Auth->Id edge per repo, not collapsed to one: {stdout}");
    assert!(rows.iter().any(|l| l.trim().ends_with("\tra")), "ra-tagged row: {rows:?}");
    assert!(rows.iter().any(|l| l.trim().ends_with("\trb")), "rb-tagged row: {rows:?}");
}

#[test]
fn declaring_type_edge_errors() {
    let d = sandbox("collision");
    for name in ["type_edge", "type_edge_rev"] {
        let prog = format!("rel {name}(a: text, b: text).\n");
        let (code, _, err) = run(&d, &prog, &[]);
        assert_ne!(code, 0, "redeclaring `{name}` must fail");
        assert!(
            err.contains("built-in type-graph relation"),
            "expected type-edge error for {name}, got: {err}"
        );
    }
}
