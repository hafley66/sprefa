//! C6 — the op/family authoring surface must contain ZERO raw SQL. The
//! `op-raw-sql` finding (an extension of the `bespoke-sql` scan in
//! `.dl/rusqlite-coupling.dl`, scoped to `src/engine/family/*.rs` and
//! escalated from a >=4-site WARNING to an any-hit ERROR) must FIRE on a
//! hand-rolled `db.prepare("SELECT ...")` planted in an op file, and stay
//! green on the three real op files (`call_site.rs`/`call_edge.rs`/
//! `call_name.rs`) — the only sanctioned SQL read site is `Ctx::scan`
//! (`src/engine/family/mod.rs`). Here we run the SHIPPED rail (not a copy)
//! against both the real repo root and a planted sandbox.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

/// The repo's actual rail file — the artifact under test, not a copy.
fn rail() -> String {
    format!("{}/.dl/rusqlite-coupling.dl", env!("CARGO_MANIFEST_DIR"))
}

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("dl_family_op_raw_sql_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src/engine/family")).unwrap();
    dir
}

/// Run the shipped rail with `--check` over `dir`. Returns (exit code,
/// stdout, stderr).
fn check(dir: &PathBuf) -> (i32, String, String) {
    let out = Command::new(DL)
        .args([&rail(), "--no-daemon", "--check"])
        .current_dir(dir)
        .env("SPREFA_CONFIG", "/dev/null")
        .output()
        .expect("run dl --check on the rusqlite-coupling rail");
    (out.status.code().unwrap_or(-1),
     String::from_utf8_lossy(&out.stdout).into_owned(),
     String::from_utf8_lossy(&out.stderr).into_owned())
}

/// The three real op-authoring files must pass with zero `op-raw-sql`
/// findings. Run against the actual repo root, not a sandbox copy, so this
/// proves the CURRENT `call_site.rs`/`call_edge.rs`/`call_name.rs` are clean
/// under the shipped rail — not just a synthetic fixture that happens to be
/// clean.
#[test]
fn rail_green_on_real_family_files() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let out = Command::new(DL)
        .args([&rail(), "--no-daemon", "--check"])
        .current_dir(&root)
        .env("SPREFA_CONFIG", "/dev/null")
        .output()
        .expect("run dl --check on the rusqlite-coupling rail over the real repo");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !text.contains("op-raw-sql"),
        "the real op-authoring files (call_site.rs/call_edge.rs/call_name.rs) must carry \
         zero op-raw-sql findings: {text}"
    );
}

/// A synthetic op file with a hand-rolled `db.prepare("SELECT ...")` — the
/// exact banned shape (raw SQL in an op file instead of routed through
/// `Ctx::scan`) — must trip the rail as an ERROR (`--check` exits 2).
#[test]
fn rail_flags_raw_sql_in_planted_family_file() {
    let dir = sandbox("dirty");
    fs::write(
        dir.join("src/engine/family/call_dirty.rs"),
        "use super::*;\n\
         pub(crate) struct CallDirty;\n\
         impl Family for CallDirty {\n\
         \x20   fn name(&self) -> &'static str { \"call_dirty\" }\n\
         \x20   fn out_cols(&self) -> &'static [&'static str] { &[\"a\"] }\n\
         \x20   fn input_rels(&self) -> &'static [&'static str] { &[\"_call_def\"] }\n\
         \x20   fn derive(&self, ctx: &mut Ctx, out: &mut RowSink) -> Result<()> {\n\
         \x20       let mut stmt = self.db.prepare(\"SELECT sym_sid FROM _call_def\")?;\n\
         \x20       Ok(())\n\
         \x20   }\n\
         }\n",
    )
    .unwrap();

    let (code, out, err) = check(&dir);
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(code, 2, "rail fails --check on raw SQL in a planted op file: out={out} err={err}");
    let text = format!("{out}{err}");
    assert!(text.contains("op-raw-sql"), "reports the ban code: {text}");
    assert!(text.contains("call_dirty.rs"), "names the offending file: {text}");
    assert!(text.contains("Ctx::scan"), "points at the sanctioned read helper: {text}");
}

/// `mod.rs` and `router.rs` are the framework (Ctx::scan itself lives in
/// mod.rs) and must stay OUT of scope even though they contain the exact
/// raw-SQL shapes the rail bans elsewhere.
#[test]
fn rail_excludes_framework_files() {
    let dir = sandbox("framework");
    fs::write(
        dir.join("src/engine/family/mod.rs"),
        "pub(crate) fn scan(rel: &str) { let sql = format!(\"SELECT * FROM {rel}\"); }\n",
    )
    .unwrap();
    fs::write(
        dir.join("src/engine/family/router.rs"),
        "fn replay(db: &Db) { let _ = db.prepare(\"SELECT 1\"); }\n",
    )
    .unwrap();

    let (code, out, err) = check(&dir);
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(code, 0, "framework files (mod.rs/router.rs) are out of scope: out={out} err={err}");
}

/// A raw SQL call inside a family file's OWN `#[cfg(test)] mod tests { .. }`
/// block (the mod.rs precedent: fixtures wire up a real engine with raw
/// `execute`/`prepare`) is a test fixture, not the authoring surface, and
/// must NOT trip the rail.
#[test]
fn rail_exempts_own_test_module() {
    let dir = sandbox("testmod");
    fs::write(
        dir.join("src/engine/family/call_with_tests.rs"),
        "use super::*;\n\
         pub(crate) struct CallWithTests;\n\
         impl Family for CallWithTests {\n\
         \x20   fn name(&self) -> &'static str { \"call_with_tests\" }\n\
         \x20   fn out_cols(&self) -> &'static [&'static str] { &[\"a\"] }\n\
         \x20   fn input_rels(&self) -> &'static [&'static str] { &[\"_call_def\"] }\n\
         \x20   fn derive(&self, ctx: &mut Ctx, out: &mut RowSink) -> Result<()> {\n\
         \x20       let defs = ctx.scan(\"_call_def\", \"sym_sid\", &[\"name_sid\"])?;\n\
         \x20       Ok(())\n\
         \x20   }\n\
         }\n\
         \n\
         #[cfg(test)]\n\
         mod tests {\n\
         \x20   use super::*;\n\
         \x20   fn fixture() {\n\
         \x20       let sql = \"SELECT sym_sid FROM _call_def\";\n\
         \x20       let mut stmt = db.prepare(&sql).unwrap();\n\
         \x20       let _ = rusqlite::params![1];\n\
         \x20   }\n\
         }\n",
    )
    .unwrap();

    let (code, out, err) = check(&dir);
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(code, 0, "raw SQL inside the file's own #[cfg(test)] mod tests is exempt: out={out} err={err}");
    let text = format!("{out}{err}");
    assert!(!text.contains("op-raw-sql"), "no finding should fire inside the test module: {text}");
}
