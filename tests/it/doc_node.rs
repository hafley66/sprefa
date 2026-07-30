//! The built-in `doc_node(file, line, kind, name, parent)` document-structure
//! relation, populated by the `ingest::IngestLang` registry (markdown is the first
//! customer). A source rule scanning `**/*.md` feeds `_file` exactly as `**/*.rs`
//! feeds the type graph; `refresh_doc_rels` walks the registry over those files
//! and emits one row per heading / code block, with `parent` naming the enclosing
//! heading so a rule can walk the section tree.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("doc_node_{tag}"));
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
        .output()
        .expect("run dl");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// `# Title` / `## Sub` / fenced ` ```rs ` / `### Deep`: four doc_nodes whose
/// parents nest by heading level, and a code block carries its fence language.
fn fixture(tag: &str) -> PathBuf {
    let d = sandbox(tag);
    fs::create_dir_all(d.join("docs")).unwrap();
    fs::write(
        d.join("docs/guide.md"),
        concat!(
            "# Title\n",
            "intro paragraph\n",
            "## Sub\n",
            "```rs\n",
            "fn x() {}\n",
            "```\n",
            "### Deep\n",
        ),
    )
    .unwrap();
    d
}

/// The scan feeds `_file`; doc_node lists the two headings, the code block, and
/// Deep, with parents nesting by level (Title none, Sub under Title, the code
/// block under Sub, Deep under Sub).
#[test]
fn markdown_headings_and_code_blocks_become_doc_nodes() {
    let d = fixture("basic");
    let prog = concat!(
        "rel seen(path: file).\n",
        "seen(path) <- scan(\"WORK\", \"docs/**/*.md\", path, rev), match(path, rev, /./, line).\n",
        "? doc_node(_, file, line, kind, name, parent).\n",
    );
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "stderr: {err}\nstdout: {out}");
    // Title heading at line 1, top level (empty parent).
    assert!(
        out.contains("docs/guide.md\t1\theading\tTitle\t"),
        "Title heading row missing:\n{out}"
    );
    // Sub under Title at line 3.
    assert!(
        out.contains("docs/guide.md\t3\theading\tSub\tTitle"),
        "Sub heading must nest under Title:\n{out}"
    );
    // code block at line 4, language "rs", under Sub.
    assert!(
        out.contains("docs/guide.md\t4\tcode_block\trs\tSub"),
        "code block must carry its fence language and Sub parent:\n{out}"
    );
    // Deep under Sub (level 3 nests under level 2) at line 7.
    assert!(
        out.contains("docs/guide.md\t7\theading\tDeep\tSub"),
        "Deep must nest under Sub:\n{out}"
    );
}

/// `doc_node` joins `changed` so a rail can scope to touched docs: edit the
/// markdown, and only the touched doc's nodes surface. This is the "track docs
/// from outside the codebase as a .dl rule" pattern the CST layer enables.
#[test]
fn doc_node_joins_changed_to_scope_to_touched_docs() {
    let d = sandbox("changed");
    git(&d, &["init", "-q"]);
    git(&d, &["config", "user.email", "t@t"]);
    git(&d, &["config", "user.name", "t"]);
    fs::create_dir_all(d.join("docs")).unwrap();
    fs::write(d.join("docs/clean.md"), "# Old\n").unwrap();
    fs::write(d.join("docs/touched.md"), "# Before\n").unwrap();
    git(&d, &["add", "-A"]);
    git(&d, &["commit", "-qm", "base"]);
    fs::write(d.join("docs/touched.md"), "# After\n").unwrap();
    let prog = concat!(
        "rel seen(path: file).\n",
        "seen(path) <- scan(\"WORK\", \"docs/**/*.md\", path, rev).\n",
        "diag(path: p, line: l, severity: \"warn\", code: \"doc-touched\", msg: \"heading in a changed doc\") <-\n",
        "    doc_node(_, p, l, \"heading\", n, par), changed(p).\n",
        "? diag(path: p, line: l, severity: s, code: c, msg: m).\n",
    );
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "stderr: {err}\nstdout: {out}");
    assert!(
        out.contains("docs/touched.md"),
        "the touched doc must trip the rail:\n{out}"
    );
    assert!(
        !out.contains("docs/clean.md"),
        "the clean doc must stay silent:\n{out}"
    );
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .expect("git");
    assert!(
        out.status.success(),
        "git {args:?} failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// `doc_node` is a reserved name.
#[test]
fn doc_node_is_reserved() {
    let d = sandbox("reserved");
    let (code, _out, err) = run(&d, "rel doc_node(a: text).\n");
    assert_ne!(code, 0);
    assert!(
        err.contains("built-in"),
        "reserved-name error expected:\n{err}"
    );
}
