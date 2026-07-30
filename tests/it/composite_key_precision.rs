//! Precision gate for the de-waived `.dl/composite-key-string.dl` rail: it
//! must FIRE on real composite-string keys and stay SILENT on the false
//! positives the de-waive (removing the 6 baseline rows + `composite_key_ok`)
//! exposed — display strings, SQL fragments, a two-part etag, and
//! `#[cfg(test)]` fixtures. The rail carries NO baseline and NO waiver rel by
//! user law (2026-07-20): a false positive is fixed by narrowing the match
//! PATTERN, never by adding an escape hatch, so this file's GREEN cases
//! double as proof the narrowing actually holds. Harness mirrors
//! `tests/it/eprintln_waiver.rs` / `tests/it/magic_rel_audit.rs`.
//!
//! Two invocation shapes are used:
//!   - `check()` — a temp sandbox (never a git worktree) with `--check`,
//!     reading the diag lines off stderr. Used for the synthetic fixtures.
//!   - `run_against_repo_root()` — the shipped rail run over THIS crate's own
//!     tree (`CARGO_MANIFEST_DIR`) with a PLAIN `--no-daemon` run (no
//!     `--check`): `--check` refuses to cold-build inside a linked git
//!     worktree (failure-modes class 14) and green-by-skips, which would make
//!     a real-tree assertion vacuous; the plain run has no such gate. Every
//!     invocation here sets `XDG_STATE_HOME` to a throwaway per-test temp
//!     dir — the ONLY env var the daemon home code (`src/daemon/home.rs`)
//!     actually reads (`DL_STATE_DIR` is not a real knob) — so nothing is
//!     ever written under the developer's real `~/.local/state/sprefa`.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

/// The repo's actual rail file — the artifact under test, not a copy.
fn rail() -> String {
    format!("{}/.dl/composite-key-string.dl", env!("CARGO_MANIFEST_DIR"))
}

/// A throwaway `XDG_STATE_HOME` for one test — every `dl` invocation below
/// passes this so no run ever touches the developer's real daemon home.
fn scratch_state_home(tag: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("dl_composite_key_xdg_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("dl_composite_key_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    dir
}

/// Run the shipped rail with `--check` over a temp sandbox root. Returns
/// (exit code, stderr).
fn check(dir: &PathBuf) -> (i32, String) {
    let xdg = scratch_state_home("check");
    let out = Command::new(DL)
        .args([&rail(), "--no-daemon", "--check"])
        .current_dir(dir)
        .env("XDG_STATE_HOME", &xdg)
        .output()
        .expect("run dl --check on the composite-key-string rail");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// Run the shipped rail, PLAIN (no `--check`), over this crate's own repo
/// root — the real tree, not a copy — so the real `mint_sym` finding can be
/// asserted without tripping the class-14 worktree-cold-build refusal that
/// only guards the `--check` path. Returns stdout (where the `?
/// composite_key_finding` query prints its rows).
fn run_against_repo_root() -> String {
    let xdg = scratch_state_home("realtree");
    let out = Command::new(DL)
        .args([&rail(), "--no-daemon"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("XDG_STATE_HOME", &xdg)
        .output()
        .expect("run dl on the composite-key-string rail against the repo root");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

// ── RED: real composite-string keys must fire ──────────────────────────────

/// Fail-pre-fix proof: a `let $ID_LIKE_NAME = format!(...)` joining 2+ holes
/// with `::` (this repo's `sym`-namespacing delimiter) fires the rail.
#[test]
fn rail_flags_a_sym_delimited_id_red() {
    let sandbox_dir = sandbox("red_sym");
    fs::write(
        sandbox_dir.join("src/fixture.rs"),
        "fn f() { let node_id = format!(\"{a}::{b}\"); let _ = node_id; }\n",
    )
    .unwrap();
    let (_code, err) = check(&sandbox_dir);
    assert!(
        err.contains("composite-key-as-string"),
        "a `::`-delimited id-shaped format! must warn; stderr:\n{err}"
    );
}

/// The real `mint_sym` site (`src/graph/typegraph/mod.rs`, `format!("{file}::{}::{p}.{name}",
/// kind.tag())`) still fires on the shipped repo tree — the narrowing pass
/// must not have swept away a genuine finding along with the false
/// positives. Asserted by the fn the finding names (`mint_sym`) rather than a
/// hardcoded line, so it tracks the site through unrelated edits above it (the
/// df-id `NodeIdx` normalization shifted mint_sym down ~16 lines).
#[test]
fn rail_flags_the_real_mint_sym_site_red() {
    let stdout = run_against_repo_root();
    assert!(
        stdout.contains("src/graph/typegraph/mod.rs") && stdout.contains("mint_sym"),
        "the real mint_sym composite key (typegraph/mod.rs, fn mint_sym) must still fire; stdout:\n{stdout}"
    );
}

// ── GREEN: the narrowed false-positive shapes must stay silent ─────────────

/// A SQL fragment (`lower.rs`'s `sym_decode` shape) bound to an id-shaped
/// name must NOT warn — SQL syntax is not a key-delimiter shape, and the
/// explicit `sql_format` exclusion is a second, independent reason.
#[test]
fn rail_is_silent_on_a_sql_fragment_green() {
    let sandbox_dir = sandbox("green_sql");
    fs::write(
        sandbox_dir.join("src/fixture.rs"),
        "fn f() { let node_id = format!(\"SELECT {col} FROM {table}\"); let _ = node_id; }\n",
    )
    .unwrap();
    let (_code, err) = check(&sandbox_dir);
    assert!(
        !err.contains("composite-key-as-string"),
        "a SQL-fragment format! bound to an id-shaped name must not warn; stderr:\n{err}"
    );
}

/// A display string (`lsp.rs`'s `handle_hover` shape) bound to an id-shaped
/// name must NOT warn — `\n\n---\n\n` is prose, not a key delimiter.
#[test]
fn rail_is_silent_on_a_display_string_green() {
    let sandbox_dir = sandbox("green_display");
    fs::write(
        sandbox_dir.join("src/fixture.rs"),
        "fn f() { let node_id = format!(\"{a}\\n\\n---\\n\\n{b}\"); let _ = node_id; }\n",
    )
    .unwrap();
    let (_code, err) = check(&sandbox_dir);
    assert!(
        !err.contains("composite-key-as-string"),
        "a display-string format! bound to an id-shaped name must not warn; stderr:\n{err}"
    );
}

/// A two-part etag (`daemon/mod.rs`'s `build_id` shape, `{version}:{secs}`)
/// inside an id-mint-named fn must NOT warn — `key_delimiter_coord` requires
/// a 3+-hole chain, not a single colon between exactly two holes.
#[test]
fn rail_is_silent_on_a_two_part_etag_green() {
    let sandbox_dir = sandbox("green_etag");
    fs::write(
        sandbox_dir.join("src/fixture.rs"),
        "fn build_id() -> String { format!(\"{a}:{b}\") }\n",
    )
    .unwrap();
    let (_code, err) = check(&sandbox_dir);
    assert!(
        !err.contains("composite-key-as-string"),
        "a two-part colon-joined etag inside an id-mint fn must not warn; stderr:\n{err}"
    );
}

/// A `#[cfg(test)] mod tests { }` fixture (`effect.rs:1266`'s
/// `engine_with` shape, `let id = format!("test-{a}-{b}")`) must NOT warn —
/// the `composite_key_test_span` containment check exempts it.
#[test]
fn rail_is_silent_on_a_cfg_test_fixture_green() {
    let sandbox_dir = sandbox("green_test_mod");
    fs::write(
        sandbox_dir.join("src/fixture.rs"),
        "#[cfg(test)]\nmod tests {\n    fn engine_with(a: &str, b: &str) -> String {\n        let id = format!(\"test-{a}-{b}\");\n        id\n    }\n}\n",
    )
    .unwrap();
    let (_code, err) = check(&sandbox_dir);
    assert!(
        !err.contains("composite-key-as-string"),
        "a #[cfg(test)] mod tests fixture must not warn; stderr:\n{err}"
    );
}

/// Bonus: the `composite_key_test_span` exemption specifically, exercised
/// with a shape that WOULD otherwise fire (a `::`-delimited id, the same
/// shape `rail_flags_a_sym_delimited_id_red` proves fires outside a test
/// module) — proves the exemption is doing real work, not just riding along
/// on the etag/display/SQL fixtures above (none of which have a
/// key-delimiter shape to begin with, so they would stay silent even if the
/// test-span exclusion were entirely broken).
#[test]
fn rail_is_silent_on_a_delimited_id_inside_cfg_test_green() {
    let sandbox_dir = sandbox("green_test_mod_delimited");
    fs::write(
        sandbox_dir.join("src/fixture.rs"),
        "#[cfg(test)]\nmod tests {\n    fn f() {\n        let node_id = format!(\"{a}::{b}\");\n        let _ = node_id;\n    }\n}\n",
    )
    .unwrap();
    let (_code, err) = check(&sandbox_dir);
    assert!(
        !err.contains("composite-key-as-string"),
        "a `::`-delimited id inside #[cfg(test)] mod tests must not warn (test-span exemption); stderr:\n{err}"
    );
}
