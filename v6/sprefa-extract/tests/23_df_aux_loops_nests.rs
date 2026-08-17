//! `df_loop` / `df_nest` / `df_allocates` across the four df walks. Expected
//! values are hand-derived from `tests/fixtures/df_loops/sample.*`, never copied
//! from the extractor's output: every row below is read off the fixture by eye,
//! so a test written by pasting extractor output is a failed deliverable.
//!
//! No v5 oracle exists for these three facets. `cat tests/fixtures/*/*.v5.jsonl`
//! carries ZERO loop_over / nest / allocates rows and the four golden fixtures
//! contain no loops at all, so the four fixtures here are new and the grading is
//! by hand.
//!
//! Each fixture holds the same program: a two-deep nest with a call, a
//! constructor and a method call inside the inner loop, a collection built
//! before the loops, a `while` with no loop variable, and a call after every
//! loop closes. Rows are matched by the FIRST LINE of each span's source slice,
//! which is what a reader can check against the fixture.
//!
//! Three shapes this pins:
//!   1. depth 1 is the OUTERMOST enclosing loop, not the innermost.
//!   2. `new`/constructor nodes nest exactly like calls (a ctor in a loop
//!      allocates per iteration).
//!   3. `allocates` is RUST ONLY, and only the fn holding the allocator call
//!      gets the row.

use std::collections::BTreeSet;

use sprefa_extract::{dispatch, flatten, FamilyMask, FlatFact};

struct Case {
    name: &'static str,
    path: &'static str,
    fixture: &'static [u8],
    /// (loop header line, loop variable, iterated collection)
    loops: &'static [(&'static str, Option<&'static str>, Option<&'static str>)],
    /// (call/new first line, enclosing loop header line, depth)
    nests: &'static [(&'static str, &'static str, u32)],
    /// callable header line, one row per allocating fn
    allocates: &'static [&'static str],
    /// call/new first lines that sit outside every loop
    unnested: &'static [&'static str],
}

const CASES: &[Case] = &[
    Case {
        name: "rust",
        path: "tests/fixtures/df_loops/sample.rs",
        fixture: include_bytes!("fixtures/df_loops/sample.rs"),
        loops: &[
            ("for row in rows {", Some("row"), Some("rows")),
            ("for col in cols {", Some("col"), Some("cols")),
            ("while total < limit {", None, None),
        ],
        nests: &[
            ("add(*row, *col)", "for row in rows {", 1),
            ("add(*row, *col)", "for col in cols {", 2),
            ("Cell {", "for row in rows {", 1),
            ("Cell {", "for col in cols {", 2),
            ("push", "for row in rows {", 1),
            ("push", "for col in cols {", 2),
            ("add(total, 1)", "while total < limit {", 1),
        ],
        allocates: &["allocating(rows: &[i64], cols: &[i64]) -> Vec<Cell> {"],
        unnested: &["Vec::new()", "add(total, 0)"],
    },
    Case {
        name: "ts",
        path: "tests/fixtures/df_loops/sample.ts",
        fixture: include_bytes!("fixtures/df_loops/sample.ts"),
        loops: &[
            ("for (const row of rows) {", Some("row"), Some("rows")),
            ("for (const col of cols) {", Some("col"), Some("cols")),
            ("while (total < limit) {", None, None),
        ],
        nests: &[
            ("add(row, col)", "for (const row of rows) {", 1),
            ("add(row, col)", "for (const col of cols) {", 2),
            ("new Cell(add(row, col))", "for (const row of rows) {", 1),
            ("new Cell(add(row, col))", "for (const col of cols) {", 2),
            ("out.push(cell)", "for (const row of rows) {", 1),
            ("out.push(cell)", "for (const col of cols) {", 2),
            ("add(total, 1)", "while (total < limit) {", 1),
        ],
        allocates: &[],
        unnested: &["[]", "add(total, 0)"],
    },
    Case {
        name: "go",
        path: "tests/fixtures/df_loops/sample.go",
        fixture: include_bytes!("fixtures/df_loops/sample.go"),
        loops: &[
            ("for _, row := range rows {", Some("row"), Some("rows")),
            ("for _, col := range cols {", Some("col"), Some("cols")),
            ("for total < limit {", None, None),
        ],
        nests: &[
            ("Add(row, col)", "for _, row := range rows {", 1),
            ("Add(row, col)", "for _, col := range cols {", 2),
            (
                "Cell{Value: Add(row, col)}",
                "for _, row := range rows {",
                1,
            ),
            (
                "Cell{Value: Add(row, col)}",
                "for _, col := range cols {",
                2,
            ),
            ("append(out, cell)", "for _, row := range rows {", 1),
            ("append(out, cell)", "for _, col := range cols {", 2),
            ("Add(total, 1)", "for total < limit {", 1),
        ],
        allocates: &[],
        unnested: &["[]Cell{}", "Add(total, 0)"],
    },
    Case {
        name: "kotlin",
        path: "tests/fixtures/df_loops/sample.kt",
        fixture: include_bytes!("fixtures/df_loops/sample.kt"),
        loops: &[
            ("for (row in rows) {", Some("row"), Some("rows")),
            ("for (col in cols) {", Some("col"), Some("cols")),
            ("while (total < limit) {", None, None),
        ],
        nests: &[
            ("add(row, col)", "for (row in rows) {", 1),
            ("add(row, col)", "for (col in cols) {", 2),
            ("Cell(add(row, col))", "for (row in rows) {", 1),
            ("Cell(add(row, col))", "for (col in cols) {", 2),
            ("out.add(cell)", "for (row in rows) {", 1),
            ("out.add(cell)", "for (col in cols) {", 2),
            ("add(total, 1)", "while (total < limit) {", 1),
        ],
        allocates: &[],
        unnested: &["mutableListOf<Cell>()", "add(total, 0)"],
    },
];

/// First line of the fixture slice `[start, end)`. Spans are half-open byte
/// ranges; a multi-line construct is identified by its opening line.
fn head(src: &str, start: u32, end: u32) -> String {
    src[start as usize..end as usize]
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .to_string()
}

struct Rows {
    loops: Vec<(String, Option<String>, Option<String>)>,
    nests: BTreeSet<(String, String, u32)>,
    allocates: BTreeSet<String>,
    nested_calls: BTreeSet<String>,
}

fn rows(case: &Case) -> Rows {
    let src = std::str::from_utf8(case.fixture).expect("fixture is UTF-8");
    let out = dispatch(case.path, case.fixture, FamilyMask::ALL).expect("a Source matches");
    let facts = flatten(&out);
    let mut loops = Vec::new();
    let mut nests = BTreeSet::new();
    let mut allocates = BTreeSet::new();
    let mut nested_calls = BTreeSet::new();
    for fact in &facts {
        match fact {
            FlatFact::DfLoop {
                span,
                var,
                collection,
                ..
            } => loops.push((
                head(src, span.start, span.end),
                var.clone(),
                collection.clone(),
            )),
            FlatFact::DfNest {
                call,
                loop_span,
                depth,
                ..
            } => {
                let call_head = head(src, call.start, call.end);
                nested_calls.insert(call_head.clone());
                nests.insert((call_head, head(src, loop_span.start, loop_span.end), *depth));
            }
            FlatFact::DfAllocates { owner, .. } => {
                allocates.insert(head(src, owner.start, owner.end));
            }
            _ => {}
        }
    }
    Rows {
        loops,
        nests,
        allocates,
        nested_calls,
    }
}

#[test]
fn loop_rows_carry_span_var_and_collection() {
    for case in CASES {
        let got = rows(case);
        let want: Vec<(String, Option<String>, Option<String>)> = case
            .loops
            .iter()
            .map(|(header, var, collection)| {
                (
                    (*header).to_string(),
                    var.map(str::to_string),
                    collection.map(str::to_string),
                )
            })
            .collect();
        let mut got_sorted = got.loops.clone();
        got_sorted.sort();
        let mut want_sorted = want.clone();
        want_sorted.sort();
        assert_eq!(
            got_sorted, want_sorted,
            "[{}] df_loop rows (span head, var, collection)",
            case.name
        );
    }
}

#[test]
fn nest_rows_rank_the_enclosing_loops_outermost_first() {
    for case in CASES {
        let got = rows(case);
        let want: BTreeSet<(String, String, u32)> = case
            .nests
            .iter()
            .map(|(call, enclosing, depth)| ((*call).to_string(), (*enclosing).to_string(), *depth))
            .collect();
        assert_eq!(
            got.nests, want,
            "[{}] df_nest rows (call head, loop head, depth)",
            case.name
        );
    }
}

#[test]
fn calls_outside_every_loop_have_no_nest_row() {
    for case in CASES {
        let got = rows(case);
        for call in case.unnested {
            assert!(
                !got.nested_calls.contains(*call),
                "[{}] `{call}` sits outside every loop yet carries a df_nest row",
                case.name
            );
        }
    }
}

#[test]
fn allocates_is_rust_only_and_names_the_allocating_fn() {
    for case in CASES {
        let got = rows(case);
        let want: BTreeSet<String> = case.allocates.iter().map(|s| (*s).to_string()).collect();
        assert_eq!(
            got.allocates, want,
            "[{}] df_allocates rows (callable head)",
            case.name
        );
    }
}
