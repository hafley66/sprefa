//! The `gen` codegen sink. File form: `gen("path", "tmpl {x}") <- body.` renders
//! body rows (deterministic order) into a file, grouping by rendered path.
//! Splice form: `gen(p, l0, l1, "tmpl {x}") <- body.` replaces the lines strictly
//! between two marker lines (the `comment` op's paired coordinates). Both skip
//! the write when bytes already match, so a converged tick is a no-op.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gen_op_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(dir: &Path, prog: &str) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--root", dir.to_str().unwrap(), "--db", dir.join("db").to_str().unwrap()])
        .output().expect("run dl");
    (out.status.code().unwrap_or(-1),
     String::from_utf8_lossy(&out.stdout).into_owned(),
     String::from_utf8_lossy(&out.stderr).into_owned())
}

#[test]
fn file_form_renders_rows_in_deterministic_order() {
    let d = sandbox("file");
    let (code, _, err) = run(&d, concat!(
        "rel edge(a: text, n: int).\n",
        "edge(\"beta\", 1).\n",
        "edge(\"alpha\", 3).\n",
        "gen(\"out/report.md\", \"| {a} | {n} |\") <- edge(a, n).\n"));
    assert_eq!(code, 0, "{err}");
    let got = fs::read_to_string(d.join("out/report.md")).unwrap();
    assert_eq!(got, "| alpha | 3 |\n| beta | 1 |\n");
}

#[test]
fn path_template_groups_rows_per_file() {
    let d = sandbox("group");
    let (code, _, err) = run(&d, concat!(
        "rel kv(f: text, v: text).\n",
        "kv(\"x\", \"1\").\n",
        "kv(\"y\", \"2\").\n",
        "gen(\"out/{f}.txt\", \"{v}\") <- kv(f, v).\n"));
    assert_eq!(code, 0, "{err}");
    assert_eq!(fs::read_to_string(d.join("out/x.txt")).unwrap(), "1\n");
    assert_eq!(fs::read_to_string(d.join("out/y.txt")).unwrap(), "2\n");
}

/// A converged gen must not touch the file. Proof without sleeping: make the
/// generated file read-only; the second run succeeds only if it skips the write.
#[test]
fn matching_bytes_skip_the_write() {
    let d = sandbox("idem");
    let prog = concat!(
        "rel edge(a: text).\n",
        "edge(\"only\").\n",
        "gen(\"out/r.txt\", \"{a}\") <- edge(a).\n");
    let (code, _, err) = run(&d, prog);
    assert_eq!(code, 0, "{err}");
    let target = d.join("out/r.txt");
    let mut perms = fs::metadata(&target).unwrap().permissions();
    perms.set_readonly(true);
    fs::set_permissions(&target, perms).unwrap();
    let (code, _, err) = run(&d, prog);
    assert_eq!(code, 0, "second run must skip the write entirely: {err}");
}

/// Two separate gen File rules rendering the same path is the v0 last-wins
/// trap: file emits don't concatenate across rules, so the engine bails loudly
/// (mirrors the mixed-source/derived bail) instead of silently dropping the
/// first rule's content. The fix is to union the rows into one relation.
#[test]
fn two_file_rules_same_path_bail_loudly() {
    let d = sandbox("dup_path");
    let prog = concat!(
        "rel xs(x: text).\n",
        "xs(\"a\").\n",
        "rel ys(y: text).\n",
        "ys(\"b\").\n",
        "gen(\"out/r.txt\", \"x {x}\") <- xs(x).\n",
        "gen(\"out/r.txt\", \"y {y}\") <- ys(y).\n");
    let (code, _, err) = run(&d, prog);
    assert_eq!(code, 1, "duplicate file-emit path must be a loud error: {err}");
    assert!(err.contains("two gen rules write the same file"),
        "error must name the collision: {err}");
}

/// Disjoint paths from two gen rules stay legal — the claim is per rendered
/// path, not per rule.
#[test]
fn two_file_rules_disjoint_paths_ok() {
    let d = sandbox("disjoint_path");
    let prog = concat!(
        "rel xs(x: text).\n",
        "xs(\"a\").\n",
        "rel ys(y: text).\n",
        "ys(\"b\").\n",
        "gen(\"out/x.txt\", \"{x}\") <- xs(x).\n",
        "gen(\"out/y.txt\", \"{y}\") <- ys(y).\n");
    let (code, _, err) = run(&d, prog);
    assert_eq!(code, 0, "{err}");
    assert_eq!(fs::read_to_string(d.join("out/x.txt")).unwrap(), "a\n");
    assert_eq!(fs::read_to_string(d.join("out/y.txt")).unwrap(), "b\n");
}

#[test]
fn escaping_path_is_rejected() {
    let d = sandbox("escape");
    fs::create_dir_all(d.join("inner")).unwrap();
    let prog = concat!(
        "rel edge(a: text).\n",
        "edge(\"x\").\n",
        "gen(\"../oops.txt\", \"{a}\") <- edge(a).\n");
    fs::write(d.join("inner/p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(d.join("inner/p.dl"))
        .args(["--root", d.join("inner").to_str().unwrap(),
               "--db", d.join("db").to_str().unwrap()])
        .output().expect("run dl");
    assert_eq!(out.status.code().unwrap_or(-1), 1, "path escape must be a loud error");
    assert!(!d.join("oops.txt").exists());
}

/// Two gen rules splicing two regions of ONE file must batch into a single
/// write: the line coordinates come from the pre-tick content, so a second
/// rule re-reading a file the first rule already grew would land stale.
#[test]
fn two_gen_rules_splice_one_file_in_one_write() {
    let d = sandbox("two_rules");
    fs::write(d.join("doc.md"),
        "<!-- BEGIN: a -->\nstale\n<!-- END: -->\n<!-- BEGIN: b -->\nstale\n<!-- END: -->\n").unwrap();
    let prog = concat!(
        "rel block(p: file, l0: int, l1: int, name: text).\n",
        "block(p, l0, l1, name) <- scan(\"WORK\", \"*.md\", p, rev), ",
        "comment(p, rev, /BEGIN: $name -->/, /END:/, l0, l1, name).\n",
        "rel xs(x: text).\n",
        "xs(\"x1\").\nxs(\"x2\").\n",
        "rel ys(y: text).\n",
        "ys(\"y1\").\n",
        "gen(p, l0, l1, \"- {x}\") <- block(p, l0, l1, \"a\"), xs(x).\n",
        "gen(p, l0, l1, \"* {y}\") <- block(p, l0, l1, \"b\"), ys(y).\n");
    let (code, _, err) = run(&d, prog);
    assert_eq!(code, 0, "{err}");
    let got = fs::read_to_string(d.join("doc.md")).unwrap();
    assert_eq!(got,
        "<!-- BEGIN: a -->\n- x1\n- x2\n<!-- END: -->\n<!-- BEGIN: b -->\n* y1\n<!-- END: -->\n");

    let target = d.join("doc.md");
    let mut perms = fs::metadata(&target).unwrap().permissions();
    perms.set_readonly(true);
    fs::set_permissions(&target, perms).unwrap();
    let (code, _, err) = run(&d, prog);
    assert_eq!(code, 0, "warm two-rule splice must converge without writing: {err}");
}

/// The marker-splice loop: comment regions give the coordinates, gen rewrites
/// the lines between the markers, the markers and surrounding text survive,
/// and a second warm run converges (no write).
#[test]
fn splice_form_rewrites_between_markers_and_converges() {
    let d = sandbox("splice");
    fs::write(d.join("README.md"),
        "# Title\n<!-- BEGIN: items -->\nstale line\n<!-- END: -->\ntail\n").unwrap();
    let prog = concat!(
        "rel block(p: file, l0: int, l1: int, name: text).\n",
        "block(p, l0, l1, name) <- scan(\"WORK\", \"*.md\", p, rev), ",
        "comment(p, rev, /BEGIN: $name -->/, /END:/, l0, l1, name).\n",
        "rel item(x: text).\n",
        "item(\"alpha\").\n",
        "item(\"beta\").\n",
        "gen(p, l0, l1, \"- {x}\") <- block(p, l0, l1, \"items\"), item(x).\n");
    let (code, _, err) = run(&d, prog);
    assert_eq!(code, 0, "{err}");
    let got = fs::read_to_string(d.join("README.md")).unwrap();
    assert_eq!(got,
        "# Title\n<!-- BEGIN: items -->\n- alpha\n- beta\n<!-- END: -->\ntail\n");

    // Converged: the regenerated content matches, so the file is untouched.
    let target = d.join("README.md");
    let mut perms = fs::metadata(&target).unwrap().permissions();
    perms.set_readonly(true);
    fs::set_permissions(&target, perms).unwrap();
    let (code, _, err) = run(&d, prog);
    assert_eq!(code, 0, "warm splice must converge without writing: {err}");
}

// ─── byte-cursor mode (gen(:mode, p, lo, hi, ...)) ──────────────────────
// Port of v4's `write_cursor`. The four modes (replace/append/prepend/wrap)
// splice at byte offsets [lo, hi) sourced from the ref spine. The gate tests
// below drive the cursor from a literal fact with known offsets so they prove
// the mode dispatch and right-to-left batching in isolation; a follow-up test
// wires the spine (match -> ref) once match emits an id column.

/// `:replace` splices the rendered rows into [lo, hi). One row at [1, 4) of
/// "ABCDE" yields "AXYZE". That's the mode that line-splice already does at
/// line granularity; proving it at byte granularity is the floor.
#[test]
fn cursor_replace_splices_into_byte_window() {
    let d = sandbox("cursor_replace");
    fs::write(d.join("data.txt"), "ABCDE").unwrap();
    let prog = concat!(
        "rel hit(p: file, lo: int, hi: int).\n",
        "hit(\"data.txt\", 1, 4).\n",
        "gen(:replace, p, lo, hi, \"XYZ\") <- hit(p, lo, hi).\n",
    );
    let (code, _, err) = run(&d, prog);
    assert_eq!(code, 0, "{err}");
    let got = fs::read_to_string(d.join("data.txt")).unwrap();
    assert_eq!(got, "AXYZE");
}

/// `:append` inserts the payload at `hi` (leaving [lo, hi) intact). On
/// "ABCDE" with [0, 3) -> "ABC", append at hi=3 yields "ABC<<A>>DEF".
#[test]
fn cursor_append_inserts_at_hi() {
    let d = sandbox("cursor_append");
    fs::write(d.join("data.txt"), "ABCDEF").unwrap();
    let prog = concat!(
        "rel hit(p: file, lo: int, hi: int).\n",
        "hit(\"data.txt\", 0, 3).\n",
        "gen(:append, p, lo, hi, \"<<A>>\") <- hit(p, lo, hi).\n",
    );
    let (code, _, err) = run(&d, prog);
    assert_eq!(code, 0, "{err}");
    let got = fs::read_to_string(d.join("data.txt")).unwrap();
    assert_eq!(got, "ABC<<A>>DEF");
}

/// `:prepend` inserts the payload at `lo` (leaving [lo, hi) intact). On
/// "ABCDEF" with [3, 6) -> "DEF", prepend at lo=3 yields "ABC<<P>>DEF".
#[test]
fn cursor_prepend_inserts_at_lo() {
    let d = sandbox("cursor_prepend");
    fs::write(d.join("data.txt"), "ABCDEF").unwrap();
    let prog = concat!(
        "rel hit(p: file, lo: int, hi: int).\n",
        "hit(\"data.txt\", 3, 6).\n",
        "gen(:prepend, p, lo, hi, \"<<P>>\") <- hit(p, lo, hi).\n",
    );
    let (code, _, err) = run(&d, prog);
    assert_eq!(code, 0, "{err}");
    let got = fs::read_to_string(d.join("data.txt")).unwrap();
    assert_eq!(got, "ABC<<P>>DEF");
}

/// `:wrap` inserts the payload at BOTH lo and hi, surrounding [lo, hi). On
/// "ABCDEF" with [1, 5), wrap yields "A<W>BCDE<W>F". This is the mode that
/// line-splice cannot express.
#[test]
fn cursor_wrap_surrounds_byte_window() {
    let d = sandbox("cursor_wrap");
    fs::write(d.join("data.txt"), "ABCDEF").unwrap();
    let prog = concat!(
        "rel hit(p: file, lo: int, hi: int).\n",
        "hit(\"data.txt\", 1, 5).\n",
        "gen(:wrap, p, lo, hi, \"<W>\") <- hit(p, lo, hi).\n",
    );
    let (code, _, err) = run(&d, prog);
    assert_eq!(code, 0, "{err}");
    let got = fs::read_to_string(d.join("data.txt")).unwrap();
    assert_eq!(got, "A<W>BCDE<W>F");
}

/// Two `:replace` regions in one file must batch into a single write with
/// regions applied right-to-left so the earlier offset stays valid. On
/// "ABCDEFGH" replace [1, 3) and [5, 7): right-to-left does [5, 7) first
/// ("ABCDE__GH") then [1, 3) ("A###DE__GH"). Left-to-right would corrupt the
/// second offset, producing "A###DE^---^H" or worse.
#[test]
fn cursor_two_regions_apply_right_to_left() {
    let d = sandbox("cursor_order");
    fs::write(d.join("data.txt"), "ABCDEFGH").unwrap();
    let prog = concat!(
        "rel hit(p: file, lo: int, hi: int, payload: text).\n",
        "hit(\"data.txt\", 1, 3, \"###\").\n",
        "hit(\"data.txt\", 5, 7, \"__\").\n",
        "gen(:replace, p, lo, hi, \"{payload}\") <- hit(p, lo, hi, payload).\n",
    );
    let (code, _, err) = run(&d, prog);
    assert_eq!(code, 0, "{err}");
    let got = fs::read_to_string(d.join("data.txt")).unwrap();
    assert_eq!(got, "A###DE__H");
}

/// End-to-end: `match`'s optional 5th arg (the whole-match spine id) joins
/// `ref(id, _, _, lo, hi)` to recover the byte offsets, which feed
/// `gen(:replace, ...)` -- no literal offsets in the rule. On "xxTARGETxx" the
/// /TARGET/ match resolves to [2, 8); :replace with "REPLACED" yields
/// "xxREPLACEDxx". This is the match -> byte-cursor loop the cursor port's
/// literal-offset gate tests deferred: the spine now carries the offsets.
#[test]
fn match_id_feeds_cursor_replace_end_to_end() {
    let d = sandbox("match_id");
    fs::write(d.join("data.txt"), "xxTARGETxx").unwrap();
    let prog = concat!(
        "rel hit(p: file, l: int, id: text).\n",
        "hit(p, l, id) <- scan(\"data.txt\", p, rev), match(p, rev, /TARGET/, l, id).\n",
        "gen(:replace, p, lo, hi, \"REPLACED\") <- hit(p, _, id), ref(id, _, _, lo, hi).\n",
    );
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "stderr: {err}\nstdout: {out}");
    let got = fs::read_to_string(d.join("data.txt")).unwrap();
    assert_eq!(got, "xxREPLACEDxx", "match id -> ref lo,hi -> cursor :replace");
}

/// Same loop with :wrap to prove the offsets are the true [lo, hi) of the match
/// (wrap touches both ends; a wrong span would corrupt one side). On "xxTARGETxx"
/// the [2, 8) match wrapped with "<w>" yields "xx<w>TARGET<w>xx".
#[test]
fn match_id_feeds_cursor_wrap_end_to_end() {
    let d = sandbox("match_id_wrap");
    fs::write(d.join("data.txt"), "xxTARGETxx").unwrap();
    let prog = concat!(
        "rel hit(p: file, l: int, id: text).\n",
        "hit(p, l, id) <- scan(\"data.txt\", p, rev), match(p, rev, /TARGET/, l, id).\n",
        "gen(:wrap, p, lo, hi, \"<w>\") <- hit(p, _, id), ref(id, _, _, lo, hi).\n",
    );
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "stderr: {err}\nstdout: {out}");
    let got = fs::read_to_string(d.join("data.txt")).unwrap();
    assert_eq!(got, "xx<w>TARGET<w>xx", "match id -> ref lo,hi -> cursor :wrap");
}

// ─── overlap gate (multi-edit safety) ──────────────────────────────────
// The right-to-left apply assumes every region's window refers to the ORIGINAL
// buffer. Two regions whose windows intersect would see the second operate on
// already-mutated bytes (silent corruption), so both cursor and line-splice
// forms bail loudly. Adjacent (touching) windows are allowed.

/// Two :replace cursor regions whose [lo,hi) windows intersect must bail
/// loudly rather than corrupt. [1,5) and [3,7) share [3,5); the file is left
/// untouched because the gate fires before any write.
#[test]
fn cursor_overlapping_windows_bail_loudly() {
    let d = sandbox("cursor_overlap");
    fs::write(d.join("data.txt"), "ABCDEFGH").unwrap();
    let prog = concat!(
        "rel hit(p: file, lo: int, hi: int, payload: text).\n",
        "hit(\"data.txt\", 1, 5, \"###\").\n",
        "hit(\"data.txt\", 3, 7, \"@@@\").\n",
        "gen(:replace, p, lo, hi, \"{payload}\") <- hit(p, lo, hi, payload).\n",
    );
    let (code, _, err) = run(&d, prog);
    assert_ne!(code, 0, "overlapping cursor regions must bail");
    assert!(err.contains("overlap"), "expected overlap message, got: {err}");
    assert_eq!(fs::read_to_string(d.join("data.txt")).unwrap(), "ABCDEFGH",
        "the overlap gate must fire before any write");
}

/// Adjacent (touching) windows — [1,3) and [3,6), hi == next lo — must NOT
/// trip the gate: they touch but don't overlap. On "ABCDEFGH" replace both,
/// right-to-left: [3,6)->"$$" then [1,3)->"##" yields "A##$$GH".
#[test]
fn cursor_adjacent_windows_do_not_bail() {
    let d = sandbox("cursor_adjacent");
    fs::write(d.join("data.txt"), "ABCDEFGH").unwrap();
    let prog = concat!(
        "rel hit(p: file, lo: int, hi: int, payload: text).\n",
        "hit(\"data.txt\", 1, 3, \"##\").\n",
        "hit(\"data.txt\", 3, 6, \"$$\").\n",
        "gen(:replace, p, lo, hi, \"{payload}\") <- hit(p, lo, hi, payload).\n",
    );
    let (code, _, err) = run(&d, prog);
    assert_eq!(code, 0, "adjacent (touching) windows must not bail: {err}");
    assert_eq!(fs::read_to_string(d.join("data.txt")).unwrap(), "A##$$GH");
}

/// Line-splice form has the same gate. Regions [1,4) and [3,6) share lines, so
/// bail loudly.
#[test]
fn splice_overlapping_windows_bail_loudly() {
    let d = sandbox("splice_overlap");
    fs::write(d.join("data.txt"), "0\n1\n2\n3\n4\n5\n6\n").unwrap();
    let prog = concat!(
        "rel hit(p: file, l0: int, l1: int).\n",
        "hit(\"data.txt\", 1, 4).\n",
        "hit(\"data.txt\", 3, 6).\n",
        "gen(p, l0, l1, \"X\") <- hit(p, l0, l1).\n",
    );
    let (code, _, err) = run(&d, prog);
    assert_ne!(code, 0, "overlapping splice regions must bail");
    assert!(err.contains("overlap"), "expected overlap message, got: {err}");
}
