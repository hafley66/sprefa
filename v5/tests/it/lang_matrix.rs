//! Matrix-honesty rail: the per-op LANGUAGE MATRIX block in the embedded skill
//! (`assets/sprefa-dl.skill.md`) must stay set-equal to the ACTUAL grammar
//! tables — `sg::sg_langs()` (the ast-grep set the `match_ast`/`ast_yaml` ops accept)
//! and `engine::ast_langs()` (the tree-sitter set the `ast` op accepts). Adding
//! a grammar to either table without updating the skill matrix fails this test,
//! so a stale matrix cannot ship.

use std::collections::BTreeSet;

/// Parse a matrix line `<label>: a b c` out of the skill body by its exact
/// trimmed label. Returns the whitespace-split token set. Panics if the label
/// is not present (the block was renamed/removed — also a failure).
fn matrix_row(label: &str) -> BTreeSet<String> {
    let mut found = None;
    for line in sprefa_v5::setup::SKILL_MD.lines() {
        let Some((lhs, rhs)) = line.split_once(':') else {
            continue;
        };
        if lhs.trim() != label {
            continue;
        }
        assert!(
            found.is_none(),
            "matrix label `{label}` appears more than once in the skill"
        );
        found = Some(
            rhs.split_whitespace()
                .map(String::from)
                .collect::<BTreeSet<_>>(),
        );
    }
    found.unwrap_or_else(|| panic!("no `{label}:` language-matrix line in the skill"))
}

#[test]
fn skill_match_ast_matrix_matches_the_grammar_table() {
    let listed = matrix_row("match_ast, ast_yaml");
    let actual: BTreeSet<String> = sprefa_v5::sg::sg_langs()
        .into_iter()
        .map(String::from)
        .collect();
    assert_eq!(
        listed, actual,
        "skill `match_ast, ast_yaml` matrix is stale vs SG_LANG_TABLE (sg::sg_langs())"
    );
}

#[test]
fn skill_ast_matrix_matches_the_grammar_table() {
    let listed = matrix_row("ast");
    let actual: BTreeSet<String> = sprefa_v5::engine::ast_langs()
        .into_iter()
        .map(String::from)
        .collect();
    assert_eq!(
        listed, actual,
        "skill `ast` matrix is stale vs AST_LANG_TABLE (engine::ast_langs())"
    );
}
