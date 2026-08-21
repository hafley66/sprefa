use sprefa_extract::{
    content_id_of, decode_ast_rule_yaml, query_ast_rule, AstRule, AstRuleError, AstRuleRequest,
    NamedAstRule, StopBy,
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
    assert_eq!(typed_rows[0].proposal.as_ref().unwrap().replacement, "eprintln!(\"hello\")");
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
