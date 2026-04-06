/// Lower .sprf parse tree to Rule types.
///
/// Scoped RuleBody -> flattened Rule with dependency-based ordering
///
/// The lowering process:
/// 1. Flatten nested blocks into linear sequence with scope depths
/// 2. Sort steps based on variable dependencies (if B references var from A, A comes first)
/// 3. Convert slots to SelectStep
///
/// Example:
/// ```sprf
/// rule(deploy) {
///   repo($REPO) {
///     rev(main) {
///       fs(**/values.yaml) > json({ svc: $SVC })
///     }
///   }
/// };
/// ```
/// Becomes steps: [repo@depth0, rev@depth1, fs@depth2, json@depth2]
/// Execution order respects scope: repo runs first, then rev, then fs/json
use anyhow::{bail, Result};
use sprefa_rules::types::{AstSelector, MatchDef, Rule, RuleSet, SelectStep, ValuePattern};
use std::collections::{HashMap, HashSet};

use crate::_0_ast::{Program, RuleBody, RuleDecl, Slot, Statement, Tag};
use crate::_2_pattern::parse_json_body;

/// Lower a parsed program into a RuleSet.
pub fn lower_program(program: &Program) -> Result<RuleSet> {
    let mut rules = vec![];

    for stmt in program {
        match stmt {
            Statement::Rule(decl) => {
                let rule = lower_rule_decl(decl)?;
                rules.push(rule);
            }
        }
    }

    Ok(RuleSet {
        schema: None,
        rules,
    })
}

/// One step in the flattened rule with scope information.
#[derive(Debug, Clone)]
struct ScopedStep {
    depth: usize,
    scope_vars: HashSet<String>, // Variables captured at this scope level
    select_steps: Vec<SelectStep>,
    ast_selector: Option<AstSelector>,
    value_pattern: Option<ValuePattern>,
}

fn lower_rule_decl(decl: &RuleDecl) -> Result<Rule> {
    // Flatten all body items into scoped steps
    let mut all_scoped = vec![];
    for body in &decl.body {
        let flattened = flatten_body(body, 0, &HashSet::new())?;
        all_scoped.extend(flattened);
    }

    // Sort by scope depth - outer scopes must execute before inner
    let mut scoped_steps = all_scoped;
    scoped_steps.sort_by_key(|s| s.depth);

    // Collect all select steps in order
    let mut select: Vec<SelectStep> = vec![];
    let mut select_ast: Option<AstSelector> = None;
    let mut value_pattern: Option<ValuePattern> = None;

    for scoped in &scoped_steps {
        select.extend(scoped.select_steps.clone());
        if let Some(ast) = scoped.ast_selector.clone() {
            select_ast = Some(ast);
        }
        if let Some(val) = scoped.value_pattern.clone() {
            value_pattern = Some(val);
        }
    }

    // Infer create_matches from all $VARs in body.
    // Detect repo()/rev() tags to set scan annotations.
    let mut scan_vars: HashMap<String, String> = HashMap::new();
    collect_scan_annotations(&decl.body, &mut scan_vars);

    let mut all_vars: Vec<String> = vec![];
    for body in &decl.body {
        for cap in body.all_captures() {
            if !all_vars.contains(&cap) {
                all_vars.push(cap);
            }
        }
    }

    let create_matches: Vec<MatchDef> = all_vars
        .iter()
        .map(|var| MatchDef {
            capture: var.clone(),
            kind: var.clone(),
            parent: None,
            scan: scan_vars.get(var).cloned(),
        })
        .collect();

    Ok(Rule {
        name: decl.name.clone(),
        description: None,
        select,
        select_ast,
        value: value_pattern,
        create_matches,
        confidence: None,
    })
}

/// Walk rule bodies to find repo($VAR)/rev($VAR) tags and record their
/// captured variables as scan-driving columns.
fn collect_scan_annotations(bodies: &[RuleBody], scan_vars: &mut HashMap<String, String>) {
    for body in bodies {
        let (slot, children) = match body {
            RuleBody::Step(slot) => (Some(slot), &[][..]),
            RuleBody::Block { slot, children } => (Some(slot), children.as_slice()),
            RuleBody::Ref { children, .. } => (None, children.as_slice()),
        };
        if let Some(Slot::Tagged { tag, body, .. }) = slot {
            match tag {
                Tag::Repo => {
                    for var in slot.unwrap().captures() {
                        scan_vars.insert(var, "repo".to_string());
                    }
                }
                Tag::Rev => {
                    for var in slot.unwrap().captures() {
                        scan_vars.insert(var, "rev".to_string());
                    }
                }
                Tag::Scan => {
                    // Parse scan(repo: $VAR, rev: $VAR) bindings
                    for part in body.split(',') {
                        let part = part.trim();
                        if let Some(colon) = part.find(':') {
                            let kind = part[..colon].trim();
                            let var_str = part[colon + 1..].trim();
                            if var_str.starts_with('$') && var_str.len() > 1 {
                                let var = &var_str[1..];
                                if matches!(kind, "repo" | "rev") {
                                    scan_vars.insert(var.to_string(), kind.to_string());
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        collect_scan_annotations(children, scan_vars);
    }
}

/// Flatten RuleBody into a list of scoped steps.
///
/// `depth` is the current nesting level (0 = top, 1 = first block, etc.)
/// `parent_vars` are variables captured by outer scopes (available to inner)
fn flatten_body(
    body: &RuleBody,
    depth: usize,
    parent_vars: &HashSet<String>,
) -> Result<Vec<ScopedStep>> {
    let mut result = vec![];

    match body {
        RuleBody::Step(slot) => {
            // Convert slot to scoped step
            let scoped = slot_to_scoped_step(slot, depth, parent_vars)?;
            result.push(scoped);
        }
        RuleBody::Block { slot, children } => {
            // The block slot captures variables for this scope
            let block_vars = extract_slot_vars(slot);
            let mut available_vars = parent_vars.clone();
            available_vars.extend(block_vars.iter().cloned());

            // Convert block slot to step
            let block_scoped = slot_to_scoped_step(slot, depth, parent_vars)?;
            result.push(block_scoped);

            // Process children at next depth level
            for child in children {
                let child_scoped = flatten_body(child, depth + 1, &available_vars)?;
                result.extend(child_scoped);
            }
        }
        RuleBody::Ref { cross_ref, children } => {
            // Cross-ref bindings introduce variables at this scope
            let mut available_vars = parent_vars.clone();
            for binding in &cross_ref.bindings {
                available_vars.insert(binding.var.clone());
            }
            // Process children at next depth level
            for child in children {
                let child_scoped = flatten_body(child, depth + 1, &available_vars)?;
                result.extend(child_scoped);
            }
        }
    }

    Ok(result)
}

/// Extract variable names from a slot body.
fn extract_slot_vars(slot: &Slot) -> HashSet<String> {
    let mut vars = HashSet::new();
    let body = match slot {
        Slot::Bare(s) => s.as_str(),
        Slot::Tagged { body, .. } => body.as_str(),
    };

    // Find $SCREAMING variables
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' {
            i += 1;
            if i < bytes.len() && bytes[i] == b'_' {
                i += 1;
                continue; // $_ is wildcard, not a capture
            }
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            if i > start {
                let var = &body[start..i];
                if var
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit())
                {
                    vars.insert(var.to_string());
                }
            }
        } else {
            i += 1;
        }
    }
    vars
}

/// Convert a slot to a scoped step.
fn slot_to_scoped_step(
    slot: &Slot,
    depth: usize,
    available_vars: &HashSet<String>,
) -> Result<ScopedStep> {
    let scope_vars = extract_slot_vars(slot);

    // Validate that all referenced variables are available
    // (This would be where we'd check for forward references)

    let (select_steps, ast_selector, value_pattern) = convert_slot(slot)?;

    Ok(ScopedStep {
        depth,
        scope_vars,
        select_steps,
        ast_selector,
        value_pattern,
    })
}

/// Convert a slot to SelectSteps.
fn convert_slot(
    slot: &Slot,
) -> Result<(Vec<SelectStep>, Option<AstSelector>, Option<ValuePattern>)> {
    let mut select = vec![];
    let mut ast_selector: Option<AstSelector> = None;
    let mut value_pattern: Option<ValuePattern> = None;

    match slot {
        Slot::Bare(pattern) => {
            // Bare glob - infer context
            // For now, treat as file pattern
            select.push(SelectStep::File {
                pattern: pattern.clone(),
                capture: None,
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
            Tag::Rev => {
                select.push(SelectStep::Rev {
                    pattern: body.clone(),
                    capture: None,
                });
            }
            Tag::Folder => {
                select.push(SelectStep::Folder {
                    pattern: body.clone(),
                    capture: None,
                });
            }
            Tag::File => {
                select.push(SelectStep::File {
                    pattern: body.clone(),
                    capture: None,
                });
            }
            Tag::Json => {
                let steps = parse_json_body(body)?;
                select.extend(steps);
            }
            Tag::Ast => {
                if ast_selector.is_some() {
                    bail!("multiple ast() slots not supported");
                }
                ast_selector = Some(AstSelector {
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
                    bail!("multiple re() slots not supported");
                }
                value_pattern = Some(ValuePattern {
                    source: String::new(),
                    pattern: body.clone(),
                    full_match: true,
                });
            }
            Tag::Scan => {
                // Annotation-only, no select step emitted.
                // Scan annotations are collected separately by collect_scan_annotations.
            }
        },
    }

    Ok((select, ast_selector, value_pattern))
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
    fn lower_flat_rule() {
        let rules =
            lower("rule(pkg) { fs(**/Cargo.toml) > json({ package: { name: $NAME } }) };");
        assert_eq!(rules.len(), 1);
        let r = &rules[0];
        assert_eq!(r.name, "pkg");

        assert!(
            matches!(&r.select[0], SelectStep::File { pattern, .. } if pattern == "**/Cargo.toml")
        );
        // select[1] is the Object from json body
        assert!(matches!(&r.select[1], SelectStep::Object { .. }));
    }

    #[test]
    fn lower_scoped_rule() {
        let rules = lower(
            r#"rule(deploy) {
            repo($REPO) {
                fs(**/values.yaml) > json({ svc: $SVC })
            }
        };"#,
        );
        assert_eq!(rules.len(), 1);
        let r = &rules[0];
        assert_eq!(r.name, "deploy");

        // Should have: repo, file, object
        assert!(matches!(&r.select[0], SelectStep::Repo { .. }));
        assert!(matches!(&r.select[1], SelectStep::File { .. }));
        assert!(matches!(&r.select[2], SelectStep::Object { .. }));
    }

    #[test]
    fn lower_nested_scopes() {
        let rules = lower(
            r#"rule(img) {
            repo($REPO) {
                rev(main) {
                    folder(packages/$PKG) {
                        fs(values.yaml) > json({ image: { repo: $REPO, tag: $TAG } })
                    }
                }
            }
        };"#,
        );

        assert_eq!(rules.len(), 1);
        let r = &rules[0];

        // Order: repo, rev, folder, file, object
        assert!(matches!(&r.select[0], SelectStep::Repo { .. }));
        assert!(matches!(&r.select[1], SelectStep::Rev { .. }));
        assert!(matches!(&r.select[2], SelectStep::Folder { .. }));
        assert!(matches!(&r.select[3], SelectStep::File { .. }));
        assert!(matches!(&r.select[4], SelectStep::Object { .. }));
    }

    #[test]
    fn lower_with_scan_annotations() {
        // repo() and rev() tags in body infer scan annotations
        let rules = lower(
            r#"rule(deploy) {
            repo($REPO) {
                rev($TAG) {
                    fs(**/values.yaml) > json({ image: { repo: $REPO, tag: $TAG } })
                }
            }
        };"#,
        );
        let r = &rules[0];
        let repo_match = r.create_matches.iter().find(|m| m.capture == "REPO").unwrap();
        let tag_match = r.create_matches.iter().find(|m| m.capture == "TAG").unwrap();
        assert_eq!(repo_match.scan.as_deref(), Some("repo"));
        assert_eq!(tag_match.scan.as_deref(), Some("rev"));
    }

    #[test]
    fn lower_scan_tag_annotations() {
        // scan() tag marks captures as scan-driving without context steps
        let rules = lower(
            r#"rule(image_refs) {
            fs(**/values.yaml) > json({ image: { repository: $REPO, tag: $TAG } })
            scan(repo: $REPO, rev: $TAG)
        };"#,
        );
        let r = &rules[0];
        let repo_match = r.create_matches.iter().find(|m| m.capture == "REPO").unwrap();
        let tag_match = r.create_matches.iter().find(|m| m.capture == "TAG").unwrap();
        assert_eq!(repo_match.scan.as_deref(), Some("repo"));
        assert_eq!(tag_match.scan.as_deref(), Some("rev"));
        // scan() should NOT produce any select steps
        assert!(r.select.iter().all(|s| !matches!(s, SelectStep::Repo { .. })));
    }

    #[test]
    fn lower_ast_with_lang() {
        let rules = lower("rule(imports) { fs(**/*.config) > ast[typescript](import $NAME from '$PATH') };");
        let r = &rules[0];
        let ast = r.select_ast.as_ref().unwrap();
        assert_eq!(ast.language.as_deref(), Some("typescript"));
        assert_eq!(ast.pattern.as_deref(), Some("import $NAME from '$PATH'"));
    }

    #[test]
    fn lower_re() {
        let rules = lower(
            r"rule(img) { fs(deploy/**/*.yaml) > re(image:\s+(?P<REPO>[^:]+):(?P<TAG>.+)) };",
        );
        let r = &rules[0];
        assert!(r.value.is_some());
        assert_eq!(
            r.value.as_ref().unwrap().pattern,
            r"image:\s+(?P<REPO>[^:]+):(?P<TAG>.+)"
        );
    }
}
