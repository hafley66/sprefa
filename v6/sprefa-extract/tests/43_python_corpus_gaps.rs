//! TEST the python arm against the gaps the CPython 3.14 stdlib battery found.
//! FAIL-PRE-FIX: every case here is red on the arm as PR #524 landed it --
//! `future_import_statement` reached no import arm, `type_alias_statement`
//! reached no entity arm, and `py_flow_stmt` walked only assignment / return /
//! for / while, so augmented assignment, `with`, `except ... as`, the walrus,
//! and every `if` / `elif` / `assert` expression produced no dataflow.
//! Each fixture states its own expected fact in a header comment.

use sprefa_extract::{DfNodeKind, FamilyMask, PythonSource, Source, TypeEntityKind};

/// (kind, name, span start) per df node, in emission order.
fn df_nodes(source: &[u8]) -> Vec<(String, Option<String>, u32)> {
    let output = PythonSource.extract("corpus.py", source, FamilyMask::ALL);
    let df = output.df.as_ref().expect("the df family is on");
    df.nodes
        .iter()
        .map(|node| {
            (
                node.kind.as_str().to_string(),
                node.name.map(|id| output.strings.lookup(id).to_string()),
                node.span.start,
            )
        })
        .collect()
}

/// (from kind, from start, to kind, to start) per df edge.
fn df_edges(source: &[u8]) -> Vec<(String, u32, String, u32)> {
    let output = PythonSource.extract("corpus.py", source, FamilyMask::ALL);
    let df = output.df.as_ref().expect("the df family is on");
    df.edges
        .iter()
        .map(|edge| {
            let from = df.node(edge.src);
            let to = df.node(edge.dst);
            (
                from.kind.as_str().to_string(),
                from.span.start,
                to.kind.as_str().to_string(),
                to.span.start,
            )
        })
        .collect()
}

fn has_node(source: &[u8], kind: DfNodeKind, name: &str) -> bool {
    df_nodes(source)
        .iter()
        .any(|(k, n, _)| k == kind.as_str() && n.as_deref() == Some(name))
}

/// The bind `name` resolves to, and the node feeding it.
fn bind_start(source: &[u8], name: &str) -> u32 {
    df_nodes(source)
        .into_iter()
        .find(|(k, n, _)| k == DfNodeKind::LetBind.as_str() && n.as_deref() == Some(name))
        .unwrap_or_else(|| panic!("no let_bind named {name}"))
        .2
}

#[test]
fn future_import_mints_a_specifier() {
    const SOURCE: &[u8] = include_bytes!("fixtures/python/corpus_1.py");
    let output = PythonSource.extract("corpus_1.py", SOURCE, FamilyMask::ALL);
    let call = output.call.as_ref().expect("the call family is on");
    let rows: Vec<(&str, &str, Option<&str>)> = call
        .aux
        .specifiers
        .iter()
        .map(|specifier| {
            (
                specifier.kind.as_str(),
                output.strings.lookup(specifier.name),
                specifier.module.map(|id| output.strings.lookup(id)),
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            ("named", "annotations", Some("__future__")),
            ("named", "generator_stop", Some("__future__")),
        ]
    );
}

#[test]
fn pep695_type_alias_mints_an_entity() {
    const SOURCE: &[u8] = include_bytes!("fixtures/python/corpus_2.py");
    let output = PythonSource.extract("corpus_2.py", SOURCE, FamilyMask::ALL);
    let types = output.types.as_ref().expect("the type family is on");
    let aliases: Vec<&str> = types
        .nodes
        .iter()
        .filter(|node| node.kind == TypeEntityKind::Alias)
        .filter_map(|node| node.name.map(|id| output.strings.lookup(id)))
        .collect();
    assert_eq!(aliases, ["Alias", "Pair"]);
}

#[test]
fn augmented_assignment_rebinds_its_target() {
    const SOURCE: &[u8] = include_bytes!("fixtures/python/corpus_3.py");
    let nodes = df_nodes(SOURCE);
    let binds: Vec<u32> = nodes
        .iter()
        .filter(|(k, n, _)| k == "let_bind" && n.as_deref() == Some("total"))
        .map(|(_, _, start)| *start)
        .collect();
    assert_eq!(binds.len(), 2, "one bind per assignment: {nodes:?}");

    let edges = df_edges(SOURCE);
    let rebind = binds[1];
    assert!(
        edges
            .iter()
            .any(|(fk, _, tk, ts)| fk == "call_res" && tk == "let_bind" && *ts == rebind),
        "the rhs call feeds the rebind: {edges:?}"
    );
    // The return must read the SECOND binding, not the stale `total = 0`.
    let read = nodes
        .iter()
        .rfind(|(k, n, _)| k == "var_read" && n.as_deref() == Some("total"))
        .expect("the return reads total")
        .2;
    assert!(
        edges.iter().any(|(fk, fs, tk, ts)| fk == "let_bind"
            && *fs == rebind
            && tk == "var_read"
            && *ts == read),
        "the return reads the rebind: {edges:?}"
    );
}

#[test]
fn with_statement_flows_its_context_and_binds_its_target() {
    const SOURCE: &[u8] = include_bytes!("fixtures/python/corpus_4.py");
    let nodes = df_nodes(SOURCE);
    let calls = nodes.iter().filter(|(k, _, _)| k == "call_res").count();
    assert_eq!(
        calls, 2,
        "one call_res for `open(path)`, one for `fh.read()`: {nodes:?}"
    );

    let bind = bind_start(SOURCE, "fh");
    let edges = df_edges(SOURCE);
    assert!(
        edges
            .iter()
            .any(|(fk, _, tk, ts)| fk == "call_res" && tk == "let_bind" && *ts == bind),
        "the context value feeds the `as` binding: {edges:?}"
    );
    assert!(
        edges
            .iter()
            .any(|(fk, fs, tk, _)| fk == "let_bind" && *fs == bind && tk == "var_read"),
        "the body reads the binding: {edges:?}"
    );
}

#[test]
fn except_as_binds_the_exception_name() {
    const SOURCE: &[u8] = include_bytes!("fixtures/python/corpus_5.py");
    let bind = bind_start(SOURCE, "err");
    let edges = df_edges(SOURCE);
    assert!(
        edges
            .iter()
            .any(|(fk, fs, tk, _)| fk == "let_bind" && *fs == bind && tk == "var_read"),
        "`report(err)` reads the handler binding: {edges:?}"
    );
}

#[test]
fn walrus_binds_its_name() {
    const SOURCE: &[u8] = include_bytes!("fixtures/python/corpus_6.py");
    let bind = bind_start(SOURCE, "size");
    let edges = df_edges(SOURCE);
    assert!(
        edges
            .iter()
            .any(|(fk, _, tk, ts)| fk == "call_res" && tk == "let_bind" && *ts == bind),
        "`len(items)` feeds the walrus binding: {edges:?}"
    );
    assert!(
        edges
            .iter()
            .any(|(fk, fs, tk, _)| fk == "let_bind" && *fs == bind && tk == "var_read"),
        "the `return size` reads it: {edges:?}"
    );
}

#[test]
fn if_elif_and_assert_expressions_flow() {
    const SOURCE: &[u8] = include_bytes!("fixtures/python/corpus_7.py");
    let nodes = df_nodes(SOURCE);
    let calls = nodes.iter().filter(|(k, _, _)| k == "call_res").count();
    assert_eq!(
        calls, 3,
        "one call_res each for ready / pending / valid: {nodes:?}"
    );
    assert_eq!(
        nodes.iter().filter(|(k, _, _)| k == "new").count(),
        1,
        "the raise operand mints a `new` for Failure: {nodes:?}"
    );
    assert!(
        has_node(SOURCE, DfNodeKind::VarRead, "job"),
        "the conditions read the param: {nodes:?}"
    );
    let edges = df_edges(SOURCE);
    let param_feeds = edges
        .iter()
        .filter(|(fk, _, tk, _)| fk == "param" && tk == "var_read")
        .count();
    assert_eq!(param_feeds, 4, "the param feeds all four reads: {edges:?}");
}
