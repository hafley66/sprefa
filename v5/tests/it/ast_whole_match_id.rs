//! christmas #9 for the `ast` op: the optional trailing `id` arg binds the
//! located spine id of the WHOLE ast match (the captures' min..max byte range),
//! so a rule joins `ref(id, string, file, lo, hi)` to recover the matched byte
//! span. Mirrors the `match` op's 5th-arg `id` behavior: the id IS a
//! `_where_bytes` id, joinable to `ref`. The whole-match source text is ALSO
//! interned into `_strings` under the same StringId the id hashes, so the id
//! round-trips through `string(id, text, norm)` too (see the string-resolve
//! test below).

use std::fs;
use std::path::PathBuf;
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ast_whole_match_id_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    dir
}

fn run_json(dir: &PathBuf, prog: &str) -> Vec<serde_json::Value> {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--root", dir.to_str().unwrap(), "--db", dir.join("db").to_str().unwrap(), "--query-json"])
        .output().expect("run dl");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("bad json line `{l}`: {e}")))
        .collect()
}

#[test]
fn ast_whole_match_id_resolves_through_ref_spine() {
    let d = sandbox("resolves");
    // One function; the `@fn` capture is the whole function_item, so the
    // whole-match span (min lo .. max hi over @name + @fn) is the function body
    // text `fn alpha() {}`.
    fs::write(d.join("src/a.rs"), "fn alpha() {}\n").unwrap();

    // `id` = the 7th `ast` arg (line=`_l`, end=`_e`, then id). The id is a
    // `_where_bytes` id, so `ref(Id, _, _, Lo, Hi)` recovers the matched span.
    let prog = r#"
rel hit(name: text, id: text).
rel located(name: text, lo: int, hi: int).
hit(name, id) <- scan("WORK","src/**/*.rs",path,rev),
  ast(path, rev, :rust, "(function_item name: (identifier) @name) @fn", _l, _e, id).
located(name, lo, hi) <- hit(name, id), ref(id, _s, _f, lo, hi).
? located(name, lo, hi).
"#;
    let recs = run_json(&d, prog);
    assert_eq!(recs.len(), 1, "one query -> one json line: {recs:?}");
    let q = &recs[0];
    assert_eq!(q["query"], "located");
    assert_eq!(q["count"], 1, "exactly one located whole-match: {q}");

    let row = &q["rows"][0];
    assert_eq!(row[0], "alpha", "name col: {row}");
    let lo = row[1].as_i64().expect("lo is int");
    let hi = row[2].as_i64().expect("hi is int");
    // `fn alpha() {}` is bytes 0..13 of the file (the whole function_item span,
    // which is the captures' min lo .. max hi over @name + @fn).
    assert_eq!(lo, 0, "whole-match starts at byte 0: {row}");
    assert_eq!(hi, 13, "whole-match ends at byte 13: {row}");
}

/// Step 1: the string-intern gap is closed. A located id (from BOTH `match`'s
/// 5th-arg id AND `ast`'s 7th-arg id) now resolves to the EXACT source bytes the
/// span covers via `ref(id, sid, ..)` -> `string(sid, text, ..)`. Before the
/// fix the `ref`'s StringId had NO `_strings` row, so `string(sid, ..)` was
/// empty even though `ref` resolved.
#[test]
fn match_and_ast_ids_resolve_through_string() {
    let d = sandbox("string_resolve");
    fs::write(d.join("src/a.rs"), "fn alpha() {}\n").unwrap();

    // `ast` id -> the whole function_item text `fn alpha() {}`.
    // `match` id -> the regex group-0 whole-match text `fn alpha`.
    // Source rules can't join a built-in query rel (`string`), so each op's id
    // is captured into a source rel first, then a derived rule joins `string`.
    let prog = r#"
rel ast_hit(id: text).
rel match_hit(id: text).
rel ast_text(t: text).
rel match_text(t: text).
ast_hit(id) <- scan("WORK","src/**/*.rs",path,rev),
  ast(path, rev, :rust, "(function_item name: (identifier) @name) @fn", _l, _e, id).
match_hit(mid) <- scan("WORK","src/**/*.rs",path,rev),
  match(path, rev, /fn [a-z]+/, _ml, mid).
ast_text(t) <- ast_hit(id), ref(id, sid, _f, _lo, _hi), string(sid, t, _n).
match_text(t) <- match_hit(mid), ref(mid, msid, _mf, _mlo, _mhi), string(msid, t, _mn).
? ast_text(t).
? match_text(t).
"#;
    let recs = run_json(&d, prog);
    assert_eq!(recs.len(), 2, "two queries -> two json lines: {recs:?}");

    let ast_q = recs.iter().find(|r| r["query"] == "ast_text").expect("ast_text query");
    assert_eq!(ast_q["count"], 1, "one ast whole-match text: {ast_q}");
    assert_eq!(ast_q["rows"][0][0], "fn alpha() {}",
        "ast id resolves through string() to the whole function_item text: {ast_q}");

    let match_q = recs.iter().find(|r| r["query"] == "match_text").expect("match_text query");
    assert_eq!(match_q["count"], 1, "one match whole-match text: {match_q}");
    assert_eq!(match_q["rows"][0][0], "fn alpha",
        "match id resolves through string() to the regex whole-match text: {match_q}");
}
