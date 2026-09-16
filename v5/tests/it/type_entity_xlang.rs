//! Cross-language proof of the common type interface: one datalog query over
//! `type_entity` finds every interface and function whose name contains
//! "network", regardless of source language. TypeScript (oxc) and Kotlin
//! (tree-sitter) both feed the same `TypeLang::extract -> TypeFacts` shape, so
//! the query never mentions a language or an extension.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("type_entity_xlang_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(dir: &Path, prog: &str) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--db", dir.join("db").to_str().unwrap()])
        .current_dir(dir)
        .output()
        .expect("run dl");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

// interfaces OR functions whose name contains "network" (case-insensitive via
// the [Nn] class), across whatever languages the scan picked up.
const PROG: &str = r#"
rel seen(path: file).
seen(path) <- scan("WORK", "src/**/*.{ts,tsx,kt,kts}", path, rev), match(path, rev, /./, line).
rel net(name: text, kind: text, file: file).
net(name, kind, file) <- type_entity(_, sym, name, kind, parent, file, line), kind = "interface", name =~ /[Nn]etwork/.
net(name, kind, file) <- type_entity(_, sym, name, kind, parent, file, line), kind = "function", name =~ /[Nn]etwork/.
? net(name, kind, file).
"#;

#[test]
fn finds_network_interfaces_and_functions_across_languages() {
    let d = sandbox("net");
    fs::create_dir_all(d.join("src")).unwrap();
    // TypeScript: an interface + a function with "network", plus decoys
    fs::write(
        d.join("src/net.ts"),
        "export interface NetworkClient { url: string }\n\
         export interface Cache {}\n\
         export function openNetwork(c: NetworkClient): void {}\n\
         export function unrelated(): void {}\n\
         export type NetworkAlias = string\n",
    )
    .unwrap();
    // Kotlin: same shapes via the tree-sitter front-end
    fs::write(
        d.join("src/Net.kt"),
        "interface NetworkSocket\n\
         interface Plain\n\
         fun pingNetwork(s: NetworkSocket) {}\n\
         fun other() {}\n",
    )
    .unwrap();

    let (code, out, err) = run(&d, PROG);
    assert_eq!(code, 0, "run failed:\nstdout={out}\nstderr={err}");

    // the four cross-language hits
    assert!(
        out.contains("NetworkClient\tinterface\tsrc/net.ts"),
        "ts interface: {out}"
    );
    assert!(
        out.contains("openNetwork\tfunction\tsrc/net.ts"),
        "ts function: {out}"
    );
    assert!(
        out.contains("NetworkSocket\tinterface\tsrc/Net.kt"),
        "kotlin interface: {out}"
    );
    assert!(
        out.contains("pingNetwork\tfunction\tsrc/Net.kt"),
        "kotlin function: {out}"
    );

    // decoys excluded: wrong name, or right name but wrong kind (the alias)
    assert!(
        !out.contains("\tCache\t") && !out.contains("Cache\tinterface"),
        "Cache leaked: {out}"
    );
    assert!(!out.contains("unrelated"), "unrelated fn leaked: {out}");
    assert!(!out.contains("Plain\tinterface"), "Plain leaked: {out}");
    assert!(
        !out.contains("NetworkAlias"),
        "alias is neither interface nor function: {out}"
    );
}
