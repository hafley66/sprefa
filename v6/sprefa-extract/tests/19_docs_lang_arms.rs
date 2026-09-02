//! Doc-facet lang arms: ts/go/python graded against the captured v5 `doc` oracle rows;
//! kotlin has NO v5 oracle doc rows, so it is graded by hand-written assertions.

use std::collections::{BTreeMap, BTreeSet};

use sprefa_extract::{dispatch, flatten, FamilyMask, FamilyTag, FlatFact};

/// 1-based line containing `byte_off` (newline count + 1). Matches v5's `line_at`.
fn line_of(bytes: &[u8], byte_off: u32) -> u32 {
    bytes[..byte_off as usize]
        .iter()
        .filter(|&&b| b == b'\n')
        .count() as u32
        + 1
}

/// The v6 `doc` rows for one fixture, rendered into v5's oracle line shape.
fn doc_rows(path: &str, bytes: &[u8]) -> BTreeSet<String> {
    let out = dispatch(path, bytes, FamilyMask::ALL).expect("a Source matches the fixture");
    let facts = flatten(&out);
    let entity: BTreeMap<(u32, u32), (String, String)> = facts
        .iter()
        .filter_map(|fact| match fact {
            FlatFact::Node {
                family: FamilyTag::Type,
                span,
                kind,
                name: Some(name),
                ..
            } => Some(((span.start, span.end), (kind.clone(), name.clone()))),
            _ => None,
        })
        .collect();
    let mut set = BTreeSet::new();
    for fact in &facts {
        if let FlatFact::Doc { owner, parent, .. } = fact {
            if let Some((kind, name)) = entity.get(&(owner.start, owner.end)) {
                let qualified = match parent {
                    Some(owner_type) => format!("{owner_type}.{name}"),
                    None => name.clone(),
                };
                set.insert(format!(
                    "doc\t{path}::{kind}::{qualified}\t{}",
                    line_of(bytes, owner.start)
                ));
            }
        }
    }
    set
}

fn oracle_doc_rows(baseline: &str) -> BTreeSet<String> {
    baseline
        .lines()
        .filter(|l| l.starts_with("doc\t"))
        .map(str::to_owned)
        .collect()
}

#[test]
fn ts_docs_match_v5() {
    let path = "v6/sprefa-extract/tests/fixtures/ts/docs.ts";
    let v6 = doc_rows(path, include_bytes!("fixtures/ts/docs.ts"));
    let v5 = oracle_doc_rows(include_str!("fixtures/ts/docs.v5.jsonl"));
    assert_eq!(v6, v5, "ts docs parity vs v5 oracle");
    // A plain string const has no doc anchor, so the jsdoc above `greeting` is
    // dropped: no doc row names it.
    assert!(
        !v6.iter().any(|row| row.contains("greeting")),
        "dropped const doc"
    );
}

#[test]
fn go_docs_match_v5() {
    let path = "v6/sprefa-extract/tests/fixtures/go/docs.go";
    let v6 = doc_rows(path, include_bytes!("fixtures/go/docs.go"));
    let v5 = oracle_doc_rows(include_str!("fixtures/go/docs.v5.jsonl"));
    // Set equality with the 6-row oracle is also the no-spurious-row check: an
    // undocumented decl (or a doc separated by a blank line) would add a row.
    assert_eq!(v6, v5, "go docs parity vs v5 oracle");
    assert_eq!(v6.len(), 6, "go emits exactly the 6 oracle doc rows");
}

#[test]
fn python_docs_match_v5() {
    let path = "v6/sprefa-extract/tests/fixtures/python/docs.py";
    let v6 = doc_rows(path, include_bytes!("fixtures/python/docs.py"));
    let v5 = oracle_doc_rows(include_str!("fixtures/python/docs.v5.jsonl"));
    // The module docstring anchors on the `<module>` entity (v5's
    // EntityKind::Module twin); class, method and function docstrings on their
    // own entity. Set equality is also the no-spurious-row check.
    assert_eq!(v6, v5, "python docs parity vs v5 oracle");
    assert_eq!(v6.len(), 4, "python emits exactly the 4 oracle doc rows");
}

// ── kotlin: no v5 oracle doc rows, so this is a self-graded fixture. ─────────

#[test]
fn kotlin_docs_self_graded() {
    let path = "v6/sprefa-extract/tests/fixtures/kotlin/docs.kt";
    let out = dispatch(
        path,
        include_bytes!("fixtures/kotlin/docs.kt"),
        FamilyMask::ALL,
    )
    .expect("a Source matches the fixture");
    let facts = flatten(&out);
    let entity: BTreeMap<(u32, u32), (String, String)> = facts
        .iter()
        .filter_map(|fact| match fact {
            FlatFact::Node {
                family: FamilyTag::Type,
                span,
                kind,
                name: Some(name),
                ..
            } => Some(((span.start, span.end), (kind.clone(), name.clone()))),
            _ => None,
        })
        .collect();

    let mut docs: Vec<(String, String, u32, String)> = Vec::new();
    for fact in &facts {
        if let FlatFact::Doc { owner, text, .. } = fact {
            let (kind, name) = entity
                .get(&(owner.start, owner.end))
                .unwrap_or_else(|| panic!("doc owner not a TypeF node at {owner:?}"));
            docs.push((
                kind.clone(),
                name.clone(),
                line_of(include_bytes!("fixtures/kotlin/docs.kt"), owner.start),
                text.clone(),
            ));
        }
    }

    let find = |kind: &str, name: &str| {
        docs.iter()
            .find(|(k, n, _, _)| k == kind && n == name)
            .unwrap_or_else(|| panic!("missing doc row for {kind}::{name}"))
            .clone()
    };

    let (_, _, class_line, class_text) = find("class", "Car");
    assert_eq!(class_line, 8, "class doc line");
    assert!(class_text.contains("A car"), "class doc text");

    let (_, _, fn_line, fn_text) = find("function", "drive");
    assert_eq!(fn_line, 10, "method doc line");
    assert!(fn_text.contains("Drives the car"), "method doc text");

    let (_, _, top_line, top_text) = find("function", "makeCar");
    assert_eq!(top_line, 20, "top-level fn doc line");
    assert!(top_text.contains("Builds a car"), "top-level fn doc text");

    // At least one structured tag survives: `@param name` -> tag=param arg=name.
    let tags: Vec<(String, Option<String>, String)> = facts
        .iter()
        .filter_map(|fact| match fact {
            FlatFact::DocTagOut { tag, arg, text, .. } => {
                Some((tag.clone(), arg.clone(), text.clone()))
            }
            _ => None,
        })
        .collect();
    assert!(
        tags.iter()
            .any(|(tag, arg, _)| tag == "param" && arg.as_deref() == Some("name")),
        "a @param tag with its name arg survives; tags={tags:?}"
    );

    // A declaration with NO KDoc emits no doc row.
    assert!(
        !docs.iter().any(|(_, n, ..)| n == "plainFn"),
        "undocumented fn emits no row; docs={docs:?}"
    );
}
