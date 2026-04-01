/// Lower .sprf parse tree to Rule types.
///
/// SelectorChain -> Rule
///
/// Bare glob inference: N bare globs before first tagged slot.
///   N=3: repo > branch > fs
///   N<3 or N>3: error
///
/// Tagged slots dispatch by tag:
///   fs(pat)            -> File { pattern }
///   repo(pat)          -> Repo { pattern }
///   branch(pat)        -> Branch { pattern }
///   json(body)         -> parse body via _2_pattern, get Vec<SelectStep>
///   ast(pat)           -> AstSelector { pattern }
///   ast[lang](pat)     -> AstSelector { pattern, language }
///   re(pat)            -> ValuePattern { source, pattern }
use anyhow::{bail, Result};
use sprefa_rules::types::{
    AstSelector, LinkPredicate, LinkRule, MatchDef, QueryAtom, QueryDef, Rule, RuleSet, SelectStep,
    Side, ValuePattern,
};

use crate::_0_ast::{LinkDecl, Program, QueryDecl, SelectorChain, Slot, Statement, Tag, Term};
use crate::_2_pattern::parse_json_body;

/// Lower a parsed program into a RuleSet (rules + link rules).
pub fn lower_program(program: &Program) -> Result<RuleSet> {
    let mut rules = vec![];
    let mut link_rules = vec![];
    let mut query_rules = vec![];
    let mut rule_idx = 0;

    for stmt in program {
        match stmt {
            Statement::Rule(chain) => {
                let rule = lower_chain(chain, rule_idx)?;
                rules.push(rule);
                rule_idx += 1;
            }
            Statement::Link(decl) => {
                let lr = lower_link(decl)?;
                link_rules.push(lr);
            }
            Statement::Query(decl) => {
                let qd = lower_query(decl)?;
                query_rules.push(qd);
            }
        }
    }

    Ok(RuleSet {
        schema: None,
        rules,
        link_rules,
        query_rules,
    })
}

fn lower_chain(chain: &SelectorChain, index: usize) -> Result<Rule> {
    let mut select: Vec<SelectStep> = vec![];
    let mut select_ast: Option<AstSelector> = None;
    let mut value_pattern: Option<ValuePattern> = None;
    let mut create_matches: Vec<MatchDef> = vec![];
    let mut rule_name: Option<String> = None;

    // Count leading bare globs
    let bare_count = chain.slots.iter().take_while(|s| matches!(s, Slot::Bare(_))).count();
    let first_tagged = bare_count;

    // Bare glob inference
    if bare_count > 0 {
        if bare_count != 3 {
            bail!(
                "rule {}: bare context requires exactly 3 slots (repo > branch > fs), found {}. \
                 Use explicit tags: repo(...), branch(...), fs(...)",
                index,
                bare_count,
            );
        }

        if let Slot::Bare(pat) = &chain.slots[0] {
            select.push(SelectStep::Repo {
                pattern: pat.clone(),
                capture: None,
            });
        }
        if let Slot::Bare(pat) = &chain.slots[1] {
            select.push(SelectStep::Branch {
                pattern: pat.clone(),
                capture: None,
            });
        }
        if let Slot::Bare(pat) = &chain.slots[2] {
            select.push(SelectStep::File {
                pattern: pat.clone(),
                capture: None,
            });
        }
    }

    // Process tagged and match slots
    for slot in &chain.slots[first_tagged..] {
        match slot {
            Slot::Bare(_) => {
                bail!(
                    "rule {}: bare glob after tagged slot is not allowed. \
                     Use explicit tags.",
                    index,
                );
            }
            Slot::Match { capture, kind } => {
                create_matches.push(MatchDef {
                    capture: capture.clone(),
                    kind: kind.clone(),
                    parent: None,
                });
            }
            Slot::Tagged { tag, arg, body } => match tag {
                Tag::Fs => {
                    select.push(SelectStep::File {
                        pattern: body.clone(),
                        capture: None,
                    });
                }
                Tag::Repo => {
                    select.push(SelectStep::Repo {
                        pattern: body.clone(),
                        capture: None,
                    });
                }
                Tag::Branch => {
                    select.push(SelectStep::Branch {
                        pattern: body.clone(),
                        capture: None,
                    });
                }
                Tag::Json => {
                    let steps = parse_json_body(body)?;
                    select.extend(steps);
                }
                Tag::Ast => {
                    if select_ast.is_some() {
                        bail!("rule {}: multiple ast() slots not supported", index);
                    }
                    select_ast = Some(AstSelector {
                        pattern: Some(body.clone()),
                        rule: None,
                        constraints: None,
                        rule_file: None,
                        language: arg.clone(),
                        capture: "$NAME".to_string(),
                        captures: None,
                    });
                }
                Tag::Re => {
                    if value_pattern.is_some() {
                        bail!("rule {}: multiple re() slots not supported", index);
                    }
                    value_pattern = Some(ValuePattern {
                        source: String::new(),
                        pattern: body.clone(),
                        full_match: true,
                    });
                }
                Tag::Rule => {
                    rule_name = Some(body.clone());
                }
            },
        }
    }

    Ok(Rule {
        name: rule_name.unwrap_or_else(|| format!("sprf-rule-{}", index)),
        description: None,
        select,
        select_ast,
        value: value_pattern,
        create_matches,
        confidence: None,
    })
}

/// Lower a query declaration to a QueryDef.
fn lower_query(decl: &QueryDecl) -> Result<QueryDef> {
    fn lower_term(t: &Term) -> String {
        match t {
            Term::Var(name) => name.clone(),
            Term::Lit(val) => format!("={}", val),
            Term::Wild => "_".to_string(),
        }
    }

    let head_args: Vec<String> = decl.head.args.iter().map(lower_term).collect();
    let arity = head_args.len();
    let name = decl.head.relation.clone();

    let body: Vec<QueryAtom> = decl
        .body
        .iter()
        .map(|atom| QueryAtom {
            relation: atom.relation.clone(),
            args: atom.args.iter().map(lower_term).collect(),
        })
        .collect();

    let is_recursive = decl.body.iter().any(|atom| atom.relation == name);

    Ok(QueryDef {
        name,
        arity,
        head_args,
        body,
        is_recursive,
    })
}

/// Lower a link declaration to a LinkRule.
fn lower_link(decl: &LinkDecl) -> Result<LinkRule> {
    let mut all = vec![
        LinkPredicate::KindEq {
            side: Side::Src,
            value: decl.src_kind.clone(),
        },
        LinkPredicate::KindEq {
            side: Side::Tgt,
            value: decl.tgt_kind.clone(),
        },
    ];

    for pred_str in &decl.predicates {
        let pred = match pred_str.as_str() {
            "norm_eq" => LinkPredicate::NormEq,
            "norm2_eq" => LinkPredicate::Norm2Eq,
            "string_eq" => LinkPredicate::StringEq,
            "target_file_eq" => LinkPredicate::TargetFileEq,
            "same_repo" => LinkPredicate::SameRepo,
            "same_file" => LinkPredicate::SameFile,
            "stem_eq_src" => LinkPredicate::StemEq { side: Side::Src },
            "stem_eq_tgt" => LinkPredicate::StemEq { side: Side::Tgt },
            "ext_eq_src" => LinkPredicate::ExtEq { side: Side::Src },
            "ext_eq_tgt" => LinkPredicate::ExtEq { side: Side::Tgt },
            "dir_eq_src" => LinkPredicate::DirEq { side: Side::Src },
            "dir_eq_tgt" => LinkPredicate::DirEq { side: Side::Tgt },
            other => bail!("unknown link predicate: {:?}", other),
        };
        all.push(pred);
    }

    let kind = decl.kind_name.clone().unwrap_or_else(|| {
        format!("{}__{}", decl.src_kind, decl.tgt_kind)
    });

    Ok(LinkRule {
        kind,
        sql: None,
        predicate: Some(LinkPredicate::And { all }),
        target_repos: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::_1_parse::parse_program;

    fn lower(input: &str) -> Vec<Rule> {
        let program = parse_program(input).unwrap();
        lower_program(&program).unwrap().rules
    }

    #[test]
    fn lower_fs_json() {
        let rules = lower("fs(**/Cargo.toml) > json({ package: { name: $NAME } });");
        assert_eq!(rules.len(), 1);
        let r = &rules[0];

        assert!(matches!(&r.select[0], SelectStep::File { pattern, .. } if pattern == "**/Cargo.toml"));
        // select[1] is the Object from json body
        assert!(matches!(&r.select[1], SelectStep::Object { .. }));
    }

    #[test]
    fn lower_bare_three() {
        let rules = lower("my-org/* > main > **/Cargo.toml > json({ name: $N });");
        let r = &rules[0];

        assert!(matches!(&r.select[0], SelectStep::Repo { pattern, .. } if pattern == "my-org/*"));
        assert!(matches!(&r.select[1], SelectStep::Branch { pattern, .. } if pattern == "main"));
        assert!(matches!(&r.select[2], SelectStep::File { pattern, .. } if pattern == "**/Cargo.toml"));
        assert!(matches!(&r.select[3], SelectStep::Object { .. }));
    }

    #[test]
    fn lower_bare_two_errors() {
        let program = parse_program("my-org/* > **/Cargo.toml > json({ name: $N });").unwrap();
        let err = lower_program(&program).unwrap_err();
        assert!(err.to_string().contains("exactly 3"), "{}", err);
    }

    #[test]
    fn lower_ast_with_lang() {
        let rules = lower("fs(**/*.config) > ast[typescript](import $NAME from '$PATH');");
        let r = &rules[0];
        let ast = r.select_ast.as_ref().unwrap();
        assert_eq!(ast.language.as_deref(), Some("typescript"));
        assert_eq!(ast.pattern.as_deref(), Some("import $NAME from '$PATH'"));
    }

    #[test]
    fn lower_recursive_descent() {
        let rules = lower("fs(**/values.yaml) > json({ **: { image: { repository: $REPO, tag: $TAG } } });");
        let r = &rules[0];
        // File, then Any, then Object
        assert!(matches!(&r.select[0], SelectStep::File { .. }));
        assert!(matches!(&r.select[1], SelectStep::Any));
        assert!(matches!(&r.select[2], SelectStep::Object { .. }));
    }

    #[test]
    fn lower_array() {
        let rules = lower("fs(**/Cargo.toml) > json({ workspace: { members: [...$MEMBER] } });");
        let r = &rules[0];
        // File -> Object(workspace -> Object(members -> Array))
        assert!(matches!(&r.select[0], SelectStep::File { .. }));
        match &r.select[1] {
            SelectStep::Object { entries } => {
                match &entries[0].value[0] {
                    SelectStep::Object { entries: inner } => {
                        assert!(matches!(&inner[0].value[0], SelectStep::Array { .. }));
                    }
                    _ => panic!("expected nested Object"),
                }
            }
            _ => panic!("expected Object"),
        }
    }

    #[test]
    fn lower_re() {
        let rules = lower(r"fs(helm/**/*.yaml) > re(image:\s+(?P<REPO>[^:]+):(?P<TAG>.+));");
        let r = &rules[0];
        assert!(r.value.is_some());
        assert_eq!(r.value.as_ref().unwrap().pattern, r"image:\s+(?P<REPO>[^:]+):(?P<TAG>.+)");
    }

    #[test]
    fn lower_rule_name() {
        let rules = lower("rule(cargo_packages) > fs(**/Cargo.toml) > json({ package: { name: $NAME } });");
        assert_eq!(rules[0].name, "cargo_packages");
    }

    #[test]
    fn lower_rule_name_at_end() {
        let rules = lower("fs(**/Cargo.toml) > json({ package: { name: $NAME } }) > rule(my_rule);");
        assert_eq!(rules[0].name, "my_rule");
    }

    #[test]
    fn lower_rule_name_absent_uses_index() {
        let rules = lower("fs(**/Cargo.toml) > json({ package: { name: $NAME } });");
        assert_eq!(rules[0].name, "sprf-rule-0");
    }

    #[test]
    fn lower_multiple_rules() {
        let rules = lower(r#"
            fs(**/package.json) > json({ name: $NAME });
            fs(**/Cargo.toml) > json({ package: { name: $N } });
        "#);
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].name, "sprf-rule-0");
        assert_eq!(rules[1].name, "sprf-rule-1");
    }

    #[test]
    fn lower_match_slots() {
        let rules = lower(
            "fs(**/Cargo.toml) > json({ package: { name: $NAME } }) > match($NAME, package_name);"
        );
        let r = &rules[0];
        assert_eq!(r.create_matches.len(), 1);
        assert_eq!(r.create_matches[0].capture, "NAME");
        assert_eq!(r.create_matches[0].kind, "package_name");
        assert!(r.create_matches[0].parent.is_none());
    }

    #[test]
    fn lower_multiple_match_slots() {
        let rules = lower(
            "fs(**/package.json) > json({ dependencies: { $N: $V } }) > match($N, dep_name) > match($V, dep_version);"
        );
        let r = &rules[0];
        assert_eq!(r.create_matches.len(), 2);
        assert_eq!(r.create_matches[0].kind, "dep_name");
        assert_eq!(r.create_matches[1].kind, "dep_version");
    }

    fn lower_full(input: &str) -> RuleSet {
        let program = parse_program(input).unwrap();
        lower_program(&program).unwrap()
    }

    #[test]
    fn lower_link_rule() {
        let rs = lower_full("link(dep_name > package_name, norm_eq) > $dep_to_package;");
        assert_eq!(rs.link_rules.len(), 1);
        let lr = &rs.link_rules[0];
        assert_eq!(lr.kind, "dep_to_package");
        assert!(lr.predicate.is_some());
    }

    #[test]
    fn lower_link_auto_kind() {
        let rs = lower_full("link(dep_name > package_name, norm_eq);");
        assert_eq!(rs.link_rules[0].kind, "dep_name__package_name");
    }

    #[test]
    fn lower_link_multiple_predicates() {
        let rs = lower_full("link(import_name > export_name, target_file_eq, string_eq) > $import_binding;");
        let lr = &rs.link_rules[0];
        match lr.predicate.as_ref().unwrap() {
            LinkPredicate::And { all } => {
                // 2 KindEq + 2 predicates = 4
                assert_eq!(all.len(), 4);
            }
            _ => panic!("expected And"),
        }
    }

    #[test]
    fn lower_mixed_rules_and_links() {
        let rs = lower_full(r#"
            fs(**/Cargo.toml) > json({ package: { name: $NAME } }) > match($NAME, package_name);
            link(dep_name > package_name, norm_eq) > $dep_to_package;
        "#);
        assert_eq!(rs.rules.len(), 1);
        assert_eq!(rs.link_rules.len(), 1);
        assert_eq!(rs.rules[0].create_matches.len(), 1);
    }

    #[test]
    fn lower_query_recursive() {
        let rs = lower_full(
            "query all_deps($A, $C) :- dep_to_package($A, $B), all_deps($B, $C);"
        );
        assert_eq!(rs.query_rules.len(), 1);
        let q = &rs.query_rules[0];
        assert_eq!(q.name, "all_deps");
        assert_eq!(q.arity, 2);
        assert!(q.is_recursive);
        assert_eq!(q.head_args, vec!["A", "C"]);
        assert_eq!(q.body.len(), 2);
        assert_eq!(q.body[0].relation, "dep_to_package");
        assert_eq!(q.body[1].relation, "all_deps");
    }

    #[test]
    fn lower_query_nonrecursive() {
        let rs = lower_full(
            "query same_eco($A, $B) :- dep_to_package($A, $X), dep_to_package($B, $X);"
        );
        let q = &rs.query_rules[0];
        assert!(!q.is_recursive);
        assert_eq!(q.body[0].args, vec!["A", "X"]);
        assert_eq!(q.body[1].args, vec!["B", "X"]);
    }

    #[test]
    fn lower_query_with_literal() {
        let rs = lower_full(
            r#"query who_uses($WHO) :- dep_to_package($WHO, "lodash");"#
        );
        let q = &rs.query_rules[0];
        assert_eq!(q.body[0].args, vec!["WHO", "=lodash"]);
    }

    #[test]
    fn lower_full_pipeline() {
        let rs = lower_full(r#"
            fs(**/Cargo.toml) > json({ package: { name: $NAME } }) > match($NAME, package_name);
            fs(**/Cargo.toml) > json({ dependencies: { $N: $_ } }) > match($N, dep_name);
            link(dep_name > package_name, norm_eq) > $dep_to_package;
            query all_deps($A, $C) :- dep_to_package($A, $B), all_deps($B, $C);
        "#);
        assert_eq!(rs.rules.len(), 2);
        assert_eq!(rs.link_rules.len(), 1);
        assert_eq!(rs.query_rules.len(), 1);
    }
}
