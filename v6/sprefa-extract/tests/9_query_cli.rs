use std::process::Command;

const RUST: &str = "tests/fixtures/rust/sample.rs";
const TS: &str = "tests/fixtures/ts/sample.ts";

fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_extract"))
        .args(args)
        .output()
        .expect("extract binary runs")
}

fn run_in(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_extract"))
        .current_dir(dir)
        .args(args)
        .output()
        .expect("extract binary runs")
}

fn git(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .expect("git runs")
}

fn temp_repo() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "blobdoor_query_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    git(&dir, &["init", "-q"]);
    dir
}

#[test]
fn query_emits_flat_jsonl_for_plain_and_alternating_patterns() {
    let plain = run(&[
        "query",
        "--lang",
        "rust",
        "--query",
        "(function_item name: (identifier) @name) @item",
        RUST,
    ]);
    assert!(plain.status.success());
    assert_eq!(
        String::from_utf8(plain.stdout).unwrap(),
        "{\"end_line\":17,\"item\":\"pub fn trim(value: String) -> String {\\n    value\\n}\",\"line\":15,\"name\":\"trim\"}\n{\"end_line\":23,\"item\":\"pub fn make_engine(name: String) -> Engine {\\n    let trimmed = trim(name);\\n    let engine = Engine { name: trimmed };\\n    engine\\n}\",\"line\":19,\"name\":\"make_engine\"}\n{\"end_line\":29,\"item\":\"pub fn mode(&self) -> Mode {\\n        let picked = Mode::Fast;\\n        picked\\n    }\",\"line\":26,\"name\":\"mode\"}\n{\"end_line\":35,\"item\":\"pub fn apply(value: String) -> String {\\n    let func = |text: String| text;\\n    func(value)\\n}\",\"line\":32,\"name\":\"apply\"}\n"
    );

    let alternate = run(&[
        "query",
        "--lang",
        "rust",
        "--query",
        "[(function_item name: (identifier) @name) @item (struct_item name: (type_identifier) @name) @item]",
        RUST,
    ]);
    assert!(alternate.status.success());
    assert_eq!(
        alternate
            .stdout
            .iter()
            .filter(|byte| **byte == b'\n')
            .count(),
        5
    );
}

#[test]
fn query_predicates_filter_matches() {
    let output = run(&[
        "query",
        "--lang",
        "ts",
        "--query",
        "((function_declaration name: (identifier) @name) @item (#match? @name \"^s\"))",
        TS,
    ]);
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "{\"end_line\":12,\"item\":\"function shift(p: Point, d: Dir): Vec2 {\\n  function clamp(n: number): number {\\n    return n;\\n  }\\n  return new Vec2(clamp(p.x), clamp(p.y));\\n}\",\"line\":7,\"name\":\"shift\"}\n"
    );
}

#[test]
fn query_rejects_unknown_language_and_invalid_query_with_exit_two() {
    let unknown = run(&[
        "query",
        "--lang",
        "ruby",
        "--query",
        "(identifier) @name",
        RUST,
    ]);
    assert_eq!(unknown.status.code(), Some(2));
    assert_eq!(
        String::from_utf8(unknown.stderr).unwrap(),
        "unknown lang 'ruby'\n"
    );

    let invalid = run(&["query", "--lang", "rust", "--query", "(", RUST]);
    assert_eq!(invalid.status.code(), Some(2));
    assert!(String::from_utf8(invalid.stderr).unwrap().lines().count() == 1);
}

#[test]
fn query_with_digest_reads_the_staged_blob() {
    let dir = temp_repo();
    let fixture = std::fs::canonicalize(RUST).unwrap();
    let fixture = fixture.to_str().unwrap();
    let hash = git(&dir, &["hash-object", "-w", fixture]);
    let oid = String::from_utf8(hash.stdout).unwrap().trim().to_string();

    let query = "(function_item name: (identifier) @name) @item";
    let via_path = run(&["query", "--lang", "rust", "--query", query, RUST]);
    let blob_path = dir.join("sample.rs");
    let via_digest = run_in(
        &dir,
        &[
            "query",
            "--lang",
            "rust",
            "--query",
            query,
            "--digest",
            &oid,
            blob_path.to_str().unwrap(),
        ],
    );

    assert!(via_path.status.success());
    assert!(via_digest.status.success());
    assert_eq!(via_digest.stdout, via_path.stdout);
}

#[test]
fn query_bad_digest_exits_two_with_one_line_stderr() {
    let output = run(&[
        "query",
        "--lang",
        "rust",
        "--query",
        "(identifier) @name",
        "--digest",
        "0000000000000000000000000000000000000000",
        RUST,
    ]);
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert_eq!(stderr.lines().count(), 1);
    assert!(stderr.contains("git cat-file blob"));
}

fn temp_file(name: &str, content: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "query_cli_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, content).unwrap();
    path
}

#[test]
fn query_markdown_block_grammar_emits_headings_jsonl() {
    let path = temp_file("sample.md", "# Title\n\nA paragraph.\n\n```js\ncode\n```\n");
    let path = path.to_str().unwrap();
    let output = run(&[
        "query",
        "--lang",
        "md",
        "--query",
        "(atx_heading) @heading",
        path,
    ]);
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "{\"end_line\":2,\"heading\":\"# Title\\n\",\"line\":1}\n"
    );
}

#[test]
fn query_markdown_inline_grammar_drops_in_without_structural_change() {
    let path = temp_file("sample_inline.md", "This is *em* and [link](url).");
    let path = path.to_str().unwrap();
    let output = run(&[
        "query",
        "--lang",
        "md_inline",
        "--query",
        "[(emphasis) @em (inline_link) @lnk]",
        path,
    ]);
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "{\"em\":\"*em*\",\"end_line\":1,\"line\":1}\n{\"end_line\":1,\"line\":1,\"lnk\":\"[link](url)\"}\n"
    );
}

#[test]
fn query_html_grammar_emits_tag_names_jsonl() {
    let path = temp_file("sample.html", "<div class=\"x\"><p>hi</p></div>");
    let path = path.to_str().unwrap();
    let output = run(&[
        "query",
        "--lang",
        "html",
        "--query",
        "(element (start_tag (tag_name) @tag))",
        path,
    ]);
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "{\"end_line\":1,\"line\":1,\"tag\":\"div\"}\n{\"end_line\":1,\"line\":1,\"tag\":\"p\"}\n"
    );
}
