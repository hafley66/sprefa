//! The CFG plane: the whole edge set of one function per language, asserted
//! exactly, plus the receipt that kotlin's jump/exit split needs a token read.

use std::collections::BTreeSet;

use sprefa_extract::{
    build_cfg, cfg_facts, dispatch, flatten_cfg, CfgRole, FamilyMask, FamilyTag, FlatFact,
    RoleRule, SpanOut, KOTLIN_ROLES,
};

const RUST_SOURCE: &str = r#"fn walk(items: Vec<i32>) -> i32 {
    let mut total = 0;
    for item in items {
        if item < 0 {
            continue;
        }
        if item > 100 {
            break;
        }
        total += item;
    }
    if total == 0 {
        return -1;
    }
    total
}
"#;

const GO_SOURCE: &str = r#"package p

func walk(items []int) int {
	total := 0
	for _, item := range items {
		if item < 0 {
			continue
		}
		if item > 100 {
			break
		}
		total += item
	}
	if total == 0 {
		return -1
	}
	return total
}
"#;

const TS_SOURCE: &str = r#"function walk(items: number[]): number {
  let total = 0;
  for (const item of items) {
    if (item < 0) {
      continue;
    }
    if (item > 100) {
      break;
    }
    total += item;
  }
  if (total === 0) {
    return -1;
  }
  return total;
}
"#;

const KOTLIN_SOURCE: &str = r#"fun walk(items: List<Int>): Int {
    var total = 0
    for (item in items) {
        if (item < 0) {
            continue
        }
        if (item > 100) {
            break
        }
        total += item
    }
    when (total) {
        0 -> return -1
        else -> throw RuntimeException("x")
    }
}
"#;

const TS_DO_SOURCE: &str = r#"function spin(limit: number): number {
  let seen = 0;
  do {
    seen += 1;
  } while (seen < limit);
  return seen;
}
"#;

/// One node rendered as `kind(first 28 chars of its own source text)`.
fn label(source: &str, kind: &str, span: SpanOut) -> String {
    let text = &source[span.start as usize..span.end as usize];
    let head: String = text
        .split('\n')
        .next()
        .unwrap_or("")
        .trim()
        .chars()
        .take(28)
        .collect();
    format!("{kind}({head})")
}

fn cfg_edges(path: &str, source: &str) -> BTreeSet<String> {
    let mut set = BTreeSet::new();
    for fact in cfg_facts(path, source.as_bytes()) {
        if let FlatFact::Edge {
            family: FamilyTag::Cfg,
            kind,
            from,
            from_kind,
            to,
            to_kind,
            ..
        } = fact
        {
            set.insert(format!(
                "{} -{kind}-> {}",
                label(source, from_kind.as_deref().unwrap_or(""), from),
                label(source, to_kind.as_deref().unwrap_or(""), to),
            ));
        }
    }
    set
}

fn expect(actual: BTreeSet<String>, wanted: &[&str]) {
    let wanted: BTreeSet<String> = wanted.iter().map(|line| line.to_string()).collect();
    assert_eq!(
        actual,
        wanted,
        "\nmissing: {:?}\nextra: {:?}",
        wanted.difference(&actual).collect::<Vec<_>>(),
        actual.difference(&wanted).collect::<Vec<_>>()
    );
}

#[test]
fn rust_if_loop_break_return_edge_set() {
    expect(
        cfg_edges("walk.rs", RUST_SOURCE),
        &[
            "entry(fn walk(items: Vec<i32>) -> ) -next-> stmt(let mut total = 0;)",
            "stmt(let mut total = 0;) -next-> loop(for item in items {)",
            "loop(for item in items {) -arm-> branch(if item < 0 {)",
            "branch(if item < 0 {) -arm-> jump(continue)",
            "jump(continue) -jump-> loop(for item in items {)",
            "branch(if item < 0 {) -next-> branch(if item > 100 {)",
            "branch(if item > 100 {) -arm-> jump(break)",
            "branch(if item > 100 {) -next-> stmt(total += item;)",
            "stmt(total += item;) -next-> loop(for item in items {)",
            "loop(for item in items {) -next-> branch(if total == 0 {)",
            "jump(break) -jump-> branch(if total == 0 {)",
            "branch(if total == 0 {) -arm-> ret(return -1)",
            "ret(return -1) -exit-> exit(fn walk(items: Vec<i32>) -> )",
            "branch(if total == 0 {) -next-> stmt(total)",
            "stmt(total) -exit-> exit(fn walk(items: Vec<i32>) -> )",
        ],
    );
}

#[test]
fn go_if_loop_break_return_edge_set() {
    expect(
        cfg_edges("walk.go", GO_SOURCE),
        &[
            "entry(func walk(items []int) int {) -next-> stmt(total := 0)",
            "stmt(total := 0) -next-> loop(for _, item := range items {)",
            "loop(for _, item := range items {) -arm-> branch(if item < 0 {)",
            "branch(if item < 0 {) -arm-> jump(continue)",
            "jump(continue) -jump-> loop(for _, item := range items {)",
            "branch(if item < 0 {) -next-> branch(if item > 100 {)",
            "branch(if item > 100 {) -arm-> jump(break)",
            "branch(if item > 100 {) -next-> stmt(total += item)",
            "stmt(total += item) -next-> loop(for _, item := range items {)",
            "loop(for _, item := range items {) -next-> branch(if total == 0 {)",
            "jump(break) -jump-> branch(if total == 0 {)",
            "branch(if total == 0 {) -arm-> ret(return -1)",
            "ret(return -1) -exit-> exit(func walk(items []int) int {)",
            "branch(if total == 0 {) -next-> ret(return total)",
            "ret(return total) -exit-> exit(func walk(items []int) int {)",
        ],
    );
}

#[test]
fn ts_if_loop_break_return_edge_set() {
    expect(
        cfg_edges("walk.ts", TS_SOURCE),
        &[
            "entry(function walk(items: number[) -next-> stmt(let total = 0;)",
            "stmt(let total = 0;) -next-> loop(for (const item of items) {)",
            "loop(for (const item of items) {) -arm-> branch(if (item < 0) {)",
            "branch(if (item < 0) {) -arm-> jump(continue;)",
            "jump(continue;) -jump-> loop(for (const item of items) {)",
            "branch(if (item < 0) {) -next-> branch(if (item > 100) {)",
            "branch(if (item > 100) {) -arm-> jump(break;)",
            "branch(if (item > 100) {) -next-> stmt(total += item;)",
            "stmt(total += item;) -next-> loop(for (const item of items) {)",
            "loop(for (const item of items) {) -next-> branch(if (total === 0) {)",
            "jump(break;) -jump-> branch(if (total === 0) {)",
            "branch(if (total === 0) {) -arm-> ret(return -1;)",
            "ret(return -1;) -exit-> exit(function walk(items: number[)",
            "branch(if (total === 0) {) -next-> ret(return total;)",
            "ret(return total;) -exit-> exit(function walk(items: number[)",
        ],
    );
}

/// The loop body is the LAST child everywhere except a do-loop, where it is the
/// first: the arm here enters the block and the condition rides the header.
#[test]
fn ts_do_while_body_is_the_first_child() {
    expect(
        cfg_edges("spin.ts", TS_DO_SOURCE),
        &[
            "entry(function spin(limit: number)) -next-> stmt(let seen = 0;)",
            "stmt(let seen = 0;) -next-> loop(do {)",
            "loop(do {) -arm-> stmt({)",
            "stmt({) -next-> loop(do {)",
            "loop(do {) -next-> ret(return seen;)",
            "ret(return seen;) -exit-> exit(function spin(limit: number))",
        ],
    );
}

#[test]
fn kotlin_when_and_jump_expression_edge_set() {
    expect(
        cfg_edges("walk.kt", KOTLIN_SOURCE),
        &[
            "entry(fun walk(items: List<Int>): ) -next-> stmt(var total = 0)",
            "stmt(var total = 0) -next-> loop(for (item in items) {)",
            "loop(for (item in items) {) -arm-> branch(if (item < 0) {)",
            "branch(if (item < 0) {) -arm-> jump(continue)",
            "jump(continue) -jump-> loop(for (item in items) {)",
            "branch(if (item < 0) {) -next-> branch(if (item > 100) {)",
            "branch(if (item > 100) {) -arm-> jump(break)",
            "branch(if (item > 100) {) -next-> stmt(total += item)",
            "stmt(total += item) -next-> loop(for (item in items) {)",
            "loop(for (item in items) {) -next-> branch(when (total) {)",
            "jump(break) -jump-> branch(when (total) {)",
            "branch(when (total) {) -arm-> stmt(0)",
            "stmt(0) -next-> ret(return -1)",
            "ret(return -1) -exit-> exit(fun walk(items: List<Int>): )",
            "branch(when (total) {) -arm-> ret(throw RuntimeException(\"x\"))",
            "ret(throw RuntimeException(\"x\")) -exit-> exit(fun walk(items: List<Int>): )",
            "branch(when (total) {) -exit-> exit(fun walk(items: List<Int>): )",
        ],
    );
}

/// FAIL RECEIPT: a kind-name-only table cannot split kotlin's jump_expression.
/// The stub row below is the whole difference, and it misclassifies return.
#[test]
fn kotlin_keyword_read_splits_return_from_break() {
    const STUB_ROLES: &[(&str, RoleRule)] = &[
        ("function_declaration", RoleRule::Fixed(CfgRole::Callable)),
        ("if_expression", RoleRule::Fixed(CfgRole::Branch)),
        ("when_expression", RoleRule::Fixed(CfgRole::Branch)),
        ("for_statement", RoleRule::Fixed(CfgRole::Loop)),
        ("jump_expression", RoleRule::Fixed(CfgRole::Jump)),
    ];
    let mut mask = FamilyMask::NONE;
    mask.cst = true;
    let out = dispatch("walk.kt", KOTLIN_SOURCE.as_bytes(), mask).expect("a Source matches .kt");
    let cst = out.cst.as_ref().expect("the cst mask was on");

    let stub = build_cfg(STUB_ROLES, cst, &out.strings, KOTLIN_SOURCE.as_bytes());
    let stub_kinds = node_kinds(&stub, KOTLIN_SOURCE, "return -1");
    assert_eq!(
        stub_kinds,
        vec!["jump".to_string()],
        "the stub table reads the kind name only, so return lands in the break class"
    );
    assert!(
        !has_exit_edge(&stub, KOTLIN_SOURCE, "return -1"),
        "the stub's return never reaches the callable's exit node"
    );

    let real = build_cfg(KOTLIN_ROLES, cst, &out.strings, KOTLIN_SOURCE.as_bytes());
    assert_eq!(
        node_kinds(&real, KOTLIN_SOURCE, "return -1"),
        vec!["ret".to_string()],
        "the leading-keyword row reads `return` and classifies it as an exit"
    );
    assert!(has_exit_edge(&real, KOTLIN_SOURCE, "return -1"));
    assert_eq!(
        node_kinds(&real, KOTLIN_SOURCE, "break"),
        vec!["jump".to_string()],
        "the same row reads `break` and leaves it a loop jump"
    );
}

#[test]
fn a_language_with_no_kind_role_rows_emits_no_cfg() {
    assert!(cfg_facts("walk.md", b"# walk\n\nreturn 1\n").is_empty());
}

fn node_kinds(
    bundle: &sprefa_extract::FamilyBundle<sprefa_extract::CfgF>,
    source: &str,
    text: &str,
) -> Vec<String> {
    flatten_cfg(bundle)
        .into_iter()
        .filter_map(|fact| match fact {
            FlatFact::Node { span, kind, .. } if slice(source, span) == text => Some(kind),
            _ => None,
        })
        .collect()
}

fn has_exit_edge(
    bundle: &sprefa_extract::FamilyBundle<sprefa_extract::CfgF>,
    source: &str,
    text: &str,
) -> bool {
    flatten_cfg(bundle).into_iter().any(|fact| {
        matches!(
            &fact,
            FlatFact::Edge { kind, from, to_kind, .. }
                if kind == "exit"
                    && slice(source, *from) == text
                    && to_kind.as_deref() == Some("exit")
        )
    })
}

fn slice(source: &str, span: SpanOut) -> &str {
    &source[span.start as usize..span.end as usize]
}
