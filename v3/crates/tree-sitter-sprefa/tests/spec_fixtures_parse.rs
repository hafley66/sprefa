//! Parse-check the v3-render-write-cursor-vision.md spec fixtures under
//! v3/crates/server/fixtures/. Each fixture's leading comment block
//! declares whether it `PARSES TODAY` or `DOES NOT PARSE TODAY`. This
//! test pins the current state — fixtures move from one bucket to the
//! other as grammar landings ship (sprefa-p0m).

use std::fs;
use tree_sitter::{Language, Parser};
use tree_sitter_sprefa::LANGUAGE;

fn parses(src: &str) -> bool {
    let mut p = Parser::new();
    let lang: Language = LANGUAGE.into();
    p.set_language(&lang).unwrap();
    let tree = p.parse(src, None).unwrap();
    !tree.root_node().has_error()
}

fn read(name: &str) -> String {
    let mut path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // crates/tree-sitter-sprefa -> crates/server/fixtures
    path.pop();
    path.push("server/fixtures");
    path.push(name);
    fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[test]
fn parses_today_fixtures_are_clean() {
    // Fixtures whose op-shapes use only currently-landed grammar
    // (no dotted carveout access, no @begin/@end markers, no backticks).
    let names = ["lsp_severity_smoke.sprf"];

    let mut failures = Vec::new();
    for name in names {
        let src = read(name);
        if !parses(&src) {
            failures.push(name);
        }
    }
    assert!(
        failures.is_empty(),
        "fixtures declared PARSES TODAY produced ERROR nodes: {failures:?}"
    );
}

#[test]
fn future_fixtures_do_not_parse_yet() {
    // Each entry depends on a grammar landing pending sprefa-p0m:
    //   render_md, render_ascii  -> dotted carveout access ${X.field?}
    //   tag                      -> dotted carveout access
    //   write_cursor, comment_*  -> @begin/@end marker tokens
    //   backtick_template        -> backtick string token
    // When any one moves to PARSES TODAY, flip its entry over.
    let names = [
        "render_md_smoke.sprf",
        "render_ascii_smoke.sprf",
        "tag_smoke.sprf",
        "write_cursor_smoke.sprf",
        "comment_sprf_eval_smoke.sprf",
        "backtick_template_smoke.sprf",
    ];
    for name in names {
        let src = read(name);
        assert!(
            !parses(&src),
            "{name} now parses cleanly; move it into the \
             PARSES TODAY list and update its header"
        );
    }
}
