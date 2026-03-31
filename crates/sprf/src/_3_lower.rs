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
use sprefa_rules::types::{AstSelector, Rule, SelectStep, ValuePattern};

use crate::_0_ast::{Program, SelectorChain, Slot, Statement, Tag};
use crate::_2_pattern::parse_json_body;

/// Lower a parsed program into a Vec<Rule>.
/// Rule names are auto-generated from position if not otherwise specified.
pub fn lower_program(program: &Program) -> Result<Vec<Rule>> {
    let mut rules = vec![];
    for (i, stmt) in program.iter().enumerate() {
        match stmt {
            Statement::Rule(chain) => {
                let rule = lower_chain(chain, i)?;
                rules.push(rule);
            }
        }
    }
    Ok(rules)
}

fn lower_chain(chain: &SelectorChain, index: usize) -> Result<Rule> {
    let mut select: Vec<SelectStep> = vec![];
    let mut select_ast: Option<AstSelector> = None;
    let mut value_pattern: Option<ValuePattern> = None;

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

    // Process tagged slots
    for slot in &chain.slots[first_tagged..] {
        match slot {
            Slot::Bare(_) => {
                bail!(
                    "rule {}: bare glob after tagged slot is not allowed. \
                     Use explicit tags.",
                    index,
                );
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
                    // The re() body is the regex pattern. The source capture
                    // needs to be specified somehow. For now, default to the
                    // first leaf capture in the chain.
                    // TODO: this needs a source capture convention.
                    value_pattern = Some(ValuePattern {
                        source: String::new(),
                        pattern: body.clone(),
                        full_match: true,
                    });
                }
            },
        }
    }

    Ok(Rule {
        name: format!("sprf-rule-{}", index),
        description: None,
        select,
        select_ast,
        value: value_pattern,
        create_matches: vec![],
        confidence: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::_1_parse::parse_program;

    fn lower(input: &str) -> Vec<Rule> {
        let program = parse_program(input).unwrap();
        lower_program(&program).unwrap()
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
    fn lower_multiple_rules() {
        let rules = lower(r#"
            fs(**/package.json) > json({ name: $NAME });
            fs(**/Cargo.toml) > json({ package: { name: $N } });
        "#);
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].name, "sprf-rule-0");
        assert_eq!(rules[1].name, "sprf-rule-1");
    }
}
