//! Integration test for the CLI JSONL contract: runs the built binary on a
//! tiny input and confirms the three stdout events parse as JSON objects with
//! the required fields.

use std::collections::BTreeMap;
use std::io::Write;

fn write_input(path: &std::path::Path, text: &str) {
    let mut file = std::fs::File::create(path).expect("create input");
    file.write_all(text.as_bytes()).expect("write input");
}

fn parse_json_object(line: &str) -> BTreeMap<String, String> {
    // Minimal parser for flat JSON objects of string and integer values, the
    // only shapes the engine emits. Enough to prove the events are JSON.
    let bytes = line.as_bytes();
    assert_eq!(bytes.first(), Some(&b'{'), "not an object: {line}");
    assert_eq!(bytes.last(), Some(&b'}'), "not an object: {line}");
    let inner = &line[1..line.len() - 1];
    let mut map = BTreeMap::new();
    let mut chars = inner.chars().peekable();

    fn read_string(chars: &mut std::iter::Peekable<std::str::Chars>) -> String {
        assert_eq!(chars.next(), Some('"'), "expected opening quote");
        let mut value = String::new();
        for ch in chars.by_ref() {
            if ch == '"' {
                break;
            }
            value.push(ch);
        }
        value
    }

    fn read_number(chars: &mut std::iter::Peekable<std::str::Chars>) -> String {
        let mut value = String::new();
        while let Some(&ch) = chars.peek() {
            if ch.is_ascii_digit() || ch == '-' {
                value.push(ch);
                chars.next();
            } else {
                break;
            }
        }
        value
    }

    while let Some(ch) = chars.next() {
        if ch.is_whitespace() || ch == ',' || ch == ':' {
            continue;
        }
        if ch == '"' {
            // Re-read the full key token already consumed by our iterator.
            let mut part = String::new();
            part.push(ch);
            for next in chars.by_ref() {
                part.push(next);
                if next == '"' {
                    break;
                }
            }
            assert!(!part.is_empty(), "bad key token");
            // part is `"key"`; strip quotes.
            let key = &part[1..part.len() - 1];
            assert_eq!(chars.next(), Some(':'), "expected colon after {key}");
            while chars.peek().map(|ch| ch.is_whitespace()).unwrap_or(false) {
                chars.next();
            }
            let first = chars.peek().copied();
            let value = match first {
                Some('"') => read_string(&mut chars),
                _ => read_number(&mut chars),
            };
            map.insert(key.to_string(), value);
        }
    }
    map
}

#[test]
fn cli_emits_three_jsonl_events() {
    let binary = env!("CARGO_BIN_EXE_rxgraph");
    let dir = std::env::temp_dir().join("rxgraph_cli_test");
    std::fs::create_dir_all(&dir).expect("mkdir");
    let input_path = dir.join("input.txt");
    write_input(&input_path, "p 4 3\n0 1\n1 2\n2 3\n");

    let output = std::process::Command::new(binary)
        .arg("--input")
        .arg(&input_path)
        .output()
        .expect("run binary");

    assert!(output.status.success(), "non-zero exit");

    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 3, "expected 3 events, got: {stdout}");

    let loaded = parse_json_object(lines[0]);
    assert_eq!(loaded.get("event").map(String::as_str), Some("loaded"));
    assert_eq!(loaded.get("edges").map(String::as_str), Some("3"));

    let fixpoint = parse_json_object(lines[1]);
    assert_eq!(fixpoint.get("event").map(String::as_str), Some("fixpoint"));
    assert_eq!(fixpoint.get("derived").map(String::as_str), Some("6"));

    let done = parse_json_object(lines[2]);
    assert_eq!(done.get("event").map(String::as_str), Some("done"));
    let checksum = done.get("checksum").expect("checksum field");
    assert_eq!(checksum.len(), 16, "checksum must be 16 hex chars");
    let _ = u64::from_str_radix(checksum, 16).expect("checksum is hex");

    let peak = done.get("peak_rss_kb").expect("peak_rss_kb field");
    assert!(!peak.is_empty());
}
