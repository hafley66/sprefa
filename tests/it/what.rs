//! End-to-end coverage of the `dl what <anchor>` and `dl summary <path>` verbs
//! (src/anchor.rs + src/cli/query.rs). Runs the real binary with `--no-daemon`
//! so every answer comes from the in-process family-forcing probe, not a live
//! daemon. Fixture: a two-file TS project (a model + a repo that imports it) so
//! the type, call, module, and comment families all populate.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("what_verb_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir.join("src")).unwrap();
    // model.ts: a type + a function the repo calls.
    fs::write(
        dir.join("src/model.ts"),
        "export interface Entity { id: Id }\n\
         export function lookup(): Entity { return build() }\n",
    )
    .unwrap();
    // repo.ts: imports the model, a class whose method calls lookup(). Line
    // numbers are load-bearing for the path:line test below.
    //   1 import { Entity, lookup } from './model'
    //   2 // find one entity
    //   3 export class Repo {
    //   4     find(): Entity {
    //   5         return lookup()
    //   6     }
    //   7 }
    fs::write(
        dir.join("src/repo.ts"),
        "import { Entity, lookup } from './model'\n\
         // find one entity\n\
         export class Repo {\n\
         \x20   find(): Entity {\n\
         \x20       return lookup()\n\
         \x20   }\n\
         }\n",
    )
    .unwrap();
    dir
}

/// Run a `dl` verb (subcommand) in `dir`, forcing the in-process path.
fn run_verb(dir: &Path, args: &[&str]) -> (i32, String, String) {
    let out = Command::new(DL)
        .args(args)
        .arg("--no-daemon")
        .current_dir(dir)
        .output()
        .expect("run dl verb");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn what_name_hits_two_families() {
    // `lookup` is BOTH a type_entity (kind function) and a call_def, so one
    // anchor resolves in two families.
    let d = sandbox("two_fam");
    let (code, out, err) = run_verb(&d, &["what", "lookup"]);
    assert_eq!(code, 0, "what failed:\nstdout={out}\nstderr={err}");
    assert!(out.contains("type_entity"), "type family hit missing: {out}");
    assert!(out.contains("call_def"), "call family hit missing: {out}");
    // A name anchor with no `*` reports match_kind = exact.
    assert!(out.contains("exact"), "match_kind exact missing: {out}");
}

#[test]
fn what_glob_reports_glob_kind() {
    let d = sandbox("glob");
    let (code, out, err) = run_verb(&d, &["what", "Enti*"]);
    assert_eq!(code, 0, "what failed:\nstdout={out}\nstderr={err}");
    assert!(out.contains("Entity"), "glob should match Entity: {out}");
    assert!(out.contains("glob"), "match_kind glob missing: {out}");
}

#[test]
fn what_path_line_lists_call_site_and_enclosing_def() {
    let d = sandbox("pathline");
    let (code, out, err) = run_verb(&d, &["what", "src/repo.ts:5"]);
    assert_eq!(code, 0, "what failed:\nstdout={out}\nstderr={err}");
    assert!(out.contains("call_site"), "call_site row missing: {out}");
    assert!(out.contains("callee=lookup"), "callee detail missing: {out}");
    assert!(out.contains("enclosing_def"), "enclosing def missing: {out}");
}

#[test]
fn summary_is_non_empty() {
    let d = sandbox("summary");
    let (code, out, err) = run_verb(&d, &["summary", "src/repo.ts"]);
    assert_eq!(code, 0, "summary failed:\nstdout={out}\nstderr={err}");
    assert!(out.contains("entity"), "entity section missing: {out}");
    assert!(out.contains("Repo"), "Repo entity missing: {out}");
    assert!(out.contains("doc_coverage"), "doc_coverage metric missing: {out}");
    assert!(out.contains("comment_nodes"), "comment_nodes metric missing: {out}");
}

#[test]
fn summary_reports_imports() {
    // repo.ts imports model.ts; model.ts is imported by repo.ts.
    let d = sandbox("imports");
    let (_c, out_repo, _e) = run_verb(&d, &["summary", "src/repo.ts"]);
    assert!(out_repo.contains("import_out"), "expected an outbound import: {out_repo}");
    let (_c, out_model, _e) = run_verb(&d, &["summary", "src/model.ts"]);
    assert!(out_model.contains("import_in"), "expected an inbound import: {out_model}");
}

#[test]
fn what_paging_limit_offset() {
    // `*` matches every name; paging must cap the rows and report the total.
    let d = sandbox("paging");
    let (code, page0, err) = run_verb(&d, &["what", "*", "--limit", "1", "--offset", "0"]);
    assert_eq!(code, 0, "what failed:\nstdout={page0}\nstderr={err}");
    assert!(page0.contains("(1 rows)"), "limit 1 should print one row: {page0}");
    assert!(page0.contains("showing 0-1 of"), "paging footer missing: {page0}");
    // The total must exceed the shown page (the fixture has several names).
    assert!(
        !page0.contains("of 1 total"),
        "expected more than one total hit: {page0}"
    );

    let (_c, page1, _e) = run_verb(&d, &["what", "*", "--limit", "1", "--offset", "1"]);
    assert!(page1.contains("showing 1-2 of"), "offset footer missing: {page1}");
    // Page 1 must differ from page 0 (offset advanced the window).
    let row0 = data_row(&page0);
    let row1 = data_row(&page1);
    assert_ne!(row0, row1, "offset did not advance the page:\np0={page0}\np1={page1}");
}

/// The first data row (the line after the tab-separated header), for comparing
/// two pages of a paged `what` result.
fn data_row(out: &str) -> String {
    out.lines().nth(1).unwrap_or("").to_string()
}
