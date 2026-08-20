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

    #[test]
    fn parses_anonymous_type_fixture() {
        let src = include_str!("../fixtures/anonymous-types.dl6");
        let result = parse_check(src);
        assert!(
            result.is_ok(),
            "anonymous-types fixture error: {:?}",
            result.err()
        );
    }

    #[test]
    fn parses_direct_type_relation_calls() {
        let src = r#"
rel annotated(id: key(int), configured: configure(int, Value: 1), composed: second(first(int))).
"#;
        let result = parse_check(src);
        assert!(result.is_ok(), "type-call error: {:?}", result.err());
        let mut parser = Parser::new();
        let lang = tree_sitter::Language::new(LANGUAGE);
        parser.set_language(&lang).unwrap();
        let tree = parser.parse(src, None).unwrap();
        let sexp = tree.root_node().to_sexp();
        assert!(sexp.contains("type_argument"));
        assert!(sexp.contains("type_named_argument"));
    }

    #[test]
    fn parses_native_json_literals() {
        let src = r#"
doc(Value) <- seed(Id), Value := {"z": [true, null, 3.5, {}, []], "a": "text"}.
"#;
        let result = parse_check(src);
        assert!(result.is_ok(), "native JSON error: {:?}", result.err());
        let mut parser = Parser::new();
        let lang = tree_sitter::Language::new(LANGUAGE);
        parser.set_language(&lang).unwrap();
        let tree = parser.parse(src, None).unwrap();
        let sexp = tree.root_node().to_sexp();
        assert!(sexp.contains("json_object"));
        assert!(sexp.contains("json_array"));
        assert!(sexp.contains("json_pair"));
    }
}
