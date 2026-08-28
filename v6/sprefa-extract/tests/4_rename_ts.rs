//! `extract rename` on the TS arm: arc 1's anchor-file rename over
//! `tests/fixtures/ts_rename/local`, judged byte-exact against a hand written
//! `after/` tree, arc 2's named stops over `tests/fixtures/ts_rename/stops/`,
//! and arc 3's importer walk over `tests/fixtures/ts_rename/exports/`.
//!
//! @comment-ok: fail-first receipt, repo law keeps these in TEST headers.
//! FAIL-FIRST (arc 2), against the arc-1 binary:
//!     ambiguous_stops_then_at_selects_the_class ... left: Some(0), right: Some(3)
//!         (the run renamed the class instead of stopping)
//!     commit_renames_the_anchor_file ... exited 2: error: unexpected argument '--at' found
//!     dry_run_touches_nothing ... exited 2: error: unexpected argument '--at' found
//!     dynamic_seats_stop_and_write_nothing ... read fixture dir: NotFound
//!         (the stop did not exist)
//!     inexact_is_unreachable_from_the_ts_arm ... ts_rename.rs constructs Inexact
//!     unknown_symbol_stops_and_writes_nothing ... left: Some(2), right: Some(4)
//! FAIL-FIRST (arc 3), against the arc-2 binary:
//!     exported_symbol_renames_every_importer ... committed tree differs from after/:
//!         Files .../src/a.ts and .../after/src/a.ts differ (and b, barrel, c, d)
//!     aliased_import_moves_only_the_imported_seat ... src/b.ts kept `import { Foo as Bar }`
//!     dry_run_prints_per_file_counts ... missing "  src/a.ts  3 uses"
//!     dynamic_stop_lists_every_seat ... Dynamic seat offset 108 missing:
//!         (the stop reported one seat per run)
//!     tsc_is_clean_on_the_committed_tree ... committed tree failed tsc:
//!         src/a.ts(1,10): error TS2724: '"./lib"' has no exported member named 'Foo'.
//! FAIL-FIRST (arc 4), against the arc-3 binary:
//!     text_refs_reports_the_string_and_the_readme ... exited 2: error: unexpected
//!         argument '--text-refs' found
//!     text_refs_never_writes ... exited 2: error: unexpected argument '--text-refs' found

use std::path::{Path, PathBuf};
use std::process::Command;

const ANCHOR: &str = "src/app.ts";

/// Arc 3's fixture: `src/lib.ts` exports `Foo`, five importers reach it.
const EXPORTS: &str = "exports";
const EXPORTS_ANCHOR: &str = "src/lib.ts";

struct Fixture {
    root: PathBuf,
    state: PathBuf,
}

fn fixture(case: &str, label: &str) -> Fixture {
    let base = std::env::temp_dir().join(format!(
        "extract_rename_ts_{case}_{label}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let root = base.join("repo");
    let state = base.join("state");
    std::fs::create_dir_all(&state).unwrap();
    copy_tree(&tree(case, "before"), &root);
    Fixture {
        root: root.canonicalize().unwrap(),
        state,
    }
}

fn tree(case: &str, side: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/fixtures/ts_rename/{case}/{side}"))
}

fn copy_tree(source: &Path, target: &Path) {
    std::fs::create_dir_all(target).expect("create target dir");
    for entry in std::fs::read_dir(source).expect("read fixture dir") {
        let entry = entry.expect("fixture entry");
        let to = target.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            copy_tree(&entry.path(), &to);
        } else {
            std::fs::copy(entry.path(), &to).expect("copy fixture file");
        }
    }
}

fn rename_verb(fixture: &Fixture, target: &str, new: &str, extra: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .arg("rename")
        .arg(target)
        .arg(new)
        .arg("--root")
        .arg(&fixture.root)
        .arg("--state")
        .arg(&fixture.state)
        .args(extra)
        .output()
        .expect("extract binary runs");
    assert!(
        output.status.success(),
        "extract rename {extra:?} exited {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("stdout is UTF-8")
}

/// A failed rename run: status, stderr, and the untouched tree claim.
struct StoppedRun {
    code: Option<i32>,
    stderr: String,
}

fn stopped_rename_verb(fixture: &Fixture, target: &str, new: &str, extra: &[&str]) -> StoppedRun {
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .arg("rename")
        .arg(target)
        .arg(new)
        .arg("--root")
        .arg(&fixture.root)
        .arg("--state")
        .arg(&fixture.state)
        .args(extra)
        .arg("--commit")
        .output()
        .expect("extract binary runs");
    StoppedRun {
        code: output.status.code(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    }
}

fn assert_untouched(fixture: &Fixture, case: &str) {
    let entries = diff_rq(&fixture.root, &tree(case, "before"));
    assert!(
        entries.is_empty(),
        "the run edited the tree:\n{}",
        entries.join("\n")
    );
}

/// Byte offset of the identifier `"{prefix} {name}"` inside the fixture's
/// before tree, so a test never hardcodes a number the fixture text owns.
fn ident_offset(case: &str, prefix: &str, name: &str) -> usize {
    let text = std::fs::read_to_string(tree(case, "before").join(ANCHOR)).expect("fixture text");
    let needle = format!("{prefix} {name}");
    let start = text.find(&needle).expect("needle in fixture");
    start + prefix.len() + 1
}

/// `diff -rq left right`, as the arc-1 receipt spells it. Returns the entries.
fn diff_rq(left: &Path, right: &Path) -> Vec<String> {
    let output = Command::new("diff")
        .arg("-rq")
        .arg(left)
        .arg(right)
        .output()
        .expect("diff runs");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::to_string)
        .collect()
}

/// The committed tree is the hand-written `after/` tree, byte for byte. The
/// string literal, the comment and the shadowed inner binding are inside that
/// judgement: `after/` keeps all three spelling `oldName`.
#[test]
fn commit_renames_the_anchor_file() {
    let fixture = fixture("local", "commit");
    let at = ident_offset("local", "function", "oldName").to_string();
    let stdout = rename_verb(
        &fixture,
        &format!("{ANCHOR}#oldName"),
        "newName",
        &["--at", &at, "--commit"],
    );
    assert!(
        stdout.contains(&format!("plan {ANCHOR} oldName -> newName")),
        "plan line missing:\n{stdout}"
    );
    assert!(
        stdout.contains("committed"),
        "commit line missing:\n{stdout}"
    );
    let entries = diff_rq(&fixture.root, &tree("local", "after"));
    assert!(
        entries.is_empty(),
        "committed tree differs from after/:\n{}",
        entries.join("\n")
    );
}

/// Without `--commit` the tree is byte-identical to `before/` and stdout carries
/// the plan lines.
#[test]
fn dry_run_touches_nothing() {
    let fixture = fixture("local", "dry");
    let at = ident_offset("local", "function", "oldName").to_string();
    let stdout = rename_verb(
        &fixture,
        &format!("{ANCHOR}#oldName"),
        "newName",
        &["--at", &at],
    );
    for line in [
        &format!("root {}", fixture.root.display()),
        &format!("plan {ANCHOR} oldName -> newName"),
        &format!("  {ANCHOR}  4 uses"),
    ] {
        assert!(stdout.contains(line.as_str()), "missing {line}:\n{stdout}");
    }
    assert!(
        stdout.contains("dry run, tree untouched"),
        "dry-run stage line missing:\n{stdout}"
    );
    assert_untouched(&fixture, "local");
}

/// A name no declaration in the anchor binds is `RenameStop::NotFound`, exit 4,
/// and the tree stays put.
#[test]
fn unknown_symbol_stops_and_writes_nothing() {
    let fixture = fixture("local", "notfound");
    let stopped = stopped_rename_verb(&fixture, &format!("{ANCHOR}#absentName"), "newName", &[]);
    assert_eq!(
        stopped.code,
        Some(4),
        "NotFound exits 4:\n{}",
        stopped.stderr
    );
    assert!(
        stopped
            .stderr
            .contains(&format!("{ANCHOR} declares no absentName")),
        "NotFound message missing:\n{}",
        stopped.stderr
    );
    assert_untouched(&fixture, "local");
}

/// Two same-named declarations in one file stop the run with BOTH byte offsets
/// named; `--at <offset of the class>` then renames the class and its uses
/// only, leaving the function-local `Foo` spelling `Foo`.
#[test]
fn ambiguous_stops_then_at_selects_the_class() {
    let case = "stops/ambiguous";
    let stopped_tree = fixture(case, "stop");
    let stopped = stopped_rename_verb(&stopped_tree, &format!("{ANCHOR}#Foo"), "Bar", &[]);
    assert_eq!(
        stopped.code,
        Some(3),
        "Ambiguous exits 3:\n{}",
        stopped.stderr
    );
    let class_offset = ident_offset(case, "class", "Foo");
    let local_offset = ident_offset(case, "const", "Foo");
    let stderr = stopped.stderr.replace('\n', " ");
    assert!(
        stderr.contains(&format!("at bytes {local_offset}, {class_offset}"))
            || stderr.contains(&format!("at bytes {class_offset}, {local_offset}")),
        "Ambiguous message must name both offsets {local_offset} and {class_offset}:\n{}",
        stopped.stderr
    );
    assert_untouched(&stopped_tree, case);
    let copy = fixture(case, "select");
    let at = class_offset.to_string();
    rename_verb(
        &copy,
        &format!("{ANCHOR}#Foo"),
        "Bar",
        &["--at", &at, "--commit"],
    );
    let entries = diff_rq(&copy.root, &tree(case, "after"));
    assert!(
        entries.is_empty(),
        "--at commit differs from after/:\n{}",
        entries.join("\n")
    );
}

/// Every seat the TS arm can reach is pinned by `oxc_semantic` to the exact
/// identifier token, so `RenameStop::Inexact` is unreachable from this arm: the
/// source never constructs it, and a battery of read/write/type-ref seats
/// commits byte-exact against the hand-written after tree.
#[test]
fn inexact_is_unreachable_from_the_ts_arm() {
    let arm = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/lang/ts_rename.rs");
    let source = std::fs::read_to_string(&arm).expect("ts_rename.rs readable");
    assert!(
        !source.contains("Inexact"),
        "ts_rename.rs constructs Inexact; the unreachable claim is void"
    );

    let case = "stops/inexact";
    let fixture = fixture(case, "commit");
    rename_verb(&fixture, &format!("{ANCHOR}#Foo"), "Bar", &["--commit"]);
    let entries = diff_rq(&fixture.root, &tree(case, "after"));
    assert!(
        entries.is_empty(),
        "committed tree differs from after/:\n{}",
        entries.join("\n")
    );
}

/// `obj["Foo"]` and `import("./m").then(m => m.Foo)` reach the symbol only at
/// runtime; the run stops with EVERY seat's file and offset, exit 6, and the
/// tree stays put. One seat per run would leave the second repair invisible
/// until the first was fixed, so the stop carries the whole list.
#[test]
fn dynamic_stop_lists_every_seat() {
    let case = "stops/dynamic";
    let fixture = fixture(case, "stop");
    let stopped = stopped_rename_verb(&fixture, &format!("{ANCHOR}#Foo"), "Bar", &[]);
    assert_eq!(
        stopped.code,
        Some(6),
        "Dynamic exits 6:\n{}",
        stopped.stderr
    );
    let seat_text =
        std::fs::read_to_string(tree(case, "before").join(ANCHOR)).expect("fixture text");
    let computed_offset = seat_text
        .find("[\"Foo\"]")
        .expect("computed seat in fixture")
        + 1;
    let member_offset = seat_text
        .find("module.Foo")
        .expect("member seat in fixture")
        + "module.".len();
    let stderr = stopped.stderr.replace('\n', " ");
    for form in ["computed member", "member access"] {
        assert!(
            stderr.contains(form),
            "Dynamic form {form} missing:\n{}",
            stopped.stderr
        );
    }
    for offset in [computed_offset, member_offset] {
        assert!(
            stderr.contains(&format!("{ANCHOR} byte {offset}")),
            "Dynamic seat offset {offset} missing:\n{}",
            stopped.stderr
        );
    }
    assert_untouched(&fixture, case);
}

// ── arc 3: the importer walk ────────────────────────────────────────────────

/// An exported symbol's rename reaches every file the importer graph joins to
/// the anchor: a bare import, an aliased import, a `export {} from` barrel, a
/// `export * from` relay and the file importing through it. The committed tree
/// is the hand-written `after/` tree, byte for byte, which also pins what stays
/// put: `src/e.ts`'s `"Foo"` string, `src/star.ts`, and `makeFoo`, whose name
/// carries `Foo` as a substring the scope plane never binds.
#[test]
fn exported_symbol_renames_every_importer() {
    let fixture = fixture(EXPORTS, "commit");
    rename_verb(
        &fixture,
        &format!("{EXPORTS_ANCHOR}#Foo"),
        "Baz",
        &["--commit"],
    );
    let entries = diff_rq(&fixture.root, &tree(EXPORTS, "after"));
    assert!(
        entries.is_empty(),
        "committed tree differs from after/:\n{}",
        entries.join("\n")
    );
}

/// `import { Foo as Bar }` moves the `Foo` seat alone. `Bar` is a binding this
/// file owns, so it and its three body uses are outside the rename.
#[test]
fn aliased_import_moves_only_the_imported_seat() {
    let fixture = fixture(EXPORTS, "alias");
    rename_verb(
        &fixture,
        &format!("{EXPORTS_ANCHOR}#Foo"),
        "Baz",
        &["--commit"],
    );
    let text = std::fs::read_to_string(fixture.root.join("src/b.ts")).expect("committed src/b.ts");
    assert!(
        text.contains("import { Baz as Bar }"),
        "the imported seat did not move:\n{text}"
    );
    let body = text.split_once('\n').expect("an import line").1;
    assert_eq!(
        body.matches("Bar").count(),
        3,
        "the local binding's uses changed:\n{text}"
    );
    assert_eq!(
        body.matches("Baz").count(),
        0,
        "the new name leaked into the body:\n{text}"
    );
}

/// The dry run prints one count line per touched file and writes nothing. A
/// file the graph reaches but the symbol never seats in (`src/star.ts`'s
/// `export *`) and a file that only spells the name in a string (`src/e.ts`)
/// carry no line at all.
#[test]
fn dry_run_prints_per_file_counts() {
    let fixture = fixture(EXPORTS, "dry");
    let stdout = rename_verb(&fixture, &format!("{EXPORTS_ANCHOR}#Foo"), "Baz", &[]);
    for (file, uses) in [
        ("src/a.ts", 3),
        ("src/b.ts", 1),
        ("src/barrel.ts", 1),
        ("src/c.ts", 2),
        ("src/d.ts", 2),
        ("src/lib.ts", 3),
    ] {
        let line = format!("  {file}  {uses} uses");
        assert!(stdout.contains(&line), "missing {line}:\n{stdout}");
    }
    for untouched in ["src/star.ts", "src/e.ts"] {
        assert!(
            !stdout.contains(untouched),
            "{untouched} is not touched, so it carries no count line:\n{stdout}"
        );
    }
    assert!(
        stdout.contains("dry run, tree untouched"),
        "dry-run stage line missing:\n{stdout}"
    );
    assert_untouched(&fixture, EXPORTS);
}

// ── arc 4: the --text-refs report ───────────────────────────────────────────

/// The line of README.md's prose mention, read from the before tree so the
/// test never hardcodes a number the fixture prose owns.
fn readme_mention_line(case: &str) -> usize {
    let text = std::fs::read_to_string(tree(case, "before").join("README.md")).expect("readme");
    1 + text
        .lines()
        .position(|line| line.contains("Foo"))
        .expect("Foo mention in README")
}

/// `--text-refs` reports exactly the two text carriers the plan never touches:
/// the `"Foo"` string in src/e.ts and README's prose mention. Every line the
/// plan rewrites is excluded, so lib.ts's `makeFoo`, whose name carries `Foo`
/// as a substring, never becomes a row.
#[test]
fn text_refs_reports_the_string_and_the_readme() {
    let fixture = fixture(EXPORTS, "textrefs");
    let stdout = rename_verb(
        &fixture,
        &format!("{EXPORTS_ANCHOR}#Foo"),
        "Baz",
        &["--text-refs"],
    );
    let rows: Vec<String> = stdout
        .lines()
        .filter(|line| line.starts_with("text-ref "))
        .map(str::to_string)
        .collect();
    let expected = vec![
        format!(
            "text-ref README.md:{} Foo -> Baz",
            readme_mention_line(EXPORTS)
        ),
        "text-ref src/e.ts:1 \"Foo\" -> \"Baz\"".to_string(),
    ];
    assert_eq!(rows, expected, "exactly the two text carriers:\n{stdout}");
    assert_untouched(&fixture, EXPORTS);
}

/// `--commit --text-refs` still writes exactly the plan: the report names the
/// carriers and rewrites nothing, so src/e.ts and README.md stay byte-identical
/// to `before/` and the committed tree matches `after/` with zero entries.
#[test]
fn text_refs_never_writes() {
    let fixture = fixture(EXPORTS, "textrefs_commit");
    rename_verb(
        &fixture,
        &format!("{EXPORTS_ANCHOR}#Foo"),
        "Baz",
        &["--commit", "--text-refs"],
    );
    let entries = diff_rq(&fixture.root, &tree(EXPORTS, "after"));
    assert!(
        entries.is_empty(),
        "the report changed the committed tree:\n{}",
        entries.join("\n")
    );
}

/// Without `--text-refs` the report is silent.
#[test]
fn without_the_flag_no_text_ref_rows() {
    let fixture = fixture(EXPORTS, "textrefs_off");
    let stdout = rename_verb(&fixture, &format!("{EXPORTS_ANCHOR}#Foo"), "Baz", &[]);
    assert!(
        !stdout.contains("text-ref"),
        "no text-ref row without the flag:\n{stdout}"
    );
}

/// The committed tree still typechecks. `diff -rq` judges the bytes; only the
/// compiler judges whether the import graph still joins up.
/// Measured 1.8 s warm, under the 10 s cap; the compiler is fetched by npx, so
/// a cold cache pays a network round trip once.
#[test]
fn tsc_is_clean_on_the_committed_tree() {
    let fixture = fixture(EXPORTS, "tsc");
    rename_verb(
        &fixture,
        &format!("{EXPORTS_ANCHOR}#Foo"),
        "Baz",
        &["--commit"],
    );
    let output = Command::new("npx")
        .args(["--yes", "-p", "typescript", "tsc", "--noEmit", "-p"])
        .arg(&fixture.root)
        .output()
        .expect("npx runs");
    assert!(
        output.status.success(),
        "committed tree failed tsc:\n{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
