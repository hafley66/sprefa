//! The `json` op dispatches on file extension: json, yaml/yml, toml. One
//! sandbox per format, same dotted-path surface, values must come back with
//! quotes stripped. TOML dotted keys consume multiple path segments; YAML
//! multi-document streams match every document.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("data_ops_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(dir: &Path, prog: &str) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--root", dir.to_str().unwrap(), "--db", dir.join("db").to_str().unwrap()])
        .output().expect("run dl");
    (out.status.code().unwrap_or(-1),
     String::from_utf8_lossy(&out.stdout).into_owned(),
     String::from_utf8_lossy(&out.stderr).into_owned())
}

fn prog(glob: &str, jpath: &str) -> String {
    format!(concat!(
        "rel val(p: file, v: text).\n",
        "val(p, v) <- scan(\"WORK\", \"{}\", p, rev), json(p, rev, \"{}\", v).\n",
        "? val(p, v).\n"), glob, jpath)
}

#[test]
fn yaml_dotted_path() {
    let d = sandbox("yaml");
    fs::write(d.join("deploy.yaml"), concat!(
        "spec:\n",
        "  template:\n",
        "    containers:\n",
        "      - name: app\n",
        "        image: \"registry.io/app:v3\"\n",
        "      - name: sidecar\n",
        "        image: registry.io/sidecar:v1\n")).unwrap();
    let (code, out, err) = run(&d, &prog("*.yaml", "spec.template.containers.*.image"));
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("registry.io/app:v3"), "double-quoted scalar, quotes stripped:\n{out}");
    assert!(out.contains("registry.io/sidecar:v1"), "plain scalar:\n{out}");
    assert!(!out.contains("\"registry.io/app:v3\""), "quotes must be stripped:\n{out}");
}

#[test]
fn yaml_multi_document_stream() {
    let d = sandbox("yaml_multi");
    fs::write(d.join("all.yml"), concat!(
        "metadata:\n  name: svc-a\n",
        "---\n",
        "metadata:\n  name: svc-b\n")).unwrap();
    let (code, out, err) = run(&d, &prog("*.yml", "metadata.name"));
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("svc-a") && out.contains("svc-b"), "both documents must match:\n{out}");
}

#[test]
fn toml_tables_and_dotted_keys() {
    let d = sandbox("toml");
    fs::write(d.join("Settings.toml"), concat!(
        "top = 1\n",
        "workspace.members = \"crates\"\n",
        "[dependencies]\n",
        "serde = { version = \"1.0\", features = [\"derive\"] }\n",
        "[dependencies.tokio]\n",
        "version = \"1.38\"\n")).unwrap();
    let (code, out, err) = run(&d, &prog("*.toml", "dependencies.serde.version"));
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("1.0"), "inline table value:\n{out}");

    let (_, out, _) = run(&d, &prog("*.toml", "dependencies.tokio.version"));
    assert!(out.contains("1.38"), "dotted table header:\n{out}");

    let (_, out, _) = run(&d, &prog("*.toml", "workspace.members"));
    assert!(out.contains("crates"), "dotted pair key spans two segments:\n{out}");
}

#[test]
fn json_still_works() {
    let d = sandbox("json");
    fs::write(d.join("pkg.json"), r#"{"scripts": {"build": "tsc -p ."}}"#).unwrap();
    let (code, out, err) = run(&d, &prog("*.json", "scripts.build"));
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("tsc -p ."), "{out}");
}
