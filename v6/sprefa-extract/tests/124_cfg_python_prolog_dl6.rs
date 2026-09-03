//! The CFG plane for python, prolog and dl6: the whole edge set of each fixture
//! under `tests/fixtures/cfg/`, hand-derived from the source, plus the receipt
//! that the three census fixtures now carry cfg rows.

use std::collections::BTreeSet;

use sprefa_extract::{cfg_facts, FamilyTag, FlatFact, SpanOut};

fn fixture(name: &str) -> (String, String) {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    let source = std::fs::read_to_string(&path).expect("fixture on disk");
    (path, source)
}

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

fn cfg_node_kinds(path: &str, source: &str) -> BTreeSet<String> {
    cfg_facts(path, source.as_bytes())
        .into_iter()
        .filter_map(|fact| match fact {
            FlatFact::Node {
                family: FamilyTag::Cfg,
                kind,
                ..
            } => Some(kind),
            _ => None,
        })
        .collect()
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

/// `walk`: if/elif/else inside a for, a while, try/except with a bare raise,
/// match with a return arm, then a return. `emit`: a yield inside a for.
/// The if/elif/else keeps its branch nodes in the exit set (the CST cannot
/// say the arms exhaust), so `branch(if ...) -next-> loop` is expected.
#[test]
fn python_if_elif_for_while_try_match_edge_set() {
    let (path, source) = fixture("cfg/walk.py");
    expect(
        cfg_edges(&path, &source),
        &[
            "entry(def walk(items):) -next-> stmt(total = 0)",
            "stmt(total = 0) -next-> loop(for item in items:)",
            "loop(for item in items:) -arm-> branch(if item < 0:)",
            "branch(if item < 0:) -arm-> jump(continue)",
            "jump(continue) -jump-> loop(for item in items:)",
            "branch(if item < 0:) -arm-> branch(elif item > 100:)",
            "branch(elif item > 100:) -arm-> jump(break)",
            "branch(if item < 0:) -arm-> stmt(else:)",
            "branch(if item < 0:) -next-> loop(for item in items:)",
            "branch(elif item > 100:) -next-> loop(for item in items:)",
            "stmt(else:) -next-> loop(for item in items:)",
            "loop(for item in items:) -next-> loop(while total > 10:)",
            "jump(break) -jump-> loop(while total > 10:)",
            "loop(while total > 10:) -arm-> stmt(total -= 1)",
            "stmt(total -= 1) -next-> loop(while total > 10:)",
            "loop(while total > 10:) -next-> branch(try:)",
            "branch(try:) -next-> stmt(check(total))",
            "branch(try:) -arm-> stmt(ValueError)",
            "stmt(ValueError) -next-> ret(raise)",
            "ret(raise) -exit-> exit(def walk(items):)",
            "stmt(check(total)) -next-> branch(match total:)",
            "branch(match total:) -arm-> stmt(0)",
            "stmt(0) -next-> ret(return -1)",
            "ret(return -1) -exit-> exit(def walk(items):)",
            "branch(match total:) -arm-> stmt(case _:)",
            "stmt(case _:) -next-> ret(return total)",
            "ret(return total) -exit-> exit(def walk(items):)",
            "entry(def emit(items):) -next-> loop(for each in items:)",
            "loop(for each in items:) -arm-> ret(yield each)",
            "ret(yield each) -exit-> exit(def emit(items):)",
            "loop(for each in items:) -next-> ret(return)",
            "ret(return) -exit-> exit(def emit(items):)",
        ],
    );
}

/// The match arms sit under one `block` child of `match_statement`; each arm
/// is entered from the match node, never from the previous arm.
#[test]
fn python_match_arms_are_not_a_sequence() {
    let (path, source) = fixture("cfg/walk.py");
    let edges = cfg_edges(&path, &source);
    assert!(!edges.iter().any(|edge| edge.starts_with("ret(return -1) -next->")));
    assert!(!edges.iter().any(|edge| edge.ends_with("-next-> stmt(case _:)")));
}

/// Clause 1 is a fact and mints nothing. Clause 2: `(C1 -> T1 ; C2 -> T2 ; E)`
/// with a cut in T2, then `\+`, then the recursive call. Clause 3: a plain
/// disjunction. The `;` of an if-then-else mints no node of its own.
#[test]
fn prolog_if_then_else_cut_negation_recursion_edge_set() {
    let (path, source) = fixture("cfg/walk.pl");
    expect(
        cfg_edges(&path, &source),
        &[
            "entry(walk([Item | Rest], Acc, Tot) -next-> branch(Item < 0)",
            "branch(Item < 0) -arm-> stmt(Next = Acc)",
            "branch(Item < 0) -arm-> branch(Item > 100)",
            "branch(Item > 100) -arm-> stmt(!)",
            "stmt(!) -next-> stmt(Next = 100)",
            "branch(Item > 100) -arm-> stmt(Next is Acc + Item)",
            "stmt(Next = Acc) -next-> branch(\\+ skip(Item))",
            "stmt(Next = 100) -next-> branch(\\+ skip(Item))",
            "stmt(Next is Acc + Item) -next-> branch(\\+ skip(Item))",
            "branch(\\+ skip(Item)) -arm-> stmt(skip(Item))",
            "branch(\\+ skip(Item)) -next-> stmt(walk(Rest, Next, Total))",
            "stmt(skip(Item)) -next-> stmt(walk(Rest, Next, Total))",
            "stmt(walk(Rest, Next, Total)) -jump-> entry(walk([Item | Rest], Acc, Tot)",
            "stmt(walk(Rest, Next, Total)) -exit-> exit(walk([Item | Rest], Acc, Tot)",
            "entry(pick(Item) :- small(Item) ; ) -next-> branch(small(Item) ; big(Item))",
            "branch(small(Item) ; big(Item)) -arm-> stmt(small(Item))",
            "branch(small(Item) ; big(Item)) -arm-> stmt(big(Item))",
            "stmt(small(Item)) -exit-> exit(pick(Item) :- small(Item) ; )",
            "stmt(big(Item)) -exit-> exit(pick(Item) :- small(Item) ; )",
        ],
    );
}

/// The rel declarations mint nothing. A rule runs its body, folds its head
/// aggregate, then produces its head; a fact or a `?` query is head only.
#[test]
fn dl6_rule_negation_aggregate_query_edge_set() {
    let (path, source) = fixture("cfg/walk.dl6");
    expect(
        cfg_edges(&path, &source),
        &[
            "entry(edge(\"a\", \"b\").) -next-> stmt(edge(\"a\", \"b\"))",
            "stmt(edge(\"a\", \"b\")) -exit-> exit(edge(\"a\", \"b\").)",
            "entry(path(A, B) <- edge(A, B).) -next-> stmt(edge(A, B))",
            "stmt(edge(A, B)) -next-> stmt(path(A, B))",
            "stmt(path(A, B)) -exit-> exit(path(A, B) <- edge(A, B).)",
            "entry(path(X, Z) <- edge(X, Y), no) -next-> stmt(edge(X, Y))",
            "stmt(edge(X, Y)) -next-> branch(not(blocked(Y)))",
            "branch(not(blocked(Y))) -arm-> stmt(blocked(Y))",
            "branch(not(blocked(Y))) -next-> stmt(path(Y, Z))",
            "stmt(blocked(Y)) -next-> stmt(path(Y, Z))",
            "stmt(path(Y, Z)) -jump-> entry(path(X, Z) <- edge(X, Y), no)",
            "stmt(path(Y, Z)) -next-> stmt(path(X, Z))",
            "stmt(path(X, Z)) -exit-> exit(path(X, Z) <- edge(X, Y), no)",
            "entry(fan(S, count(D)) <- edge(S, ) -next-> stmt(edge(S, D))",
            "stmt(edge(S, D)) -next-> loop(count(D))",
            "loop(count(D)) -next-> stmt(fan(S, count(D)))",
            "stmt(fan(S, count(D))) -exit-> exit(fan(S, count(D)) <- edge(S, )",
            "entry(? path(X, Y).) -next-> stmt(path(X, Y))",
            "stmt(path(X, Y)) -exit-> exit(? path(X, Y).)",
        ],
    );
}

/// The census fixtures the plan names: 0 cfg rows before this table, more
/// than 0 after, with the node kinds each construct set implies.
#[test]
fn census_fixtures_carry_cfg_rows() {
    let (path, source) = fixture("tsi/probe_graph.py");
    let kinds = cfg_node_kinds(&path, &source);
    assert!(kinds.contains("entry") && kinds.contains("exit") && kinds.contains("ret"));

    let (path, source) = fixture("prolog/corpus_2_meta_use.pl");
    let kinds = cfg_node_kinds(&path, &source);
    assert_eq!(
        kinds.into_iter().collect::<Vec<_>>(),
        vec!["entry", "exit", "stmt"],
        "one `:-` clause of four conjoined meta-call goals"
    );
    let (path, source) = fixture("prolog/corpus_2_meta_use.pl");
    assert_eq!(
        cfg_edges(&path, &source).len(),
        5,
        "entry -> 4 goals in sequence -> exit"
    );

    let (path, source) = fixture("dl6/2_callee.dl6");
    let kinds = cfg_node_kinds(&path, &source);
    assert_eq!(
        kinds.into_iter().collect::<Vec<_>>(),
        vec!["entry", "exit", "stmt"],
        "a rel declaration mints nothing; the one fact is a bodiless clause"
    );
    assert_eq!(cfg_edges(&path, &source).len(), 2, "entry -> head -> exit");
}
