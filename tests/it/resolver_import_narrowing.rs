//! Win D: import-scoped ambiguity narrowing in the extraction-time resolver
//! (`narrow_ambiguous` in `src/engine/extract.rs`, wired into both
//! `refresh_type_rels`'s `resolve` and `refresh_call_rels`'s `resolve_callee`).
//!
//! Setup: two files declare a colliding `Config` struct + `make` fn (pkg_a and
//! pkg_b). A third file (`holder_one`) imports ONLY pkg_a's declarations and
//! references both: the referencing file's own `module_edge_rev` import
//! narrows the otherwise-ambiguous (len > 1) def bucket down to a single
//! survivor. A fourth file (`holder_both`) imports BOTH pkg_a's and pkg_b's
//! declarations; narrowing still finds two survivors, so it correctly stays
//! bare rather than guessing.
//!
//! The fixture keeps the colliding defs and the referencing files in
//! DIFFERENT directories (`src/pkg_a`, `src/pkg_b`, `src/app`) on purpose:
//! `narrow_ambiguous`'s same-directory criterion is a real filter, and if the
//! referencing file shared a directory with BOTH candidates, that criterion
//! alone would make both survive (correctly staying ambiguous) and mask the
//! import-based narrowing this test is actually proving.
//!
//! This also caught a real bug during development: the module family
//! (`ModuleFamily`) only used to run when the PROGRAM referenced a
//! `module_*` relation directly (`module_rels_used`). A program reading only
//! `type_link`/`call_edge` never populated `module_edge_rev`, so the
//! narrowing silently no-op'd (every ambiguous name stayed bare forever).
//! Fixed by `module_rels_needed` (`src/engine/mod.rs`), which ORs in
//! type/call/doc-family usage so the module graph is always fresh when a
//! resolver that reads it runs. This test's first assertion is also a
//! regression guard for that: the query below references type_link/call_edge
//! only, never module_edge.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("resolver_import_narrowing_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Empty config, set via `$SPREFA_CONFIG` so the test is hermetic regardless
/// of the developer machine's real `~/.config/sprefa/config.toml` (which, on
/// a machine with configured repos, would otherwise register EXTRA repos:
/// harmless to resolution here since the resolver is repo-scoped, but noisy
/// in `--profile`/stderr and worth ruling out explicitly).
fn empty_config(dir: &Path) -> PathBuf {
    let cfg = dir.join("empty.toml");
    fs::write(&cfg, "# no repos\n").unwrap();
    cfg
}

// `revision_marker` is an extra trailing comment on `holder_one.rs`, distinct
// per call, so a re-tick's rewrite is NEVER byte-length-identical to the
// prior one. Without it, retargeting the import from "pkg_a" to "pkg_b"
// (equal-length names) can produce a byte-IDENTICAL-LENGTH file whose mtime
// lands in the same filesystem timestamp tick as the previous write under
// heavy parallel load. `enumerate_with_hash`'s mtime+size fast path
// (src/engine/mod.rs) then reuses the STALE stored hash instead of re-
// reading, and the whole point of this test (a content change must
// invalidate the resolver) never gets exercised. This is a real, pre-
// existing edge in that fast path, not a Win D defect; sidestepping it here
// keeps the test aimed at what it is actually proving (the module_edge_rev
// digest fold), not a coincidental mtime collision.
fn write_fixture(dir: &Path, holder_one_target: &str, revision_marker: &str) {
    fs::create_dir_all(dir.join("src/pkg_a")).unwrap();
    fs::create_dir_all(dir.join("src/pkg_b")).unwrap();
    fs::create_dir_all(dir.join("src/app")).unwrap();
    fs::write(dir.join("src/lib.rs"), "mod pkg_a;\nmod pkg_b;\nmod app;\n").unwrap();
    fs::write(dir.join("src/pkg_a.rs"), "pub mod config;\n").unwrap();
    fs::write(
        dir.join("src/pkg_a/config.rs"),
        "pub struct Config { pub n: i64 }\npub fn make() -> i64 { 1 }\n",
    )
    .unwrap();
    fs::write(dir.join("src/pkg_b.rs"), "pub mod config;\n").unwrap();
    fs::write(
        dir.join("src/pkg_b/config.rs"),
        "pub struct Config { pub n: i64 }\npub fn make() -> i64 { 2 }\n",
    )
    .unwrap();
    fs::write(dir.join("src/app.rs"), "pub mod holder_one;\npub mod holder_both;\n").unwrap();
    fs::write(
        dir.join("src/app/holder_one.rs"),
        format!(
            "use crate::{holder_one_target}::config::Config;\n\
             use crate::{holder_one_target}::config::make;\n\
             pub struct Holder {{ pub c: Config }}\n\
             pub fn build() -> i64 {{ make() }}\n\
             // {revision_marker}\n"
        ),
    )
    .unwrap();
    fs::write(
        dir.join("src/app/holder_both.rs"),
        "use crate::pkg_a::config::Config;\n\
         use crate::pkg_b::config::Config;\n\
         use crate::pkg_a::config::make;\n\
         use crate::pkg_b::config::make;\n\
         pub struct HolderBoth { pub c: Config }\n\
         pub fn build_both() -> i64 { make() }\n",
    )
    .unwrap();
}

const PROG: &str = r#"
rel seen(path: file).
seen(path) <- scan("WORK", "src/**/*.rs", path, rev), match(path, rev, /./, line).
? type_link(src, dst, kind).
? call_edge(caller, callee, kind).
"#;

fn run(dir: &Path, cfg: &Path, db: &Path) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), PROG).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--no-daemon", "--db", db.to_str().unwrap()])
        .current_dir(dir)
        .env("SPREFA_CONFIG", cfg)
        .output()
        .expect("run dl");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn unique_import_narrows_ambiguity_both_ambiguous_stays_bare() {
    let d = sandbox("gate");
    let cfg = empty_config(&d);
    write_fixture(&d, "pkg_a", "rev-1");

    let (code, out, err) = run(&d, &cfg, &d.join("db"));
    assert_eq!(code, 0, "run failed:\nstdout={out}\nstderr={err}");

    let links = out.split("? type_link").nth(1).unwrap_or("").split("? call_edge").next().unwrap_or("");
    let calls = out.split("? call_edge").nth(1).unwrap_or("");

    // holder_one imports ONLY pkg_a's Config/make: both resolvers narrow to
    // the unique survivor (import-based, a "strong" reason).
    assert!(
        links.lines().any(|l| l.contains("holder_one.rs::struct::Holder")
            && l.contains("pkg_a/config.rs::struct::Config")),
        "type_link narrows to the imported pkg_a::Config:\n{links}"
    );
    assert!(
        calls.lines().any(|l| l.contains("holder_one.rs::function::build")
            && l.contains("pkg_a/config.rs::function::make")),
        "call_edge narrows to the imported pkg_a::make:\n{calls}"
    );

    // holder_both imports BOTH pkg_a's and pkg_b's Config/make: narrowing
    // still finds two survivors, so type_link falls back to the bare name
    // (never a guess) and call_edge emits no row at all (unresolved calls
    // never appear in call_edge, only in call_site).
    assert!(
        links.lines().any(|l| l.contains("holder_both.rs::struct::HolderBoth") && l.ends_with("\tConfig\tfield")),
        "type_link stays bare for the both-imports case:\n{links}"
    );
    assert!(
        !calls.contains("holder_both"),
        "call_edge must not resolve build_both's ambiguous make() call:\n{calls}"
    );
}

#[test]
fn editing_an_import_flips_the_resolution_on_a_retick() {
    // Same fixture, but this test proves the resolution is not stuck warm:
    // editing holder_one's import (which file's Config/make it names) and
    // re-ticking against the SAME db must flip which def type_link/
    // call_edge resolve to. `extract_input_digest`'s module_edge_rev fold
    // (Win D point 3) is what invalidates the family's warm-tick skip here.
    let d = sandbox("flip");
    let cfg = empty_config(&d);
    let db = d.join("db");
    write_fixture(&d, "pkg_a", "rev-1");

    let (code1, out1, err1) = run(&d, &cfg, &db);
    assert_eq!(code1, 0, "first run failed:\nstdout={out1}\nstderr={err1}");
    assert!(
        out1.contains("holder_one.rs::struct::Holder") && {
            let links = out1.split("? type_link").nth(1).unwrap_or("");
            links.lines().any(|l| l.contains("holder_one.rs::struct::Holder") && l.contains("pkg_a/config.rs::struct::Config"))
        },
        "first tick resolves to pkg_a:\n{out1}"
    );

    // Retarget holder_one's imports to pkg_b, re-tick the SAME db. The
    // marker string's length differs from "rev-1" on purpose (see
    // `write_fixture`'s doc) so this edit can never collide in size with the
    // prior write.
    write_fixture(&d, "pkg_b", "rev-2-after-the-import-edit");
    let (code2, out2, err2) = run(&d, &cfg, &db);
    assert_eq!(code2, 0, "second run failed:\nstdout={out2}\nstderr={err2}");

    let links2 = out2.split("? type_link").nth(1).unwrap_or("").split("? call_edge").next().unwrap_or("");
    let calls2 = out2.split("? call_edge").nth(1).unwrap_or("");
    assert!(
        links2.lines().any(|l| l.contains("holder_one.rs::struct::Holder") && l.contains("pkg_b/config.rs::struct::Config")),
        "re-tick flips type_link to pkg_b after the import edit:\n{links2}"
    );
    assert!(
        calls2.lines().any(|l| l.contains("holder_one.rs::function::build") && l.contains("pkg_b/config.rs::function::make")),
        "re-tick flips call_edge to pkg_b after the import edit:\n{calls2}"
    );
    assert!(
        !links2.lines().any(|l| l.contains("holder_one.rs::struct::Holder") && l.contains("pkg_a/config.rs::struct::Config")),
        "the stale pkg_a resolution must not survive the retick:\n{links2}"
    );
}
