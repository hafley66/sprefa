//! Laws over `src/**` (`CLAUDE.md`, standing laws): every loop, `while`, recursive fn and DD
//! fixpoint is listed in `SITES` against its budget, and no `eprintln!`, `Mutex` or `RwLock`
//! appears. A site the scanner finds and `SITES` lacks fails the test with the row to add; a
//! `SITES` row the scanner no longer finds fails it too.

use std::path::{Path, PathBuf};

/// One bounded site.
struct Site {
    /// Under the crate root.
    path: &'static str,
    /// The site's line, trimmed.
    text: &'static str,
    kind: &'static str,
    /// Appears in the enclosing fn body: the budget the site counts against.
    budget: &'static str,
    /// Declared `const` in `src/**` with a `///` comment above it saying what it protects.
    constant: &'static str,
    /// Appears in the site's file: the named diagnostic a hit stops with.
    diag: &'static str,
}

const SITES: &[Site] = &[
    Site {
        path: "src/_6_back/_1_dd.rs",
        text: "for _ in 0..MAX_WORKER_STEPS {",
        kind: "for budget",
        budget: "MAX_WORKER_STEPS",
        constant: "MAX_WORKER_STEPS",
        diag: "Diag::StepLimit",
    },
    Site {
        path: "src/_6_back/_1_dd.rs",
        text: "let left = scope.iterative::<u64, _, _>(|inner| {",
        kind: "fixpoint",
        budget: "bounds.max_rounds",
        constant: "MAX_ROUNDS",
        diag: "Diag::FixpointLimit",
    },
    Site {
        path: "src/_6_back/_1_dd.rs",
        text: "let (variable, rows) = Variable::new(inner, Product::new(0, 1));",
        kind: "fixpoint",
        budget: "bounds.max_rounds",
        constant: "MAX_ROUNDS",
        diag: "Diag::FixpointLimit",
    },
    Site {
        path: "src/_8_runtime/_3_core.rs",
        text: "for _ in 0..MAX_SERVICE_ROUNDS {",
        kind: "for budget",
        budget: "MAX_SERVICE_ROUNDS",
        constant: "MAX_SERVICE_ROUNDS",
        diag: "Diag::ServiceRounds",
    },
    Site {
        path: "src/bin/v9.rs",
        text: "for _ in 0..CLOSURE_MAX_POPS {",
        kind: "for budget",
        budget: "CLOSURE_MAX_POPS",
        constant: "CLOSURE_MAX_POPS",
        diag: "ClosureLimit",
    },
];

/// A site found in source: `(path, line number, trimmed text, kind)`.
type Found = (String, usize, String, &'static str);

/// Code lines only: a line whose trimmed start is `//` is a comment.
fn is_comment(line: &str) -> bool {
    line.trim_start().starts_with("//")
}

/// `line` with the inside of every `"..."` literal blanked, so SQL like `count(*)` in a string is
/// not a call.
fn code(line: &str) -> String {
    let (mut out, mut quoted, mut escaped) = (String::new(), false, false);
    for c in line.chars() {
        match (quoted, escaped, c) {
            (true, true, _) => escaped = false,
            (true, false, '\\') => escaped = true,
            (_, false, '"') => {
                quoted = !quoted;
                out.push(c);
            }
            (true, false, _) => {}
            (false, _, _) => out.push(c),
        }
    }
    out
}

/// Every `.rs` under `dir`, sorted.
#[allow(clippy::disallowed_methods)]
fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("read src dir")
        .map(|entry| entry.expect("dir entry").path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

#[allow(clippy::disallowed_methods)]
fn read(path: &Path) -> String {
    std::fs::read_to_string(path).expect("read source")
}

/// `(name, first line, last line)` of every fn with a body, 0-based and inclusive; the body is
/// matched by braces from the first `{` after the signature's parens close.
fn fn_spans(lines: &[&str]) -> Vec<(String, usize, usize)> {
    let mut spans = Vec::new();
    for (first, line) in lines.iter().enumerate() {
        if is_comment(line) {
            continue;
        }
        let Some(at) = find_word(line, "fn ") else {
            continue;
        };
        let name: String = line[at + 3..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if name.is_empty() {
            continue;
        }
        let (mut parens, mut braces, mut opened) = (0i32, 0i32, false);
        let mut last = None;
        'lines: for (index, text) in lines.iter().enumerate().skip(first) {
            let start = if index == first { at } else { 0 };
            for c in text[start..].chars() {
                match c {
                    '(' => parens += 1,
                    ')' => parens -= 1,
                    ';' if parens == 0 && !opened => break 'lines,
                    '{' if parens == 0 || opened => {
                        opened = true;
                        braces += 1;
                    }
                    '}' if opened => {
                        braces -= 1;
                        if braces == 0 {
                            last = Some(index);
                            break 'lines;
                        }
                    }
                    _ => {}
                }
            }
        }
        if let Some(last) = last {
            spans.push((name, first, last));
        }
    }
    spans
}

/// Byte offset of `word` in `line` where the char before it is not part of an identifier.
fn find_word(line: &str, word: &str) -> Option<usize> {
    line.match_indices(word).map(|(at, _)| at).find(|at| {
        line[..*at]
            .chars()
            .next_back()
            .is_none_or(|c| !(c.is_ascii_alphanumeric() || c == '_'))
    })
}

/// The innermost fn span holding line `index`.
fn enclosing(spans: &[(String, usize, usize)], index: usize) -> Option<&(String, usize, usize)> {
    spans
        .iter()
        .filter(|(_, first, last)| *first <= index && index <= *last)
        .min_by_key(|(_, first, last)| last - first)
}

/// True when the body (after the signature line) calls `name` bare or on `self`.
fn calls_itself(lines: &[&str], name: &str, first: usize, last: usize) -> bool {
    let call = format!("{name}(");
    lines[first + 1..=last]
        .iter()
        .filter(|line| !is_comment(line))
        .map(|line| code(line))
        .any(|line| {
            line.match_indices(&call).any(|(at, _)| {
                let before = line[..at].chars().next_back();
                let bare = before.is_none_or(|c| {
                    !(c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == ':')
                });
                bare || line[..at].ends_with("self.")
            })
        })
}

/// Every loop, `while`, budget `for`, DD fixpoint and recursive fn in one file's text.
fn scan(path: &str, text: &str) -> Vec<Found> {
    let lines: Vec<&str> = text.lines().collect();
    let mut found = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if is_comment(line) {
            continue;
        }
        let line = code(line);
        let kind = if line.contains("loop {") {
            "loop"
        } else if find_word(&line, "while ").is_some() {
            "while"
        } else if line.contains("for _ in 0..") {
            "for budget"
        } else if line.contains(".iterative::<") || line.contains("Variable::new(") {
            "fixpoint"
        } else {
            continue;
        };
        found.push((
            path.to_string(),
            index + 1,
            lines[index].trim().to_string(),
            kind,
        ));
    }
    for (name, first, last) in fn_spans(&lines) {
        if calls_itself(&lines, &name, first, last) {
            found.push((
                path.to_string(),
                first + 1,
                lines[first].trim().to_string(),
                "recursive fn",
            ));
        }
    }
    found
}

fn crate_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// `(path under the crate root, text)` of every file in `src/**`.
fn src_files() -> Vec<(String, String)> {
    let mut paths = Vec::new();
    sources(&crate_root().join("src"), &mut paths);
    paths
        .iter()
        .map(|path| {
            let relative = path.strip_prefix(crate_root()).expect("under root");
            (relative.display().to_string(), read(path))
        })
        .collect()
}

#[test]
fn the_scanner_finds_each_kind_of_site() {
    let text = "fn spin() {\n    loop {\n        break;\n    }\n}\n\
                fn wait(x: u32) {\n    while x > 0 {}\n}\n\
                // while in a comment\n\
                fn walk(n: u32) -> u32 {\n    if n == 0 { 0 } else { walk(n - 1) }\n}\n\
                fn calls_other() {\n    other_walk(1);\n}\n\
                fn count() -> &'static str {\n    \"SELECT count(*) FROM t\"\n}\n";
    let found: Vec<String> = scan("x.rs", text)
        .into_iter()
        .map(|(_, line, text, kind)| format!("{line} {kind}: {text}"))
        .collect();
    assert_eq!(
        found,
        [
            "2 loop: loop {",
            "7 while: while x > 0 {}",
            "10 recursive fn: fn walk(n: u32) -> u32 {",
        ]
    );
}

#[test]
fn every_loop_while_fixpoint_and_recursion_in_src_is_listed_against_its_budget() {
    let files = src_files();
    let found: Vec<Found> = files
        .iter()
        .flat_map(|(path, text)| scan(path, text))
        .collect();
    let all_src: String = files.iter().map(|(_, text)| text.as_str()).collect();
    let mut problems = Vec::new();
    for (path, line, text, kind) in &found {
        if !SITES
            .iter()
            .any(|site| site.path == path && site.text == text && site.kind == *kind)
        {
            problems.push(format!(
                "{path}:{line} {kind} not in SITES: Site {{ path: {path:?}, text: {text:?}, kind: {kind:?}, .. }}"
            ));
        }
    }
    for site in SITES {
        let Some((_, line, _, _)) = found.iter().find(|(path, _, text, kind)| {
            path == site.path && text == site.text && *kind == site.kind
        }) else {
            problems.push(format!(
                "SITES row gone from src: {}: {}",
                site.path, site.text
            ));
            continue;
        };
        let (_, text) = files
            .iter()
            .find(|(path, _)| path == site.path)
            .expect("site file");
        let lines: Vec<&str> = text.lines().collect();
        let spans = fn_spans(&lines);
        let body = match enclosing(&spans, line - 1) {
            Some((_, first, last)) => lines[*first..=*last].join("\n"),
            None => text.clone(),
        };
        if !body.contains(site.budget) {
            problems.push(format!(
                "{}:{line}: budget {:?} not in the enclosing fn",
                site.path, site.budget
            ));
        }
        if !text.contains(site.diag) {
            problems.push(format!(
                "{}:{line}: diag {:?} not in the file",
                site.path, site.diag
            ));
        }
        let declared = files.iter().find_map(|(path, text)| {
            let lines: Vec<&str> = text.lines().collect();
            lines
                .iter()
                .position(|line| {
                    line.trim_start()
                        .trim_start_matches("pub ")
                        .starts_with(&format!("const {}:", site.constant))
                })
                .map(|index| (path, index, lines))
        });
        match declared {
            None => problems.push(format!("const {} declared nowhere in src", site.constant)),
            Some((path, index, lines)) => {
                if index == 0 || !lines[index - 1].trim_start().starts_with("///") {
                    problems.push(format!(
                        "{path}:{}: const {} has no `///` comment above it",
                        index + 1,
                        site.constant
                    ));
                }
            }
        }
    }
    assert!(!all_src.is_empty());
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn no_eprintln_mutex_or_rwlock_in_src() {
    let mut hits = Vec::new();
    for (path, text) in src_files() {
        for (index, line) in text.lines().enumerate() {
            for banned in ["eprintln!", "Mutex", "RwLock"] {
                if line.contains(banned) {
                    hits.push(format!("{path}:{}: {}", index + 1, line.trim()));
                }
            }
        }
    }
    assert_eq!(hits, Vec::<String>::new());
}
