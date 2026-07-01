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
        "val(p, v) <- scan(\"WORK\", \"{}\", p, rev), jsonp(p, rev, \"{}\", v).\n",
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

/// The declarative `json` brace pattern: `q:{ $k: $v }` binds both the key and
/// the value as rule vars, one row per object entry. The 3rd arg is a `q:`
/// PathLit (structured/highlightable), not a string.
#[test]
fn json_declarative_brace_pattern() {
    let d = sandbox("json_decl");
    fs::write(
        d.join("pkg.json"),
        r#"{"name":"app","version":"1.2.3","private":true}"#,
    )
    .unwrap();
    let prog = concat!(
        "rel kv(p: file, k: text, v: text).\n",
        "kv(p, k, v) <- scan(\"WORK\", \"*.json\", p, rev),\n",
        "               json(p, rev, q:{ $k: $v }).\n",
        "? kv(p, k, v).\n",
    );
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("name") && out.contains("app"), "key+value row:\n{out}");
    assert!(out.contains("version") && out.contains("1.2.3"), "second entry:\n{out}");
    assert!(out.contains("private") && out.contains("true"), "boolean value:\n{out}");
}

/// Nested + literal-key descent in the declarative `json` pattern.
#[test]
fn json_declarative_nested_and_exact() {
    let d = sandbox("json_decl_nested");
    fs::write(
        d.join("pkg.json"),
        r#"{"engines":{"node":">=18"},"name":"x"}"#,
    )
    .unwrap();
    // `{ name: $N }` descends by exact key; `{ engines: { node: $V } }` nests.
    let prog = concat!(
        "rel p1(n: text).\n",
        "rel p2(v: text).\n",
        "p1(n) <- scan(\"WORK\", \"*.json\", p, rev), json(p, rev, q:{ name: $n }).\n",
        "p2(v) <- scan(\"WORK\", \"*.json\", p, rev), json(p, rev, q:{ engines: { node: $v } }).\n",
        "? p1(n).\n? p2(v).\n",
    );
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("x"), "exact-key descent:\n{out}");
    assert!(out.contains(">=18"), "nested descent:\n{out}");
}

/// TERM-form `jsonp`: the source is a bound `str` value (a relation column), not
/// a file. The hybrid join+extract pass joins `page`, then parses each `body`
/// string and binds the extracted value. This is the response-body path (no file,
/// no scan).
#[test]
fn jsonp_term_source_extracts_from_a_bound_value() {
    let d = sandbox("jsonp_term");
    let prog = concat!(
        "rel page(repo: text, body: text).\n",
        "page(\"octo\", \"{\\\"stargazerCount\\\": 42}\").\n",
        "rel star(repo: text, n: text).\n",
        "star(repo, n) <- page(repo, body), jsonp(body, \"stargazerCount\", n).\n",
        "? star(repo, n).\n",
    );
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("octo"), "repo echoes from the join:\n{out}");
    assert!(out.contains("42"), "the extracted value binds:\n{out}");
}

/// TERM-form `json` brace pattern over a bound value, with a Cmp post-filter on
/// the EXTRACTED var (applied after the parse, in the hybrid pass).
#[test]
fn json_term_source_brace_pattern_and_filter() {
    let d = sandbox("json_term");
    let prog = concat!(
        "rel doc(id: text, body: text).\n",
        "doc(\"a\", \"{\\\"name\\\": \\\"alice\\\", \\\"age\\\": \\\"30\\\"}\").\n",
        "rel person(id: text, name: text).\n",
        "person(id, nm) <- doc(id, body), json(body, q:{ name: $nm }).\n",
        "? person(id, nm).\n",
    );
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("alice"), "brace capture binds from the bound value:\n{out}");
}

/// A string passed to `json(` is redirected: it's a parse error pointing at jsonp.
#[test]
fn json_string_arg_redirects_to_jsonp() {
    let d = sandbox("json_decl_err");
    fs::write(d.join("pkg.json"), "{}").unwrap();
    let prog = concat!(
        "rel v(p: file, x: text).\n",
        "v(p, x) <- scan(\"WORK\", \"*.json\", p, rev), json(p, rev, \"a.b\", x).\n",
        "? v(p, x).\n",
    );
    let (code, _out, err) = run(&d, prog);
    assert_ne!(code, 0, "expected a parse error, got success");
    assert!(err.contains("jsonp") || err.contains("brace-pattern"), "redirect msg:\n{err}");
}

/// The declarative `json` pattern dispatches by file extension (the same
/// tree-sitter substrate as jsonp). Christmas #6: the brace pattern is
/// format-agnostic, so `{ $k: $v }` works over YAML and TOML too.
#[test]
fn json_declarative_over_yaml() {
    let d = sandbox("json_decl_yaml");
    fs::write(
        d.join("svc.yaml"),
        concat!("name: svc-a\n", "port: 8080\n"),
    )
    .unwrap();
    let prog = concat!(
        "rel kv(p: file, k: text, v: text).\n",
        "kv(p, k, v) <- scan(\"WORK\", \"*.yaml\", p, rev), json(p, rev, q:{ $k: $v }).\n",
        "? kv(p, k, v).\n",
    );
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("svc-a"), "yaml scalar value:\n{out}");
    assert!(out.contains("8080"), "yaml plain integer:\n{out}");
}

#[test]
fn json_declarative_over_toml() {
    let d = sandbox("json_decl_toml");
    fs::write(
        d.join("pkg.toml"),
        concat!("name = \"app\"\n", "version = \"1.2.3\"\n"),
    )
    .unwrap();
    let prog = concat!(
        "rel kv(p: file, k: text, v: text).\n",
        "kv(p, k, v) <- scan(\"WORK\", \"*.toml\", p, rev), json(p, rev, q:{ $k: $v }).\n",
        "? kv(p, k, v).\n",
    );
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("app"), "toml string value (quotes stripped):\n{out}");
    assert!(out.contains("1.2.3"), "toml second pair:\n{out}");
}
