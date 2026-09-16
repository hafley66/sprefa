//! Doc-comment extraction (Tier 1 `doc_comment` + Tier 2 `doc_tag`) across the
//! three TypeLang front-ends. The locator is AST-anchored per language: Rust
//! reads `#[doc]` attrs (so a `#[derive]` between the doc and the `struct` is a
//! non-issue), Kotlin the preceding KDoc sibling, TS the `/** */` block that
//! immediately precedes the decl. Both rels join `type_entity` on `sym`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("doc_comment_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    dir
}

fn run(dir: &Path, prog: &str) -> String {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--db", dir.join("db").to_str().unwrap()])
        .current_dir(dir)
        .output()
        .expect("run dl");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

const PROG: &str = r#"
rel seen(p: file).
seen(p) <- scan("WORK", "src/**/*.{rs,ts,kt}", p, rev).
? doc_comment(repo, sym, line, text).
? doc_tag(repo, sym, tag, arg, text).
"#;

#[test]
fn extracts_docs_and_tags_across_languages() {
    let d = sandbox("xlang");
    // Rust: doc sits ABOVE a #[derive], with rustdoc `# Section` headings.
    fs::write(d.join("src/lib.rs"),
        "/// Adds two numbers.\n///\n/// # Panics\n/// Never.\n#[derive(Debug)]\npub struct Calc;\n\
         /// Free fn.\npub fn add(a: i32, b: i32) -> i32 { a + b }\n").unwrap();
    // TS: JSDoc with @param/@returns/@deprecated.
    fs::write(d.join("src/api.ts"),
        "/**\n * Fetch a user.\n * @param id the id\n * @returns the User\n * @deprecated soon\n */\n\
         export function fetchUser(id: string): User { return null as any; }\n\
         /** An interface. */\nexport interface User { name: string; }\n").unwrap();
    // Kotlin: KDoc with @param.
    fs::write(
        d.join("src/Svc.kt"),
        "/**\n * A service.\n * @param host the host\n */\nclass Svc(host: String) {}\n",
    )
    .unwrap();

    let out = run(&d, PROG);

    // Tier 1: one doc_comment per documented entity, keyed by its type_entity sym.
    assert!(
        out.contains("src/lib.rs::struct::Calc"),
        "rust struct doc missing:\n{out}"
    );
    assert!(
        out.contains("src/lib.rs::function::add"),
        "rust fn doc missing:\n{out}"
    );
    assert!(
        out.contains("Adds two numbers."),
        "rust summary missing:\n{out}"
    );
    assert!(
        out.contains("src/api.ts::function::fetchUser"),
        "ts fn doc missing:\n{out}"
    );
    assert!(
        out.contains("src/api.ts::interface::User"),
        "ts interface doc missing:\n{out}"
    );
    assert!(
        out.contains("src/Svc.kt::class::Svc"),
        "kotlin class doc missing:\n{out}"
    );

    // Tier 2: rustdoc sections + JSDoc/KDoc @tags.
    assert!(
        out.contains("section\tPanics"),
        "rust section tag missing:\n{out}"
    );
    assert!(
        out.contains("param\tid\tthe id"),
        "ts @param tag missing:\n{out}"
    );
    assert!(
        out.contains("returns\t\tthe User"),
        "ts @returns tag missing:\n{out}"
    );
    assert!(
        out.contains("deprecated\t\tsoon"),
        "ts @deprecated tag missing:\n{out}"
    );
    assert!(
        out.contains("param\thost\tthe host"),
        "kotlin @param tag missing:\n{out}"
    );
}
