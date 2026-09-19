//! The bounded-loop law, `CLAUDE.md` 2026-09-18: every `loop {}`, `while`,
//! and self-recursive fn in `src/**` carries an explicit budget and stops
//! with a named diagnostic when the budget is hit. This test walks
//! `src/**/*.rs`, finds each site, and looks inside the enclosing fn body
//! for the budget evidence: a `for _ in 0..` bound, a comparison against a
//! named constant carrying budget vocabulary (`*BUDGET*`, `*LIMIT*`,
//! `*CAP*`, `*MAX*`, `*BOUND*`, `*DEPTH*`), or a repeat check (`== previous`
//! and friends), plus a named diagnostic: a string literal naming the cap
//! (`"..._exceeded"`, `"..._budget"`, ...) or a `tracing::warn`/`error`
//! call. A `while let Some(..)` line that drains its source (`next`, `pop`,
//! `next_entry`, `next_element`) is bounded by construction and needs no
//! further evidence.
//!
//! Sites without a budget today are listed in `UNBUDGETED` as
//! (path, trimmed line text). The list only shrinks: a listed site that
//! gains a budget or vanishes must leave the list in the same commit, and a
//! new unbudgeted site must be listed or budgeted in its own commit.

use std::fs;
use std::path::{Path, PathBuf};

const UNBUDGETED: &[(&str, &str)] = &[
    ("src/_0_read/_2_reader.rs", "loop {"),
    ("src/_0_read/_2_reader.rs", "while self.peek().is_some() {"),
    ("src/_0_read/_2_reader.rs", "while let Some(c) = self.peek() {"),
    ("src/_0_read/_2_reader.rs", "loop {"),
    ("src/_0_read/_2_reader.rs", "while let Some((from, to)) = self.top_colon() {"),
    ("src/_0_read/_2_reader.rs", "loop {"),
    ("src/_0_read/_2_reader.rs", "fn read_list_level(&mut self, top: u32, node_id: TermId, start: Pos) -> Result<TermId, TermId> {"),
    ("src/_0_read/_2_reader.rs", "loop {"),
    ("src/_0_read/_2_reader.rs", "loop {"),
    ("src/_0_read/_4_expand.rs", "fn rewrite_fixpoint("),
    ("src/_0_read/_4_expand.rs", "fn valid_tree(u: &Universe, tree: TermId) -> bool {"),
    ("src/_0_read/_4_expand.rs", "pub fn node_tree(u: &mut Universe, node: TermId) -> TermId {"),
    ("src/_0_read/_4_expand.rs", "fn mint_tree("),
    ("src/_0_read/_5_digest.rs", "while message.len() % 64 != 56 {"),
    ("src/_1_macrotime/_2_protocol.rs", "loop {"),
    ("src/_1_macrotime/_3_rewrite.rs", "fn node(&mut self, node: TermId) -> Vec<TermId> {"),
    ("src/_2_lower/_0_cx.rs", "pub fn replace(&mut self, from: TermId, to: TermId, term: TermId) -> TermId {"),
    ("src/_2_lower/_10_index.rs", "loop {"),
    ("src/_2_lower/_4_promote.rs", "fn alias_terminal_deferred("),
    ("src/_2_lower/_8_express.rs", "pub fn lower_expression(cx: &mut Cx, node: TermId, owner: TermId) -> Lowering {"),
    ("src/_3_check/_2_resolve.rs", "fn path_owner("),
    ("src/_3_check/_2_resolve.rs", "fn resolve_argument(u: &mut Universe, cx: &Cx, argument: TermId) -> Result<TermId, TermId> {"),
    ("src/_3_check/_4_mode.rs", "fn collect_variables(u: &Universe, id: TermId, out: &mut Vec<TermId>) {"),
    ("src/_3_check/_6_strata.rs", "fn arg_of(u: &Universe, term: TermId, vars: &mut Vec<TermId>) -> Arg {"),
    ("src/_4_comptime/_0_load/_1_paths.rs", "while shared < left.len() && shared < right.len() && left[shared] == right[shared] {"),
    ("src/_4_comptime/_0_load/_2_project.rs", "fn path_claims("),
    ("src/_4_comptime/_0_load/_6_source.rs", "fn json_term(u: &mut Universe, value: &Value) -> Option<TermId> {"),
    ("src/_4_comptime/_0_load/_9_openapi.rs", "fn schema("),
    ("src/_4_comptime/_2_rounds.rs", "fn arg_from_term(u: &Universe, term: TermId, vars: &mut Vec<TermId>) -> Arg {"),
    ("src/_4_comptime/_5_finish.rs", "while start < keyed.len() {"),
    ("src/_4_comptime/_5_finish.rs", "while end < keyed.len() && keyed[end].0 == keyed[start].0 {"),
    ("src/_5_reify/_1_rows.rs", "fn argument_value_rows("),
    ("src/_5_reify/_4_emit.rs", "loop {"),
    ("src/_5_reify/_7_sqlite.rs", "while !components.is_empty() {"),
    ("src/_6_eval/_0_term.rs", "loop {"),
    ("src/_6_eval/_0_term.rs", "pub fn cmp(&self, a: TermId, b: TermId) -> Ordering {"),
    ("src/_6_eval/_2_stratify.rs", "loop {"),
    ("src/_6_eval/_2_stratify.rs", "loop {"),
    ("src/_6_eval/_5_evaluate.rs", "while self.trail.len() > to {"),
    ("src/_6_eval/_5_evaluate.rs", "fn value(&self, a: &Arg) -> Option<TermId> {"),
    ("src/_6_eval/_5_evaluate.rs", "fn unify(&mut self, a: &Arg, t: TermId) -> bool {"),
    ("src/_6_eval/_5_evaluate.rs", "fn solve("),
    ("src/_6_eval/_5_evaluate.rs", "fn arg_vars(arg: &Arg) -> Vec<VarId> {"),
    ("src/_6_eval/_5_evaluate.rs", "loop {"),
    ("src/_6_eval/_6_json.rs", "pub fn term_from_json(u: &mut Universe, v: &Value) -> Result<TermId, String> {"),
    ("src/_6_eval/_6_json.rs", "fn walk("),
    ("src/_6_eval/_6_json.rs", "pub fn term_to_json(u: &Universe, id: TermId) -> Value {"),
    ("src/_6_eval/_6_json.rs", "fn arg_from_json(u: &mut Universe, v: &Value, vars: &mut Vec<TermId>) -> Result<Arg, String> {"),
    ("src/_6_eval/_6_json.rs", "fn arg_to_json(u: &Universe, id: TermId) -> Value {"),
    ("src/_9_runtime/_2_reconcile.rs", "while closure.diagnostics.is_empty() && ticks < max_ticks {"),
    ("src/_9_runtime/_3_executors/timer.rs", "loop {"),
];

struct Hit {
    path: String,
    line: usize,
    text: String,
    kind: &'static str,
}

fn is_comment(line: &str) -> bool {
    line.trim_start().starts_with("//")
}

fn collect_sources(directory: &Path, out: &mut Vec<PathBuf>) {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// `(name, first line, body open line, last line)` of every `fn` with a
/// body, 1-based and inclusive. Signatures may span lines; the body span
/// comes from brace matching after the signature's closing parenthesis.
fn fn_spans(lines: &[&str]) -> Vec<(String, usize, usize, usize)> {
    let text = lines.join("\n");
    let bytes = text.as_bytes();
    let mut spans = Vec::new();
    let mut at = 0;
    while let Some(found) = text[at..].find("fn ") {
        let head = at + found;
        let before = if head == 0 { b' ' } else { bytes[head - 1] };
        let boundary_ok = !before.is_ascii_alphanumeric() && before != b'_';
        let mut cursor = head + 3;
        at = head + 3;
        if !boundary_ok {
            continue;
        }
        let name_start = cursor;
        while cursor < bytes.len()
            && (bytes[cursor].is_ascii_alphanumeric() || bytes[cursor] == b'_')
        {
            cursor += 1;
        }
        let name = text[name_start..cursor].to_string();
        if name.is_empty() {
            continue;
        }
        let mut depth = 0i32;
        let mut body_open = None;
        while cursor < bytes.len() {
            match bytes[cursor] {
                b'(' => depth += 1,
                b')' => depth -= 1,
                b'{' if depth == 0 => {
                    body_open = Some(cursor);
                    break;
                }
                b';' if depth == 0 => break,
                _ => {}
            }
            cursor += 1;
        }
        let Some(open) = body_open else {
            continue;
        };
        let mut depth = 0i32;
        let mut close = open;
        while close < bytes.len() {
            if bytes[close] == b'{' {
                depth += 1;
            } else if bytes[close] == b'}' {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            close += 1;
        }
        let first_line = text[..head].matches('\n').count() + 1;
        let open_line = text[..open].matches('\n').count() + 1;
        let last_line = text[..close.min(bytes.len() - 1)].matches('\n').count() + 1;
        spans.push((name, first_line, open_line, last_line));
        at = close;
    }
    spans
}

/// The fn body text after the signature: from just past the `{` that opens
/// the body, so the definition line never certifies its own recursion.
/// Whole-line comments are dropped.
fn body_after_signature(lines: &[&str], open_line: usize, last_line: usize) -> String {
    let open_index = open_line - 1;
    let mut body: Vec<&str> = Vec::new();
    if let Some(position) = lines[open_index].find('{') {
        body.push(lines[open_index][position + 1..].trim());
    }
    body.extend(
        lines[open_line..last_line.min(lines.len())]
            .iter()
            .filter(|line| !is_comment(line))
            .copied(),
    );
    body.join("\n")
}

/// True when `body` calls `name` as itself: a bare `name(` (the preceding
/// character is not an identifier character, `.`, or `:`) or a method call
/// on `self`. Substrings of longer names (`finish_compile(` for `compile(`)
/// and calls on other types (`Vec::new(`, `x.len(`) do not count.
fn calls_itself(body: &str, name: &str) -> bool {
    let call = format!("{name}(");
    let mut search = 0;
    while let Some(found) = body[search..].find(&call) {
        let start = search + found;
        let before = body[..start].chars().next_back();
        let bare =
            before.is_none_or(|c| !(c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == ':'));
        let on_self = body[..start].ends_with("self.");
        if bare || on_self {
            return true;
        }
        search = start + call.len();
    }
    false
}

fn has_budget_evidence(body: &str) -> bool {
    if body.contains("for _ in 0..") {
        return true;
    }
    for token in body.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
        let vocabulary = ["BUDGET", "LIMIT", "CAP", "MAX", "BOUND", "DEPTH"];
        let carries = token.len() > 3
            && token
                .chars()
                .all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit())
            && vocabulary.iter().any(|word| token.contains(word));
        if carries {
            return true;
        }
    }
    let repeat_checks = [
        "== previous",
        "previous ==",
        "== prev",
        "prev ==",
        "changed",
        "settled",
        "stable",
    ];
    repeat_checks.iter().any(|mark| body.contains(mark))
}

fn has_named_diagnostic(body: &str) -> bool {
    if body.contains("tracing::warn") || body.contains("tracing::error") {
        return true;
    }
    let named = ["budget", "cap", "limit", "exceeded", "unbounded"];
    for segment in body.split('"').step_by(2) {
        if named.iter().any(|word| segment.contains(word)) {
            return true;
        }
    }
    false
}

fn is_drain_line(line: &str) -> bool {
    let drains = [".next(", ".pop(", ".next_entry(", ".next_element("];
    (line.contains("while let Some(") || line.contains("while let Ok("))
        && drains.iter().any(|mark| line.contains(mark))
}

fn enclosing_body(spans: &[(String, usize, usize, usize)], line: usize, lines: &[&str]) -> String {
    for (_name, first, open_line, last) in spans {
        if line >= *first && line <= *last {
            return body_after_signature(lines, *open_line, *last);
        }
    }
    lines.join("\n")
}

#[test]
fn every_loop_and_recursion_in_src_carries_its_budget() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    collect_sources(&root, &mut files);
    files.sort();
    assert!(files.len() > 50, "the walk found no sources under src/");

    let mut hits: Vec<Hit> = Vec::new();
    for file in &files {
        let relative = file
            .strip_prefix(env!("CARGO_MANIFEST_DIR"))
            .unwrap()
            .display()
            .to_string()
            .replace('\\', "/");
        let text = fs::read_to_string(file).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        let spans = fn_spans(&lines);

        for (index, line) in lines.iter().enumerate() {
            let number = index + 1;
            if is_comment(line) {
                continue;
            }
            let is_loop = line.contains("loop {");
            let is_while = line.contains("while ") && !is_drain_line(line);
            if !is_loop && !is_while {
                continue;
            }
            let body = enclosing_body(&spans, number, &lines);
            let budgeted = has_budget_evidence(&body) && has_named_diagnostic(&body);
            if !budgeted {
                hits.push(Hit {
                    path: relative.clone(),
                    line: number,
                    text: line.trim().to_string(),
                    kind: if is_loop { "loop" } else { "while" },
                });
            }
        }

        for (name, first, open_line, last) in &spans {
            let rest = body_after_signature(&lines, *open_line, *last);
            if !calls_itself(&rest, name) {
                continue;
            }
            let budgeted = has_budget_evidence(&rest) && has_named_diagnostic(&rest);
            if !budgeted {
                hits.push(Hit {
                    path: relative.clone(),
                    line: *first,
                    text: lines[first - 1].trim().to_string(),
                    kind: "recursive fn",
                });
            }
        }
    }

    hits.sort_by(|a, b| (&a.path, a.line).cmp(&(&b.path, b.line)));

    let mut unbudgeted_now: Vec<String> = Vec::new();
    for hit in &hits {
        let listed = UNBUDGETED
            .iter()
            .any(|(path, text)| *path == hit.path && *text == hit.text);
        if !listed {
            unbudgeted_now.push(format!(
                "    ({:?}, {:?}), // {}:{} {}",
                hit.path, hit.text, hit.path, hit.line, hit.kind
            ));
        }
    }
    let mut listed_but_gone: Vec<String> = Vec::new();
    for (path, text) in UNBUDGETED {
        let alive = hits
            .iter()
            .any(|hit| hit.path == *path && hit.text == *text);
        if !alive {
            listed_but_gone.push(format!("    ({path:?}, {text:?}),"));
        }
    }

    let mut failure = String::new();
    if !unbudgeted_now.is_empty() {
        failure.push_str(&format!(
            "{} site(s) with no budget and no UNBUDGETED entry:\n",
            unbudgeted_now.len()
        ));
        failure.push_str(&unbudgeted_now.join("\n"));
        failure.push('\n');
    }
    if !listed_but_gone.is_empty() {
        failure.push_str(&format!(
            "{} UNBUDGETED entries no longer exist unbudgeted; shrink the list:\n",
            listed_but_gone.len()
        ));
        failure.push_str(&listed_but_gone.join("\n"));
        failure.push('\n');
    }
    assert!(
        failure.is_empty(),
        "bounded-loop law violations:\n{failure}\nUNBUDGETED carries {} entries",
        UNBUDGETED.len()
    );
}
