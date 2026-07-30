//! The LANG-JUNCTION drift rail (`examples/gen-lang-skill.dl`) must FIRE when
//! the marker set in src/ and the junction list in the sprf-add-language skill
//! disagree, and stay green when they match. Without this, a rotted rail
//! (regex drift, renamed skill path) would pass a bare `--check` silently.
//! Mirrors tests/it/magic_rel_audit.rs: plant files under a temp root, run the
//! SHIPPED rail against it.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

/// The repo's actual rail file — the artifact under test, not a copy.
fn rail() -> String {
    format!("{}/examples/gen-lang-skill.dl", env!("CARGO_MANIFEST_DIR"))
}

fn sandbox(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("dl_langskill_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(p.join("src")).unwrap();
    fs::create_dir_all(p.join(".agents/skills/sprf-add-language")).unwrap();
    p
}

fn write_skill(dir: &PathBuf, zone_rows: &str) {
    fs::write(
        dir.join(".agents/skills/sprf-add-language/SKILL.md"),
        format!(
            "# stub\n\n<!-- BEGIN: lang-junctions -->\n{zone_rows}<!-- END: lang-junctions -->\n"
        ),
    )
    .unwrap();
}

fn check(dir: &PathBuf) -> (i32, String) {
    let out = Command::new(DL)
        .args([&rail(), "--no-daemon", "--check"])
        .env("SPREFA_CONFIG", "/nonexistent/sprefa-hermetic.toml")
        .current_dir(dir)
        .output()
        .expect("run dl --check on the lang-junction rail");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.code().unwrap_or(-1), text)
}

#[test]
fn fires_on_marker_missing_from_skill() {
    let dir = sandbox("drift");
    fs::write(
        dir.join("src/planted.rs"),
        "// LANG-JUNCTION(planted-junction): a registration point the skill has not heard of\nfn body() {}\n",
    )
    .unwrap();
    write_skill(&dir, "");
    let (code, text) = check(&dir);
    assert_eq!(code, 2, "unsynced marker must fail --check; output: {text}");
    assert!(
        text.contains("lang-junction-drift"),
        "expected drift diag: {text}"
    );
}

#[test]
fn green_when_synced() {
    let dir = sandbox("green");
    fs::write(
        dir.join("src/planted.rs"),
        "// LANG-JUNCTION(planted-junction): a registration point\nfn body() {}\n",
    )
    .unwrap();
    write_skill(
        &dir,
        "- `src/planted.rs:1` planted-junction: a registration point\n",
    );
    let (code, text) = check(&dir);
    assert_eq!(
        code, 0,
        "synced marker + skill row must pass; output: {text}"
    );
}

#[test]
fn orphan_fires_on_stale_skill_row() {
    let dir = sandbox("orphan");
    fs::write(dir.join("src/planted.rs"), "fn body() {}\n").unwrap();
    write_skill(
        &dir,
        "- `src/gone.rs:1` vanished-junction: a marker that was deleted\n",
    );
    let (code, text) = check(&dir);
    assert_eq!(code, 2, "stale skill row must fail --check; output: {text}");
    assert!(
        text.contains("lang-junction-orphan"),
        "expected orphan diag: {text}"
    );
}

#[test]
fn string_literal_marker_never_counts() {
    let dir = sandbox("strlit");
    fs::write(
        dir.join("src/planted.rs"),
        "fn body() -> &'static str { \"LANG-JUNCTION(fake-junction): not a comment\" }\n",
    )
    .unwrap();
    write_skill(&dir, "");
    let (code, text) = check(&dir);
    assert_eq!(
        code, 0,
        "a marker inside a string literal must not mint a junction; output: {text}"
    );
}
