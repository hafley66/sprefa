use std::process::Command;

const RUST: &str = "tests/fixtures/rust/sample.rs";
const TS: &str = "tests/fixtures/ts/sample.ts";

fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_extract"))
        .args(args)
        .output()
        .expect("extract binary runs")
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
