//! S6 (CLAUDE.md ledger): a source-extract rule (`scan`/`match`/`ast`/`sg`/
//! `json`/`comment` body op) whose body ALSO joins a plain rel atom used to
//! typecheck clean and silently drop the joined atom at runtime — nothing in
//! `parse_file`'s extraction loop (`src/engine/eval.rs`) evaluates a
//! `BodyItem::Pos`/`Neg` atom against the scanned rows; it falls through the
//! catch-all match arm. `check_rule_types` (src/typecheck.rs) now refuses this
//! shape at typecheck time with `source-rule-extra-atom`, UNLESS the rule's
//! `scan` repo/rev is a data-driven `Term::Var` coordinate (see
//! `Engine::resolve_scan_bindings`) — that is the one shape where a body atom
//! is legitimately consumed (as the coordinate-providing join), so it must
//! stay silent.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("source_rule_extra_atom_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    dir
}

/// `dl <prog> --check --no-daemon`, hermetic (no ambient repo config, no
/// daemon autostart). Returns (exit, stdout, stderr).
fn check(dir: &Path, prog: &str) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args([
            "--db",
            dir.join("db").to_str().unwrap(),
            "--check",
            "--no-daemon",
        ])
        .env("SPREFA_CONFIG", dir.join("no.toml"))
        .current_dir(dir)
        .output()
        .expect("run dl --check");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// (1) FAIL-PRE-FIX: a source rule (`scan` + `match`, literal repo/rev) whose
/// body also joins `tagged_path` — a plain rel atom that nothing in the
/// extraction path ever evaluates — is refused at typecheck time.
#[test]
fn extra_atom_in_literal_scan_source_rule_is_refused() {
    let sandbox_dir = sandbox("literal");
    fs::write(
        sandbox_dir.join("src/marked.rs"),
        "fn important_work() {}\n",
    )
    .unwrap();
    let prog = r#"
rel tagged_path(marked_path: text).
tagged_path("src/marked.rs").

rel important_function(function_path: text, function_name: text).
important_function(function_path, function_name) <-
    scan("WORK", "src/*.rs", function_path, rev),
    match(function_path, rev, /fn (?P<function_name>\w+)/, matched_line),
    tagged_path(function_path).
"#;
    let (code, _stdout, stderr) = check(&sandbox_dir, prog);
    // Exit contract (src/cli/mod.rs): 0 clean, 1 broken program (a lower-time
    // type error skips the tick entirely, same bucket as brand-mismatch/
    // not-stratified/json-in-source), 2 a runtime rail violation. This is a
    // typecheck-time TypeDiag, so it is a "broken program" (1), not a rail (2).
    assert_eq!(
        code, 1,
        "an extra body atom on a literal-coord source rule must block --check:\n{stderr}"
    );
    assert!(
        stderr.contains("source-rule-extra-atom"),
        "expected the source-rule-extra-atom code:\n{stderr}"
    );
    assert!(
        stderr.contains("important_function") && stderr.contains("tagged_path"),
        "message must name the rel and the dropped join:\n{stderr}"
    );
}

/// (2) Negative control: the SAME rule shape minus the extra atom typechecks
/// clean — proves the diagnostic targets the extra join, not `scan`+`match`
/// itself.
#[test]
fn source_rule_without_extra_atom_is_clean() {
    let sandbox_dir = sandbox("clean");
    fs::write(
        sandbox_dir.join("src/marked.rs"),
        "fn important_work() {}\n",
    )
    .unwrap();
    let prog = r#"
rel important_function(function_path: text, function_name: text).
important_function(function_path, function_name) <-
    scan("WORK", "src/*.rs", function_path, rev),
    match(function_path, rev, /fn (?P<function_name>\w+)/, matched_line).
"#;
    let (code, _stdout, stderr) = check(&sandbox_dir, prog);
    assert_eq!(
        code, 0,
        "a plain scan+match source rule must typecheck clean:\n{stderr}"
    );
    assert!(
        !stderr.contains("source-rule-extra-atom"),
        "false positive:\n{stderr}"
    );
}

/// (3) Negative control: a DATA-DRIVEN scan (`repo`/`rev` bound by a
/// coordinate-providing body atom, `Engine::resolve_scan_bindings`'s own
/// documented shape) must NOT be flagged — the body atom there is genuinely
/// consumed, not dropped.
#[test]
fn data_driven_scan_coordinate_atom_is_not_flagged() {
    let sandbox_dir = sandbox("data_driven");
    fs::write(
        sandbox_dir.join("src/marked.rs"),
        "fn important_work() {}\n",
    )
    .unwrap();
    let prog = r#"
rel scan_pin(pinned_repo: text, pinned_rev: text).
scan_pin(".", "WORK").

rel scanned_path(pinned_rev: text, scanned_path: file).
scanned_path(pinned_rev, scanned_path) <-
    scan_pin(pinned_repo, pinned_rev),
    scan(pinned_repo, pinned_rev, "src/*.rs", scanned_path, rev_out).
"#;
    let (code, _stdout, stderr) = check(&sandbox_dir, prog);
    assert_eq!(
        code, 0,
        "a data-driven scan coordinate atom must typecheck clean:\n{stderr}"
    );
    assert!(
        !stderr.contains("source-rule-extra-atom"),
        "false positive on data-driven scan:\n{stderr}"
    );
}
