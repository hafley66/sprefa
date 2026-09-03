use sprefa_extract::{
    content_id_of, decode_ast_rule_yaml, query_ast_rule, query_patterns, query_source,
    query_source_facts, AstCaptureFact, AstPatternQuery, AstRule, AstRuleError, AstRuleRequest,
    NamedAstRule, SourceQuery, SourceQueryOutput, StopBy, TreeSitterQuery,
};

fn request(rule: AstRule) -> AstRuleRequest {
    AstRuleRequest {
        id: "rule".into(),
        rule,
        utils: Vec::new(),
        fix: None,
    }
}

#[test]
fn typed_and_documented_yaml_requests_have_equal_rows() {
    let typed = AstRuleRequest {
        id: "println".into(),
        rule: AstRule::Pattern("println!($MESSAGE)".into()),
        utils: Vec::new(),
        fix: Some("eprintln!($MESSAGE)".into()),
    };
    let yaml = r#"
id: println
rule:
  pattern: println!($MESSAGE)
fix: eprintln!($MESSAGE)
"#;
    let decoded = decode_ast_rule_yaml(yaml).expect("documented ast-rule yaml");
    assert_eq!(typed, decoded);

    let bytes = b"fn main() { println!(\"hello\"); }";
    let typed_rows = query_ast_rule("main.rs", bytes, &typed).expect("typed query");
    let yaml_rows = query_ast_rule("main.rs", bytes, &decoded).expect("yaml query");
    assert_eq!(typed_rows, yaml_rows);
    assert_eq!(typed_rows[0].content, content_id_of(bytes));
    assert_eq!(typed_rows[0].span.start, 12);
    assert_eq!(
        typed_rows[0].proposal.as_ref().unwrap().span,
        typed_rows[0].span
    );
    assert_eq!(
        typed_rows[0].proposal.as_ref().unwrap().replacement,
        "eprintln!(\"hello\")"
    );
    assert_eq!(typed_rows[0].proposal.as_ref().unwrap().span.start, 12);
    assert_eq!(typed_rows[0].proposal.as_ref().unwrap().span.len, 17);
}

#[test]
fn composition_algebra_compiles_against_the_linked_ast_grep_config() {
    let source = b"fn one() { println!(\"one\"); }\nfn two() { eprintln!(\"two\"); }\n";
    let cases = [
        AstRule::Any(vec![
            AstRule::Pattern("println!($X)".into()),
            AstRule::Pattern("eprintln!($X)".into()),
        ]),
        AstRule::All(vec![
            AstRule::Kind("macro_invocation".into()),
            AstRule::Pattern("println!($X)".into()),
        ]),
        AstRule::All(vec![
            AstRule::Kind("macro_invocation".into()),
            AstRule::Not(Box::new(AstRule::Pattern("eprintln!($X)".into()))),
        ]),
        AstRule::All(vec![
            AstRule::Kind("macro_invocation".into()),
            AstRule::Inside {
                rule: Box::new(AstRule::Pattern("fn $NAME() { $$$BODY }".into())),
                stop_by: Some(StopBy::End("end".into())),
            },
        ]),
        AstRule::All(vec![
            AstRule::Kind("function_item".into()),
            AstRule::Has {
                rule: Box::new(AstRule::Pattern("println!($X)".into())),
                stop_by: Some(StopBy::End("end".into())),
            },
        ]),
        AstRule::All(vec![
            AstRule::Kind("function_item".into()),
            AstRule::Follows {
                rule: Box::new(AstRule::Pattern("fn one() { $$$BODY }".into())),
                stop_by: Some(StopBy::End("end".into())),
            },
        ]),
        AstRule::All(vec![
            AstRule::Kind("function_item".into()),
            AstRule::Precedes {
                rule: Box::new(AstRule::Pattern("fn two() { $$$BODY }".into())),
                stop_by: Some(StopBy::End("end".into())),
            },
        ]),
    ];
    for rule in cases {
        query_ast_rule("main.rs", source, &request(rule)).expect("composition rule compiles");
    }
}

#[test]
fn named_utils_and_matches_are_resolved_by_the_same_typed_model() {
    let request = AstRuleRequest {
        id: "uses_util".into(),
        rule: AstRule::Matches("print_call".into()),
        utils: vec![NamedAstRule {
            id: "print_call".into(),
            rule: AstRule::Pattern("println!($MESSAGE)".into()),
        }],
        fix: None,
    };
    let rows = query_ast_rule("main.rs", b"fn main() { println!(\"ok\"); }", &request)
        .expect("matches utility");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].captures[0].name, "MESSAGE");
}

#[test]
fn malformed_yaml_and_invalid_rules_are_named_errors() {
    assert!(matches!(
        decode_ast_rule_yaml("id: ["),
        Err(AstRuleError::Yaml(_))
    ));
    assert!(matches!(
        query_ast_rule(
            "main.rs",
            b"fn main() {}",
            &request(AstRule::Kind("not_a_rust_kind".into()))
        ),
        Err(AstRuleError::InvalidRule(_))
    ));
}

/// The v5 AstPatternQuery surface remains available for callers that provide
/// a pattern, an optional contextual selector, and the capture names to emit.
/// This checks the extractor API shape; it does not assert parity with any
/// external ast-grep YAML configuration format.
#[test]
fn pattern_query_preserves_contextual_selector_and_requested_captures() {
    let facts = query_patterns(
        "main.rs",
        b"fn one() { println!(\"one\"); }\nfn two() {}\n",
        &[AstPatternQuery {
            id: "function_name".into(),
            pattern: "fn $NAME() { $$$BODY }".into(),
            selector: Some("function_item".into()),
            captures: vec!["NAME".into()],
        }],
    )
    .expect("v5-shaped pattern query");

    assert_eq!(
        facts,
        vec![
            AstCaptureFact {
                record: "capture",
                query: "function_name".into(),
                capture: "NAME".into(),
                text: "one".into(),
                start: 3,
                end: 6,
                match_start: 0,
                match_end: 29,
            },
            AstCaptureFact {
                record: "capture",
                query: "function_name".into(),
                capture: "NAME".into(),
                text: "two".into(),
                start: 33,
                end: 36,
                match_start: 30,
                match_end: 41,
            },
        ]
    );
}

#[test]
fn source_query_facade_preserves_each_engine_result_shape() {
    let source = b"fn main() { println!(\"hello\"); }";

    let tree_sitter = query_source(
        "main.rs",
        source,
        &SourceQuery::TreeSitter(TreeSitterQuery {
            language: "rust".into(),
            query: "(function_item name: (identifier) @name) @item".into(),
        }),
    )
    .expect("tree-sitter query");
    let SourceQueryOutput::TreeSitter(tree_sitter) = tree_sitter else {
        panic!("tree-sitter output variant")
    };
    assert_eq!(
        serde_json::to_string(&tree_sitter).unwrap(),
        r#"[{"end_line":1,"item":"fn main() { println!(\"hello\"); }","line":1,"name":"main"}]"#
    );

    let patterns = query_source(
        "main.rs",
        source,
        &SourceQuery::AstPatterns(vec![AstPatternQuery {
            id: "print_message".into(),
            pattern: "println!($MESSAGE)".into(),
            selector: None,
            captures: vec!["MESSAGE".into()],
        }]),
    )
    .expect("ast-grep pattern query");
    let SourceQueryOutput::AstPatterns(patterns) = patterns else {
        panic!("ast-grep pattern output variant")
    };
    assert_eq!(patterns[0].query, "print_message");
    assert_eq!(patterns[0].text, "\"hello\"");

    let rule = query_source(
        "main.rs",
        source,
        &SourceQuery::AstRule(request(AstRule::Pattern("println!($MESSAGE)".into()))),
    )
    .expect("composed ast-grep rule");
    let SourceQueryOutput::AstRule(rule) = rule else {
        panic!("composed ast-grep output variant")
    };
    assert_eq!(rule[0].query, "rule");
    assert_eq!(rule[0].captures[0].name, "MESSAGE");
}

#[test]
fn soopy_source_to_common_query_facts_is_one_content_addressed_graph() {
    let fixture_root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/source_facts");
    let mut directory = soopy::DirectoryRoot::open(&fixture_root).unwrap();
    let snapshot = directory
        .snapshot(&soopy::FileQuery {
            patterns: vec![soopy::Pattern("0_subject.rs".into())],
        })
        .unwrap();
    assert_eq!(snapshot.files.len(), 1);
    let entry = snapshot.files[0].clone();
    let action_source = soopy::ActionSource::Directory {
        file: entry.file.clone(),
    };
    let requests = [soopy::FileReadRequest {
        file: entry.file.clone(),
        expected: Some(entry.content.clone()),
    }];
    let queries = [
        SourceQuery::TreeSitter(TreeSitterQuery {
            language: "rust".into(),
            query: "(function_item name: (identifier) @name) @function".into(),
        }),
        SourceQuery::AstPatterns(vec![AstPatternQuery {
            id: "print_message".into(),
            pattern: "println!($MESSAGE)".into(),
            selector: None,
            captures: vec!["MESSAGE".into()],
        }]),
        SourceQuery::AstRule(AstRuleRequest {
            id: "replace_print".into(),
            rule: AstRule::Pattern("println!($MESSAGE)".into()),
            utils: Vec::new(),
            fix: Some("eprintln!($MESSAGE)".into()),
        }),
    ];
    let mut rendered = String::new();
    directory
        .read_each(&requests, |read| {
            let graphs = queries
                .iter()
                .map(|query| {
                    query_source_facts(
                        &action_source,
                        read.content,
                        read.file.path.0.as_ref(),
                        read.bytes,
                        query,
                    )
                    .unwrap()
                })
                .collect::<Vec<_>>();
            let mut value = serde_json::to_value(&graphs).unwrap();
            for graph in value.as_array_mut().unwrap() {
                graph["source"]["directory"] = "<directory>".into();
            }
            rendered = serde_json::to_string_pretty(&value).unwrap();
            Ok(())
        })
        .unwrap();

    assert_eq!(
        rendered,
        include_str!("fixtures/source_facts/1_expected.json").trim_end()
    );

    let wrong_content = soopy::ContentId::blake3(b"different bytes");
    let bytes = std::fs::read(fixture_root.join("0_subject.rs")).unwrap();
    let error = query_source_facts(
        &action_source,
        &wrong_content,
        entry.file.path.0.as_ref(),
        &bytes,
        &queries[0],
    )
    .unwrap_err();
    assert!(error.to_string().contains("observed blake3:"));
}
