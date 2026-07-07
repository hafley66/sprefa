//! The `comment` op end to end: `comment(p, rev, /open/[, /close/], l0, l1, label)`
//! binds one row per marker-bounded region. Sequential (one regex) and paired
//! (two regexes) modes; `$NAME` holes work in the marker regexes.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("comment_op_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(dir: &Path, prog: &str) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--db", dir.join("db").to_str().unwrap()])
        .current_dir(dir)
        .output().expect("run dl");
    (out.status.code().unwrap_or(-1),
     String::from_utf8_lossy(&out.stdout).into_owned(),
     String::from_utf8_lossy(&out.stderr).into_owned())
}

#[test]
fn sequential_sections_bind_span_and_label() {
    let d = sandbox("seq");
    fs::write(d.join("deploy.sh"),
        "# SECTION: build\nstep a\nstep b\n# SECTION: ship\nstep c\n").unwrap();
    let (code, out, err) = run(&d, concat!(
        "rel sec(p: file, l0: int, l1: int, name: text).\n",
        "sec(p, l0, l1, name) <- scan(\"WORK\", \"*.sh\", p, rev), ",
        "comment(p, rev, /SECTION: $name/, l0, l1, name).\n",
        "? sec(p, l0, l1, name).\n"));
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("\t1\t3\tbuild"), "{out}");
    assert!(out.contains("\t4\t5\tship"), "{out}");
}

#[test]
fn paired_markers_nest_and_bind_the_open_label() {
    let d = sandbox("paired");
    fs::write(d.join("gen.rs"),
        "// BEGIN: types\nstruct A;\n// BEGIN: inner\nstruct B;\n// END:\n// END:\nfn x() {}\n").unwrap();
    let (code, out, err) = run(&d, concat!(
        "rel block(p: file, l0: int, l1: int, name: text).\n",
        "block(p, l0, l1, name) <- scan(\"WORK\", \"*.rs\", p, rev), ",
        "comment(p, rev, /BEGIN: $name/, /END:/, l0, l1, name).\n",
        "? block(p, l0, l1, name).\n"));
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("\t1\t6\ttypes"), "outer pair:\n{out}");
    assert!(out.contains("\t3\t5\tinner"), "inner pair:\n{out}");
}

#[test]
fn regions_join_other_relations_by_line_interval() {
    // The point of binding l0/l1: attribute per-line facts to their section.
    let d = sandbox("join");
    fs::write(d.join("conf.sh"),
        "# SECTION: prod\nurl=a\n# SECTION: dev\nurl=b\n").unwrap();
    let (code, out, err) = run(&d, concat!(
        "rel sec(p: file, l0: int, l1: int, name: text).\n",
        "sec(p, l0, l1, name) <- scan(\"WORK\", \"*.sh\", p, rev), ",
        "comment(p, rev, /SECTION: $name/, l0, l1, name).\n",
        "rel url(p: file, l: int).\n",
        "url(p, l) <- scan(\"WORK\", \"*.sh\", p, rev), match(p, rev, /url=/, l).\n",
        "rel owned(name: text, l: int).\n",
        "owned(name, l) <- sec(p, l0, l1, name), url(p, l), l >= l0, l <= l1.\n",
        "? owned(name, l).\n"));
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("prod\t2"), "{out}");
    assert!(out.contains("dev\t4"), "{out}");
}

/// `comment(p, rev, /re/, label: name)` — the kwarg form binds only the named
/// output and defaults the rest to `_`. No throwaway l0/l1 vars needed.
#[test]
fn kwarg_binds_only_the_named_output() {
    let d = sandbox("kwarg");
    fs::write(d.join("deploy.sh"),
        "# SECTION: build\nstep a\n# SECTION: ship\nstep b\n").unwrap();
    let (code, out, err) = run(&d, concat!(
        "rel named(p: file, name: text).\n",
        "named(p, name) <- scan(\"WORK\", \"*.sh\", p, rev), ",
        "comment(p, rev, /SECTION: $name/, label: name).\n",
        "? named(p, name).\n"));
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("build"), "kwarg label must bind build:\n{out}");
    assert!(out.contains("ship"), "kwarg label must bind ship:\n{out}");
}

/// `comment(p, rev, /re/, _, _, name)` — wildcard form drops the unwanted slots.
#[test]
fn wildcard_outputs_drop_unwanted_slots() {
    let d = sandbox("wild");
    fs::write(d.join("deploy.sh"),
        "# SECTION: build\nstep a\n# SECTION: ship\nstep b\n").unwrap();
    let (code, out, err) = run(&d, concat!(
        "rel named(p: file, name: text).\n",
        "named(p, name) <- scan(\"WORK\", \"*.sh\", p, rev), ",
        "comment(p, rev, /SECTION: $name/, _, _, name).\n",
        "? named(p, name).\n"));
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("build") && out.contains("ship"), "{out}");
}

/// `comment(p, rev, /re/, l0, label: name)` — positional prefix then kwarg skips
/// the middle (l1) output. The form used in gen-doc-index.dl.
#[test]
fn mixed_positional_then_kwarg_skips_middle() {
    let d = sandbox("mixed");
    fs::write(d.join("deploy.sh"),
        "# SECTION: build\nstep a\n# SECTION: ship\nstep b\n").unwrap();
    let (code, out, err) = run(&d, concat!(
        "rel first(p: file, l0: int, name: text).\n",
        "first(p, l0, name) <- scan(\"WORK\", \"*.sh\", p, rev), ",
        "comment(p, rev, /SECTION: $name/, l0, label: name).\n",
        "? first(p, l0, name).\n"));
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("\t1\tbuild"), "l0 positional + label kwarg:\n{out}");
    assert!(out.contains("\t3\tship"), "{out}");
}

/// A typo'd output name is a parse error, caught at parse time.
#[test]
fn unknown_kwarg_name_is_a_parse_error() {
    let d = sandbox("badkwarg");
    fs::write(d.join("a.sh"), "# x\n").unwrap();
    let (code, _out, err) = run(&d, concat!(
        "rel x(p: file).\n",
        "x(p) <- scan(\"WORK\", \"*.sh\", p, rev), ",
        "comment(p, rev, /./, bogus: y).\n"));
    assert_ne!(code, 0, "expected parse failure");
    assert!(err.contains("unknown output arg"), "expected unknown-arg diagnostic:\n{err}");
}
