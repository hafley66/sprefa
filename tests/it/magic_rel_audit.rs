//! The magic-rel ban rail (`.dl/magic-rel-audit.dl`) must FIRE on an
//! unregistered literal rel-name lookup and stay green on a catalogued one.
//! Without this, a broken rail (regex rot, deleted catalog read) would pass
//! CI's bare `dl --check` silently — no diags looks like no findings. Here we
//! plant a `.rs` file under a temp root and run the SHIPPED rail against it.
//!
//! The demand/overlay sinks (scip_want / rev_cmp_want / def_target / effect_cmd)
//! are catalogued builtins (group `demand`, src/engine/mod.rs `demand_rel_decls`),
//! so reading them by literal name is allowed. See
//! plans/2026-07-06-magic-rel-audit.md.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

/// The repo's actual rail file — the artifact under test, not a copy.
fn rail() -> String {
    format!("{}/.dl/magic-rel-audit.dl", env!("CARGO_MANIFEST_DIR"))
}

fn sandbox(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("dl_magicrel_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(p.join("src")).unwrap();
    p
}

/// Run the shipped rail with `--check` over a temp root that has one planted
/// source file. Returns (exit code, stdout, stderr).
fn check(dir: &PathBuf, rs_body: &str) -> (i32, String, String) {
    fs::write(dir.join("src/planted.rs"), rs_body).unwrap();
    let out = Command::new(DL)
        .args([&rail(), "--root", dir.to_str().unwrap(), "--no-daemon", "--check"])
        .output()
        .expect("run dl --check on the magic-rel rail");
    (out.status.code().unwrap_or(-1),
     String::from_utf8_lossy(&out.stdout).into_owned(),
     String::from_utf8_lossy(&out.stderr).into_owned())
}

#[test]
fn rail_flags_unregistered_literal_name() {
    let d = sandbox("dirty");
    // A `rels.get("...")` for a name that is neither catalogued nor in
    // special_rels() — exactly the banned pattern.
    let (code, out, err) = check(&d,
        "fn probe(eng: &Engine) { let _ = eng.rels.get(\"totally_unregistered_rel\"); }\n");
    assert_eq!(code, 2, "rail fails --check on an unregistered magic name: out={out} err={err}");
    let text = format!("{out}{err}");
    assert!(text.contains("magic-rel-unregistered"),
        "reports the ban code: {text}");
    assert!(text.contains("totally_unregistered_rel"),
        "names the offending rel: {text}");
}

#[test]
fn rail_green_on_registered_name() {
    let d = sandbox("clean");
    // `scip_want` is a catalogued builtin (group `demand`); reading it by literal
    // name is allowed and must not trip the rail.
    let (code, _out, err) = check(&d,
        "fn probe(eng: &Engine) { let _ = eng.rels.get(\"scip_want\"); }\n");
    assert_eq!(code, 0, "rail is green on a catalogued demand sink: {err}");
}

#[test]
fn rail_green_on_catalogued_builtin() {
    let d = sandbox("builtin");
    // `diag` is a catalogued builtin; `FROM rel_repo` names another. Both are
    // known, so a literal read of either must not trip the rail.
    let (code, _out, err) = check(&d,
        "fn probe(eng: &Engine) {\n\
         let _ = eng.rels.get(\"diag\");\n\
         let _ = eng.query_sql(\"SELECT slug FROM rel_repo\", &[]);\n\
         }\n");
    assert_eq!(code, 0, "rail is green on catalogued builtins: {err}");
}
