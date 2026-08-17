//! tree-sitter grammar for dl6.

use tree_sitter_language::LanguageFn;

extern "C" {
    fn tree_sitter_dl6() -> *const ::core::ffi::c_void;
}

unsafe extern "C" fn raw() -> *const () {
    tree_sitter_dl6() as *const ()
}

pub const LANGUAGE: LanguageFn = unsafe { LanguageFn::from_raw(raw) };

pub fn language() -> LanguageFn {
    LANGUAGE
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn parse_check(src: &str) -> Result<(), String> {
        let mut parser = Parser::new();
        let lang = tree_sitter::Language::new(LANGUAGE);
        parser
            .set_language(&lang)
            .map_err(|e| format!("set_language error: {e}"))?;
        let tree = parser
            .parse(src, None)
            .ok_or_else(|| "parse returned None".to_string())?;
        let root = tree.root_node();
        if root.has_error() {
            return Err(format!("syntax error in tree: {}", root.to_sexp()));
        }
        Ok(())
    }

    #[test]
    fn parses_simple_relation_and_rule() {
        let src = r#"
rel edge(src: text, dst: text).
edge("a", "b").
path(X, Y) <- edge(X, Y).
? path(X, Y).
"#;
        assert!(parse_check(src).is_ok());
    }

    #[test]
    fn parses_golden_flex_fixture() {
        let src = include_str!("../fixtures/golden-flex-175-236.dl6");
        let result = parse_check(src);
        assert!(result.is_ok(), "golden fixture error: {:?}", result.err());
    }

    #[test]
    fn parses_format_input_fixture() {
        let src = include_str!("../fixtures/format-input.dl6");
        let result = parse_check(src);
        assert!(
            result.is_ok(),
            "format-input fixture error: {:?}",
            result.err()
        );
    }
}
