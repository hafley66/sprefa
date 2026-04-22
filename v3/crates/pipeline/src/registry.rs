//! Operator registry. Maps op name → factory closure.
//!
//! Post-§14.5m Pass B: registration goes through trait impls
//! ([`crate::op_ctor::OpCtor`], [`crate::op_ctor::PatternCtor`],
//! [`crate::op_ctor::OpLowering`]) via the `register::<O>()`,
//! `register_pattern::<O>()`, and `register_custom::<L>()` builders.
//! Each op carries its own NAME / DOC / language() / from_values
//! straight off the type — `with_stdlib` is now one line per op.
//!
//! Three factory surfaces still live underneath the trait facade:
//!   - `OpFactory`: `(name, Vec<Value>) -> Box<dyn Op>` — the value-arg
//!     door used at pipe-step and for ops constructed from lowered
//!     argument values.
//!   - `PatternOpFactory`: `(tree, bytes) -> Arc<dyn Op>` — the
//!     compile-from-injected-tree door used when a pattern op appears
//!     inside an arg slot.
//!   - `CustomLowerFn`: raw-CST escape hatch for ops that need to
//!     inspect sibling nodes at lower time (no stdlib consumer yet).
//!
//! The pattern sub-grammar `Language` and `highlights` strings now
//! live in the registry (`pattern_language(name)`, `pattern_highlights
//! (name)`) instead of being looked up by name in `op_languages`. The
//! build-script-generated `op_languages` module remains as the C-FFI
//! linkage thunk; `PatternCtor::language()` impls call into it directly.

use std::collections::HashMap;
use std::sync::Arc;

use tree_sitter::{Language, Node, Parser, Tree};

use crate::_1_op::Op;
use crate::op_ctor::{op_factory, CustomLowerFn, OpCtor, OpLowering, PatternCtor};
use crate::ops::{
    CaptureWriteOp, CommentOp, FsOp, GlobOp, PrintOp, ReadOp, ReOp, RepoOp, RevOp, StrOp, VoidOp,
};
use crate::pattern_op::PatternDiagnostic;
use crate::value::Value;

/// Factory signature for ops constructed from lowered values.
pub type OpFactory =
    dyn Fn(&str, Vec<Value>) -> Result<Box<dyn Op>, Vec<PatternDiagnostic>> + Send + Sync;

/// Factory signature for pattern ops compiled from injected sub-trees.
type PatternOpFactory =
    dyn Fn(&Tree, &[u8]) -> Result<Arc<dyn Op>, Vec<PatternDiagnostic>> + Send + Sync;

/// Op name → factory map. Clone-cheap (`Arc` inside).
#[derive(Clone, Default)]
pub struct Registry {
    factories:         Arc<HashMap<&'static str, Arc<OpFactory>>>,
    pattern_factories: Arc<HashMap<&'static str, Arc<PatternOpFactory>>>,
    custom_lowerers:   Arc<HashMap<&'static str, Arc<CustomLowerFn>>>,
    languages:         Arc<HashMap<&'static str, Language>>,
    highlights:        Arc<HashMap<&'static str, &'static str>>,
    docs:              Arc<HashMap<&'static str, &'static str>>,
}

struct Builder {
    factories:         HashMap<&'static str, Arc<OpFactory>>,
    pattern_factories: HashMap<&'static str, Arc<PatternOpFactory>>,
    custom_lowerers:   HashMap<&'static str, Arc<CustomLowerFn>>,
    languages:         HashMap<&'static str, Language>,
    highlights:        HashMap<&'static str, &'static str>,
    docs:              HashMap<&'static str, &'static str>,
}

impl Builder {
    fn new() -> Self {
        Self {
            factories:         HashMap::new(),
            pattern_factories: HashMap::new(),
            custom_lowerers:   HashMap::new(),
            languages:         HashMap::new(),
            highlights:        HashMap::new(),
            docs:              HashMap::new(),
        }
    }

    fn register<O: OpCtor>(mut self) -> Self {
        self.factories.insert(O::NAME, op_factory::<O>());
        self.docs.insert(O::NAME, O::DOC);
        self
    }

    fn register_pattern<O: PatternCtor>(mut self) -> Self {
        self.pattern_factories.insert(
            O::NAME,
            Arc::new(|tree, bytes| {
                O::compile_from_tree(tree, bytes).map(|op| Arc::new(op) as Arc<dyn Op>)
            }),
        );
        self.languages.insert(O::NAME, <O as PatternCtor>::language());
        if let Some(h) = <O as PatternCtor>::highlights() {
            self.highlights.insert(O::NAME, h);
        }
        self.docs.insert(O::NAME, O::DOC);
        self
    }

    #[allow(dead_code)] // Escape hatch for ops needing raw CST access at lower
                        // time (canonical: tag). Intentional unused-until-tag.
    fn register_custom<L: OpLowering>(mut self) -> Self {
        self.custom_lowerers.insert(
            L::NAME,
            Arc::new(|node, src, registry, diags| L::lower(node, src, registry, diags)),
        );
        if let Some(lang) = L::language() {
            self.languages.insert(L::NAME, lang);
        }
        self.docs.insert(L::NAME, L::DOC);
        self
    }

    fn finish(self) -> Registry {
        Registry {
            factories:         Arc::new(self.factories),
            pattern_factories: Arc::new(self.pattern_factories),
            custom_lowerers:   Arc::new(self.custom_lowerers),
            languages:         Arc::new(self.languages),
            highlights:        Arc::new(self.highlights),
            docs:              Arc::new(self.docs),
        }
    }
}

impl Registry {
    pub fn with_stdlib() -> Self {
        Builder::new()
            .register::<RepoOp>()
            .register::<RevOp>()
            .register::<FsOp>()
            .register::<VoidOp>()
            .register::<StrOp>()
            .register::<CommentOp>()
            .register::<PrintOp>()
            .register::<ReadOp>()
            .register_pattern::<GlobOp>()
            .register_pattern::<ReOp>()
            .finish()
    }

    pub fn capture_write(target: impl Into<Arc<str>>) -> Box<dyn Op> {
        Box::new(CaptureWriteOp::new(target))
    }

    pub fn build(
        &self,
        op_name: &str,
        values: Vec<Value>,
    ) -> Option<Result<Box<dyn Op>, Vec<PatternDiagnostic>>> {
        let factory = self.factories.get(op_name)?;
        Some(factory(op_name, values))
    }

    /// Compile a pattern-op body into an `Arc<dyn Op>`. Returns `None`
    /// when `op_name` is not registered as a pattern op.
    pub fn compile_pattern(
        &self,
        op_name: &str,
        tree: &Tree,
        bytes: &[u8],
    ) -> Option<Result<Arc<dyn Op>, Vec<PatternDiagnostic>>> {
        let pfactory = self.pattern_factories.get(op_name)?;
        Some(pfactory(tree, bytes))
    }

    pub fn is_pattern_op(&self, op_name: &str) -> bool {
        self.pattern_factories.contains_key(op_name)
    }

    /// Sub-grammar `Language` for a pattern (or custom-lower) op.
    /// `None` for plain value-arg ops.
    pub fn pattern_language(&self, op_name: &str) -> Option<Language> {
        self.languages.get(op_name).cloned()
    }

    /// Editor highlight query string for a pattern op's sub-grammar.
    pub fn pattern_highlights(&self, op_name: &str) -> Option<&'static str> {
        self.highlights.get(op_name).copied()
    }

    /// Run an op-defined custom lowerer over a raw `op_invocation` node.
    /// Returns `None` when `op_name` is not registered via
    /// `register_custom`. No stdlib op uses this yet.
    pub fn custom_lower(
        &self,
        op_name:     &str,
        node:        Node<'_>,
        src:         &[u8],
        diagnostics: &mut Vec<PatternDiagnostic>,
    ) -> Option<Value> {
        let f = self.custom_lowerers.get(op_name)?;
        Some(f(node, src, self, diagnostics))
    }

    pub fn doc(&self, op_name: &str) -> Option<&'static str> {
        self.docs.get(op_name).copied()
    }

    pub fn names(&self) -> Vec<&'static str> {
        self.factories.keys().copied().collect()
    }
}

// ---------------------------------------------------------------------------
// lower_paren_slot — host CST paren_slot node → Vec<Value>
// ---------------------------------------------------------------------------

pub fn lower_paren_slot(
    paren_node: Node<'_>,
    src: &[u8],
    registry: &Registry,
    diagnostics: &mut Vec<PatternDiagnostic>,
) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();

    let mut cursor = paren_node.walk();
    for child in paren_node.named_children(&mut cursor) {
        let kind = child.kind();
        match kind {
            "op_invocation" => {
                let val = lower_nested_op_invocation(child, src, registry, diagnostics);
                out.push(val);
            }
            "term_ref" => {
                let name = extract_term_ref_name(child, src);
                out.push(Value::Term(Arc::from(name)));
            }
            "atom_literal" => {
                let range = child.byte_range();
                let bytes = &src[range.start + 1..range.end];
                let s = std::str::from_utf8(bytes).unwrap_or("");
                out.push(Value::Atom(Arc::from(s)));
            }
            "string_literal" => {
                let s = process_string_literal(child, src);
                out.push(Value::Str(Arc::from(s)));
            }
            "raw_string_literal" => {
                let s = process_raw_string_literal(child, src);
                out.push(Value::Str(Arc::from(s)));
            }
            "number_literal" => {
                let range = child.byte_range();
                let text = std::str::from_utf8(&src[range]).unwrap_or("0");
                if let Ok(n) = text.parse::<i64>() {
                    out.push(Value::Int(n));
                } else if let Ok(f) = text.parse::<f64>() {
                    out.push(Value::Float(f));
                } else {
                    diagnostics.push(PatternDiagnostic {
                        code: "value/bad-number",
                        message: format!("cannot parse `{text}` as a number"),
                        byte_range: child.byte_range(),
                    });
                }
            }
            "identifier" => {
                let range = child.byte_range();
                let text = std::str::from_utf8(&src[range]).unwrap_or("");
                diagnostics.push(PatternDiagnostic {
                    code: "value/bare-ident",
                    message: format!(
                        "bare identifier `{text}` in value position; \
                         use :{text} for an atom or str({text}) for a literal"
                    ),
                    byte_range: child.byte_range(),
                });
                out.push(Value::Atom(Arc::from(text)));
            }
            _ => {}
        }
    }
    out
}

fn lower_nested_op_invocation(
    node:        Node<'_>,
    src:         &[u8],
    registry:    &Registry,
    diagnostics: &mut Vec<PatternDiagnostic>,
) -> Value {
    let name_node = match node.child_by_field_name("name") {
        Some(n) => n,
        None => {
            diagnostics.push(PatternDiagnostic {
                code: "value/bad-nested-op",
                message: "nested op_invocation has no name".into(),
                byte_range: node.byte_range(),
            });
            return Value::Atom(Arc::from(""));
        }
    };
    let op_name = match std::str::from_utf8(&src[name_node.byte_range()]) {
        Ok(s) => s,
        Err(_) => {
            diagnostics.push(PatternDiagnostic {
                code: "value/bad-nested-op",
                message: "op name is not valid UTF-8".into(),
                byte_range: name_node.byte_range(),
            });
            return Value::Atom(Arc::from(""));
        }
    };

    // Custom-lowering door takes priority. Op owns the whole CST walk.
    if let Some(v) = registry.custom_lower(op_name, node, src, diagnostics) {
        return v;
    }

    // Pattern-op path: parse the injected body, compile to Arc<dyn Op>.
    if registry.is_pattern_op(op_name) {
        if let Some(lang) = registry.pattern_language(op_name) {
            let body_src = extract_op_body_str(node, src);
            let mut parser = Parser::new();
            if parser.set_language(&lang).is_ok() {
                if let Some(sub_tree) = parser.parse(body_src, None) {
                    match registry.compile_pattern(op_name, &sub_tree, body_src.as_bytes()) {
                        Some(Ok(op)) => return Value::Op(op),
                        Some(Err(errs)) => {
                            diagnostics.extend(errs);
                            return Value::Atom(Arc::from(op_name));
                        }
                        None => {}
                    }
                }
            }
        }
    }

    let has_paren = node.child_by_field_name("paren").is_some();
    let has_bracket = node.child_by_field_name("bracket").is_some();
    let has_brace = node.child_by_field_name("brace").is_some();
    let is_call = has_paren || has_bracket || has_brace;

    if !is_call && registry.build(op_name, vec![]).is_none() && !registry.is_pattern_op(op_name) {
        diagnostics.push(PatternDiagnostic {
            code: "value/bare-ident",
            message: format!(
                "bare identifier `{op_name}` in value position; \
                 use :{op_name} for an atom or str({op_name}) for a literal"
            ),
            byte_range: node.byte_range(),
        });
        return Value::Atom(Arc::from(op_name));
    }

    let inner_values = if let Some(paren) = node.child_by_field_name("paren") {
        lower_paren_slot(paren, src, registry, diagnostics)
    } else {
        Vec::new()
    };

    match registry.build(op_name, inner_values) {
        Some(Ok(op)) => Value::Op(Arc::from(op)),
        Some(Err(errs)) => {
            diagnostics.extend(errs);
            Value::Atom(Arc::from(op_name))
        }
        None => {
            diagnostics.push(PatternDiagnostic {
                code: "value/unknown-op",
                message: format!("unknown op `{op_name}` in arg position"),
                byte_range: node.byte_range(),
            });
            Value::Atom(Arc::from(op_name))
        }
    }
}

fn extract_op_body_str<'a>(node: Node<'_>, src: &'a [u8]) -> &'a str {
    let Some(paren) = node.child_by_field_name("paren") else {
        return "";
    };
    let range = paren.byte_range();
    if range.end <= range.start + 2 {
        return "";
    }
    std::str::from_utf8(&src[range.start + 1..range.end.saturating_sub(1)]).unwrap_or("")
}

fn extract_term_ref_name<'a>(node: Node<'_>, src: &'a [u8]) -> &'a str {
    let range = node.byte_range();
    let bytes = &src[range];
    let rest = if bytes.first() == Some(&b'$') { &bytes[1..] } else { bytes };
    let rest = if rest.first() == Some(&b'{') && rest.last() == Some(&b'}') {
        &rest[1..rest.len() - 1]
    } else {
        rest
    };
    std::str::from_utf8(rest).unwrap_or("")
}

fn process_string_literal(node: Node<'_>, src: &[u8]) -> String {
    let range = node.byte_range();
    let bytes = &src[range];
    let inner = if bytes.len() >= 2 && bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"' {
        &bytes[1..bytes.len() - 1]
    } else {
        bytes
    };
    let mut out = String::with_capacity(inner.len());
    let mut i = 0;
    while i < inner.len() {
        if inner[i] == b'\\' && i + 1 < inner.len() {
            match inner[i + 1] {
                b'n'  => { out.push('\n'); i += 2; }
                b't'  => { out.push('\t'); i += 2; }
                b'r'  => { out.push('\r'); i += 2; }
                b'\\' => { out.push('\\'); i += 2; }
                b'"'  => { out.push('"');  i += 2; }
                other => {
                    out.push('\\');
                    out.push(other as char);
                    i += 2;
                }
            }
        } else {
            out.push(inner[i] as char);
            i += 1;
        }
    }
    out
}

fn process_raw_string_literal(node: Node<'_>, src: &[u8]) -> String {
    let range = node.byte_range();
    let bytes = &src[range];
    let mut i = 0;
    if i < bytes.len() && bytes[i] == b'r' { i += 1; }
    let hashes = bytes[i..].iter().take_while(|&&b| b == b'#').count();
    i += hashes;
    if i < bytes.len() && bytes[i] == b'"' { i += 1; }
    let end = bytes.len().saturating_sub(1 + hashes);
    if i < end {
        String::from_utf8_lossy(&bytes[i..end]).into_owned()
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn str_val(s: &str) -> Value {
        Value::Str(Arc::from(s))
    }

    #[test]
    fn stdlib_roundtrip() {
        let r = Registry::with_stdlib();

        let built = r.build("repo", vec![str_val("myorg/*")]).unwrap().unwrap();
        assert_eq!(built.name(), "repo");

        assert!(r.build("nope", vec![]).is_none());

        match r.build("fs", vec![]).unwrap() {
            Ok(_) => panic!("expected fs/missing-arg diagnostic"),
            Err(errs) => {
                assert_eq!(errs.len(), 1);
                assert_eq!(errs[0].code, "fs/missing-arg");
            }
        }
    }

    #[test]
    fn docs_cover_every_registered_name() {
        let r = Registry::with_stdlib();
        for name in r.names() {
            assert!(
                r.doc(name).is_some(),
                "no hover doc registered for `{name}`"
            );
        }
    }

    #[test]
    fn stdlib_names_match_expected_set() {
        let r = Registry::with_stdlib();
        let mut names = r.names();
        names.sort();
        assert_eq!(
            names,
            vec!["comment", "fs", "print", "read", "repo", "rev", "str", "void"]
        );
    }

    #[test]
    fn pattern_op_detection() {
        let r = Registry::with_stdlib();
        assert!(r.is_pattern_op("glob"));
        assert!(r.is_pattern_op("re"));
        assert!(!r.is_pattern_op("repo"));
        assert!(!r.is_pattern_op("fs"));
        assert!(!r.is_pattern_op("void"));
    }

    #[test]
    fn pattern_languages_registered() {
        let r = Registry::with_stdlib();
        assert!(r.pattern_language("glob").is_some());
        assert!(r.pattern_language("re").is_some());
        assert!(r.pattern_language("fs").is_none());
    }

    #[test]
    fn pattern_highlights_registered() {
        let r = Registry::with_stdlib();
        assert!(r.pattern_highlights("glob").is_some());
        assert!(r.pattern_highlights("re").is_some());
        assert!(r.pattern_highlights("repo").is_none());
    }

    #[test]
    fn lower_paren_slot_basic_value_kinds() {
        use tree_sitter::Parser;

        let src = "repo(:main)";
        let mut p = Parser::new();
        p.set_language(&tree_sitter_sprefa::LANGUAGE.into()).unwrap();
        let tree = p.parse(src, None).unwrap();
        let root = tree.root_node();
        let pipe = root.named_child(0).unwrap();
        let op   = pipe.named_child(0).unwrap();
        let paren = op.child_by_field_name("paren").unwrap();

        let r = Registry::with_stdlib();
        let mut diags: Vec<PatternDiagnostic> = Vec::new();
        let values = lower_paren_slot(paren, src.as_bytes(), &r, &mut diags);

        assert!(diags.is_empty(), "unexpected diags: {diags:?}");
        assert_eq!(values.len(), 1);
        assert!(matches!(&values[0], Value::Atom(s) if s.as_ref() == "main"));
    }

    #[test]
    fn lower_paren_slot_nested_glob_yields_op_value() {
        use tree_sitter::Parser;

        let src = "fs(glob(**/*.rs))";
        let mut p = Parser::new();
        p.set_language(&tree_sitter_sprefa::LANGUAGE.into()).unwrap();
        let tree = p.parse(src, None).unwrap();
        let root = tree.root_node();
        let pipe = root.named_child(0).unwrap();
        let op   = pipe.named_child(0).unwrap();
        let paren = op.child_by_field_name("paren").unwrap();

        let r = Registry::with_stdlib();
        let mut diags: Vec<PatternDiagnostic> = Vec::new();
        let values = lower_paren_slot(paren, src.as_bytes(), &r, &mut diags);

        assert!(diags.is_empty(), "unexpected diags: {diags:?}");
        assert_eq!(values.len(), 1);
        match &values[0] {
            Value::Op(op) => {
                assert_eq!(op.name(), "glob");
                assert!(op.try_raw_regex().is_some());
                assert!(op.try_raw_regex().unwrap().as_str().starts_with('^'));
            }
            other => panic!("expected Value::Op(glob), got {other:?}"),
        }
    }

    #[test]
    fn lower_paren_slot_term_ref() {
        use tree_sitter::Parser;

        let src = "repo($R)";
        let mut p = Parser::new();
        p.set_language(&tree_sitter_sprefa::LANGUAGE.into()).unwrap();
        let tree = p.parse(src, None).unwrap();
        let root = tree.root_node();
        let pipe = root.named_child(0).unwrap();
        let op   = pipe.named_child(0).unwrap();
        let paren = op.child_by_field_name("paren").unwrap();

        let r = Registry::with_stdlib();
        let mut diags: Vec<PatternDiagnostic> = Vec::new();
        let values = lower_paren_slot(paren, src.as_bytes(), &r, &mut diags);

        assert!(diags.is_empty(), "unexpected diags: {diags:?}");
        assert_eq!(values.len(), 1);
        assert!(matches!(&values[0], Value::Term(n) if n.as_ref() == "R"));
    }

    #[test]
    fn lower_paren_slot_bare_ident_diagnoses() {
        use tree_sitter::Parser;

        let src = "foo(main)";
        let mut p = Parser::new();
        p.set_language(&tree_sitter_sprefa::LANGUAGE.into()).unwrap();
        let tree = p.parse(src, None).unwrap();
        let root = tree.root_node();
        let pipe = root.named_child(0).unwrap();
        let op   = pipe.named_child(0).unwrap();
        let paren = op.child_by_field_name("paren").unwrap();

        let r = Registry::with_stdlib();
        let mut diags: Vec<PatternDiagnostic> = Vec::new();
        let values = lower_paren_slot(paren, src.as_bytes(), &r, &mut diags);

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, "value/bare-ident");
        assert_eq!(values.len(), 1);
    }

    #[test]
    fn lower_paren_slot_two_args_comma() {
        use tree_sitter::Parser;

        let src = r#"comment("SECTION:", "END:")"#;
        let mut p = Parser::new();
        p.set_language(&tree_sitter_sprefa::LANGUAGE.into()).unwrap();
        let tree = p.parse(src, None).unwrap();
        let root = tree.root_node();
        let pipe = root.named_child(0).unwrap();
        let op   = pipe.named_child(0).unwrap();
        let paren = op.child_by_field_name("paren").unwrap();

        let r = Registry::with_stdlib();
        let mut diags: Vec<PatternDiagnostic> = Vec::new();
        let values = lower_paren_slot(paren, src.as_bytes(), &r, &mut diags);

        assert!(diags.is_empty(), "unexpected diags: {diags:?}");
        assert_eq!(values.len(), 2);
        assert!(matches!(&values[0], Value::Str(s) if s.as_ref() == "SECTION:"));
        assert!(matches!(&values[1], Value::Str(s) if s.as_ref() == "END:"));
    }
}
