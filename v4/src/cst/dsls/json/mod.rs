//! `json` — brace-pattern walker over JSON/YAML/TOML documents.
//!
//! Hand-rolled: NOT tree-sitter-backed at the query-DSL surface (the body
//! parser is in [`walk::brace_parse`]). At match time the target document
//! is parsed by one of the [`data`] format adapters and the compiled
//! brace-pattern walks it.
//!
//! Sprf-blind. Faithful 1:1 port from v3/crates/pipeline/src/walk/ +
//! v3/crates/pipeline/src/data/. Sprf carveouts (Cursor, Capture span_backed,
//! TermPosition, ext-routed dispatch via cursor.fs) are dropped at the
//! adapter; the underlying walk + data crates stay verbatim.

pub mod data;
pub mod walk;

use std::ops::ControlFlow;
use std::sync::Arc;

use crate::cst::diag::{Diag, DiagSink};
use crate::cst::dsl::{CaptureKind, CaptureRow, CaptureSink, Compiled, Dsl};
use crate::cst::lsp::providers::{DslBodyLsp, SemanticToken};
use crate::cst::lsp::highlights::Legend;

use data::{AnyDataNode, JsonNode, DataNode};
use walk::brace_parse::parse_body;
use walk::compile::compile_steps;
use walk::compiled::CompiledStep;
use walk::walker::walk;

/// JSON brace-pattern DSL. Defaults to JSON for the target document parse;
/// consumers needing YAML/TOML construct the compiled pattern directly via
/// [`JsonCompiled::with_format`] or wrap this DSL.
pub struct JsonDsl;

impl Default for JsonDsl {
    fn default() -> Self { Self }
}

impl JsonDsl {
    pub fn new() -> Self { Self }

    /// Typed compile — returns a `JsonCompiled` directly (bypasses the
    /// `dyn Compiled` boxing) so callers that need `match_grouped` or
    /// `with_format` don't have to downcast.
    pub fn compile_typed(body: &[u8]) -> Result<JsonCompiled, Diag> {
        let body_str = std::str::from_utf8(body).map_err(|e| {
            Diag::error("json.utf8", format!("body not utf-8: {e}"), 0..body.len())
        })?;
        let trimmed = body_str.trim();
        if trimmed.is_empty() {
            return Err(Diag::error(
                "json.empty-body",
                "json body is empty; expected a brace pattern, e.g. { key: $V }",
                0..body.len(),
            ));
        }
        let (steps, _annotations, _positions) = parse_body(body_str)
            .map_err(|e| Diag::error("json.parse-body", e, 0..body.len()))?;
        let compiled = compile_steps(&steps)
            .map_err(|e| Diag::error("json.compile", e, 0..body.len()))?;
        Ok(JsonCompiled {
            steps:  Arc::from(compiled.into_boxed_slice()),
            format: TargetFormat::Json,
        })
    }
}

impl Dsl for JsonDsl {
    fn id(&self) -> &'static str { "json" }

    fn lsp(&self) -> Option<&dyn DslBodyLsp> { Some(self) }

    fn compile(
        &self,
        body:  &[u8],
        diags: &dyn DiagSink,
    ) -> Result<Box<dyn Compiled>, Diag> {
        let body_str = std::str::from_utf8(body).map_err(|e| {
            Diag::error("json.utf8", format!("body not utf-8: {e}"), 0..body.len())
        })?;
        let trimmed = body_str.trim();
        if trimmed.is_empty() {
            return Err(Diag::error(
                "json.empty-body",
                "json body is empty; expected a brace pattern, e.g. { key: $V }",
                0..body.len(),
            ));
        }

        let (steps, _annotations, _positions) = parse_body(body_str)
            .map_err(|e| Diag::error("json.parse-body", e, 0..body.len()))?;

        let compiled = compile_steps(&steps)
            .map_err(|e| Diag::error("json.compile", e, 0..body.len()))?;

        let _ = diags;

        Ok(Box::new(JsonCompiled {
            steps:  Arc::from(compiled.into_boxed_slice()),
            format: TargetFormat::Json,
        }))
    }
}

#[derive(Clone, Copy, Debug)]
pub enum TargetFormat { Json, Yaml, Toml }

pub struct JsonCompiled {
    pub steps:  Arc<[CompiledStep]>,
    pub format: TargetFormat,
}

impl JsonCompiled {
    pub fn with_format(mut self, fmt: TargetFormat) -> Self {
        self.format = fmt;
        self
    }

    /// Run the compiled pattern against `target` and return one
    /// `Vec<(name, byte_lo, byte_hi, value)>` per matched row, preserving
    /// row boundaries (unlike `match_into` which flattens via the
    /// `CaptureSink` trait). `target_off` is added to byte offsets so
    /// callers can pass document slices.
    pub fn match_grouped(
        &self,
        target:     &[u8],
        target_off: usize,
    ) -> Vec<Vec<(Arc<str>, usize, usize, Arc<str>)>> {
        let parsed: AnyDataNode = match self.format {
            TargetFormat::Json => match JsonNode::parse(Arc::from(target)) {
                Ok(n)  => AnyDataNode::Json(n),
                Err(_) => return Vec::new(),
            },
            TargetFormat::Yaml => match data::YamlNode::parse(Arc::from(target)) {
                Ok(n)  => AnyDataNode::Yaml(n),
                Err(_) => return Vec::new(),
            },
            TargetFormat::Toml => match data::TomlNode::parse(Arc::from(target)) {
                Ok(n)  => AnyDataNode::Toml(n),
                Err(_) => return Vec::new(),
            },
        };
        let outcome = walk(&parsed, &self.steps);
        outcome.rows.into_iter().map(|row| {
            row.captures.into_iter().map(|(name, wc)| {
                (
                    name,
                    target_off + wc.byte_start as usize,
                    target_off + wc.byte_end   as usize,
                    wc.text,
                )
            }).collect()
        }).collect()
    }
}

impl Compiled for JsonCompiled {
    fn match_into(&self, target: &[u8], target_off: usize, sink: &mut dyn CaptureSink) {
        let parsed: AnyDataNode = match self.format {
            TargetFormat::Json => match JsonNode::parse(Arc::from(target)) {
                Ok(n)  => AnyDataNode::Json(n),
                Err(_) => return,
            },
            TargetFormat::Yaml => match data::YamlNode::parse(Arc::from(target)) {
                Ok(n)  => AnyDataNode::Yaml(n),
                Err(_) => return,
            },
            TargetFormat::Toml => match data::TomlNode::parse(Arc::from(target)) {
                Ok(n)  => AnyDataNode::Toml(n),
                Err(_) => return,
            },
        };

        let outcome = walk(&parsed, &self.steps);
        for row in outcome.rows {
            for (name, wc) in row.captures {
                let abs_start = target_off + wc.byte_start as usize;
                let abs_end   = target_off + wc.byte_end   as usize;
                let row = CaptureRow {
                    name,
                    kind: CaptureKind::Span { byte_range: abs_start..abs_end },
                };
                if let ControlFlow::Break(_) = sink.emit(row) { return; }
            }
        }
    }
}

// ─── DslBodyLsp ───────────────────────────────────────────────────────────
// Body-pure LSP features. The brace-pattern body is hand-rolled; semantic
// tokens come from a quick byte scan that mirrors `brace_parse` token
// classes. Completion suggests the structural primitives (`**`, `*`,
// `...`, `[`, `]`, `{`, `}`) and any `$IDENT` capture names already
// declared earlier in the body. Hover surfaces the role of each token.

impl DslBodyLsp for JsonDsl {
    fn semantic_tokens(&self, body: &[u8]) -> Vec<SemanticToken> {
        let legend = Legend::standard();
        use lsp_types::SemanticTokenType as T;
        let ty_var    = legend.type_index(&T::VARIABLE).unwrap_or(0);
        let ty_prop   = legend.type_index(&T::PROPERTY).unwrap_or(0);
        let ty_kw     = legend.type_index(&T::KEYWORD).unwrap_or(0);
        let ty_str    = legend.type_index(&T::STRING).unwrap_or(0);
        let ty_num    = legend.type_index(&T::NUMBER).unwrap_or(0);
        let ty_op     = legend.type_index(&T::OPERATOR).unwrap_or(0);

        let mut out = Vec::new();
        let s = match std::str::from_utf8(body) { Ok(s) => s, Err(_) => return out };
        let bytes = s.as_bytes();
        let mut i = 0usize;
        while i < bytes.len() {
            let b = bytes[i];
            // capture sigil $NAME
            if b == b'$' && i + 1 < bytes.len()
                && (bytes[i+1].is_ascii_alphabetic() || bytes[i+1] == b'_')
            {
                let lo = i;
                let mut j = i + 1;
                while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                    j += 1;
                }
                out.push(SemanticToken { byte_range: lo..j, token_type: ty_var, token_modifiers: 0 });
                i = j; continue;
            }
            // host hole ${...}
            if b == b'$' && i + 1 < bytes.len() && bytes[i+1] == b'{' {
                let lo = i;
                let mut j = i + 2;
                while j < bytes.len() && bytes[j] != b'}' { j += 1; }
                let hi = (j + 1).min(bytes.len());
                out.push(SemanticToken { byte_range: lo..hi, token_type: ty_var, token_modifiers: 0 });
                i = hi; continue;
            }
            // string literal "..."
            if b == b'"' {
                let lo = i;
                let mut j = i + 1;
                while j < bytes.len() && bytes[j] != b'"' {
                    if bytes[j] == b'\\' && j + 1 < bytes.len() { j += 2; } else { j += 1; }
                }
                let hi = (j + 1).min(bytes.len());
                out.push(SemanticToken { byte_range: lo..hi, token_type: ty_str, token_modifiers: 0 });
                i = hi; continue;
            }
            // number
            if b.is_ascii_digit() || (b == b'-' && i + 1 < bytes.len() && bytes[i+1].is_ascii_digit()) {
                let lo = i;
                let mut j = i + 1;
                while j < bytes.len() && (bytes[j].is_ascii_digit() || bytes[j] == b'.') { j += 1; }
                out.push(SemanticToken { byte_range: lo..j, token_type: ty_num, token_modifiers: 0 });
                i = j; continue;
            }
            // structural punctuation
            if matches!(b, b'{' | b'}' | b'[' | b']' | b':' | b',') {
                out.push(SemanticToken { byte_range: i..i+1, token_type: ty_op, token_modifiers: 0 });
                i += 1; continue;
            }
            // wildcard / glob keywords
            if b == b'*' {
                let lo = i;
                let mut j = i + 1;
                if j < bytes.len() && bytes[j] == b'*' { j += 1; }
                out.push(SemanticToken { byte_range: lo..j, token_type: ty_kw, token_modifiers: 0 });
                i = j; continue;
            }
            // bare key (alphanumeric run)
            if b.is_ascii_alphabetic() || b == b'_' {
                let lo = i;
                let mut j = i + 1;
                while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                    j += 1;
                }
                out.push(SemanticToken { byte_range: lo..j, token_type: ty_prop, token_modifiers: 0 });
                i = j; continue;
            }
            i += 1;
        }
        out
    }

    fn hover(&self, body: &[u8], byte: usize) -> Option<lsp_types::Hover> {
        if byte >= body.len() { return None; }
        let s = std::str::from_utf8(body).ok()?;
        let bytes = s.as_bytes();
        // Walk the cursor byte to its containing token; classify role.
        let b = bytes[byte];
        let role = if b == b'$' || (byte > 0 && bytes[byte-1] == b'$') {
            "capture (binds $NAME at runtime)"
        } else if b == b'*' {
            "wildcard — `**` recurses, `*` matches any single key"
        } else if matches!(b, b'{' | b'}' | b'[' | b']') {
            "structural — object / array delimiter"
        } else if b == b':' {
            "key/value separator"
        } else {
            return None;
        };
        Some(lsp_types::Hover {
            contents: lsp_types::HoverContents::Scalar(
                lsp_types::MarkedString::String(role.to_string())
            ),
            range: None,
        })
    }

    fn completions(&self, body: &[u8], byte: usize) -> Vec<lsp_types::CompletionItem> {
        let mut out = Vec::new();
        // Always-available structural primitives.
        for (lbl, doc) in &[
            ("**",  "recursive object/array descent"),
            ("*",   "match any single key"),
            ("...", "array element wildcard"),
            ("$",   "capture sigil — follow with NAME"),
        ] {
            out.push(lsp_types::CompletionItem {
                label: lbl.to_string(),
                detail: Some(doc.to_string()),
                kind: Some(lsp_types::CompletionItemKind::KEYWORD),
                ..Default::default()
            });
        }
        // Already-declared captures earlier in the body — surface for reuse.
        let prefix = &body[..byte.min(body.len())];
        for binder in scan_capture_names(prefix) {
            out.push(lsp_types::CompletionItem {
                label: format!("${}", binder),
                detail: Some("declared capture".into()),
                kind: Some(lsp_types::CompletionItemKind::VARIABLE),
                ..Default::default()
            });
        }
        out
    }
}

fn scan_capture_names(raw: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < raw.len() {
        if raw[i] == b'$' && i + 1 < raw.len()
            && (raw[i+1].is_ascii_alphabetic() || raw[i+1] == b'_')
        {
            let mut j = i + 1;
            while j < raw.len() && (raw[j].is_ascii_alphanumeric() || raw[j] == b'_') { j += 1; }
            if let Ok(s) = std::str::from_utf8(&raw[i+1..j]) {
                if !out.iter().any(|x| x == s) { out.push(s.to_string()); }
            }
            i = j;
        } else {
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cst::diag::SilentSink;
    use crate::cst::dsl::VecCaptureSink;

    #[test]
    fn lsp_semantic_tokens_classify_capture_string_number_braces() {
        let dsl = JsonDsl::new();
        let toks = dsl.semantic_tokens(b"{ id: $X, name: \"alice\" }");
        let kinds: Vec<&str> = toks.iter().map(|t| {
            // tag tokens by their byte_range start char so the test reads
            match toks.iter().position(|x| x as *const _ == t as *const _).map(|i| (i, t)) {
                Some((_, tt)) if tt.byte_range.is_empty() => "?",
                Some((_, _)) => "tok",
                None => "?",
            }
        }).collect();
        assert!(!toks.is_empty(), "expected tokens, got {:?}", kinds);
        // capture sigil $X → exactly one VARIABLE-ish token
        let has_capture = toks.iter().any(|t| {
            std::str::from_utf8(&b"{ id: $X, name: \"alice\" }"[t.byte_range.clone()])
                .map(|s| s.starts_with('$'))
                .unwrap_or(false)
        });
        assert!(has_capture, "expected $X to be tokenized");
        // string literal "alice" → at least one string-class token
        let has_string = toks.iter().any(|t| {
            std::str::from_utf8(&b"{ id: $X, name: \"alice\" }"[t.byte_range.clone()])
                .map(|s| s.starts_with('"'))
                .unwrap_or(false)
        });
        assert!(has_string, "expected \"alice\" to be tokenized");
    }

    #[test]
    fn lsp_completions_include_structural_and_declared_captures() {
        let dsl = JsonDsl::new();
        // cursor at end of body — earlier captures `$N` should be suggested.
        let body = b"{ name: $N, id: ";
        let items = dsl.completions(body, body.len());
        let labels: Vec<&str> = items.iter().map(|c| c.label.as_str()).collect();
        assert!(labels.contains(&"**"));
        assert!(labels.contains(&"*"));
        assert!(labels.contains(&"..."));
        assert!(labels.contains(&"$"));
        assert!(labels.iter().any(|l| *l == "$N"),
                "expected $N in completions, got {:?}", labels);
    }

    #[test]
    fn lsp_hover_classifies_token_role() {
        let dsl = JsonDsl::new();
        let body = b"{ id: $X }";
        // byte 6 = '$'
        let h = dsl.hover(body, 6).expect("hover at $");
        match h.contents {
            lsp_types::HoverContents::Scalar(lsp_types::MarkedString::String(s)) => {
                assert!(s.contains("capture"), "expected capture role, got {s:?}");
            }
            _ => panic!("unexpected hover shape"),
        }
        // byte 0 = '{' → structural
        let h = dsl.hover(body, 0).expect("hover at {");
        match h.contents {
            lsp_types::HoverContents::Scalar(lsp_types::MarkedString::String(s)) => {
                assert!(s.contains("structural"));
            }
            _ => panic!("unexpected hover shape"),
        }
    }

    #[test]
    fn flat_pattern_emits_caps_at_match_time() {
        let dsl = JsonDsl::new();
        let c = dsl.compile(b"{ name: $N, version: $V }", &SilentSink).unwrap();
        let mut sink = VecCaptureSink::new();
        c.match_into(br#"{"name":"alice","version":"1.2"}"#, 0, &mut sink);
        let names: Vec<&str> = sink.rows.iter().map(|r| &*r.name).collect();
        assert!(names.contains(&"N"));
        assert!(names.contains(&"V"));
    }

    #[test]
    fn rejects_empty_body() {
        let err = match JsonDsl::new().compile(b"", &SilentSink) {
            Err(d) => d,
            Ok(_)  => panic!("expected empty-body error"),
        };
        assert_eq!(err.code, "json.empty-body");
    }

    #[test]
    fn rejects_bad_brace_syntax() {
        let err = match JsonDsl::new().compile(b"{ name ", &SilentSink) {
            Err(d) => d,
            Ok(_)  => panic!("expected parse-body error"),
        };
        assert_eq!(err.code, "json.parse-body");
    }

    #[test]
    fn matches_flat_object() {
        let c = JsonDsl::new()
            .compile(b"{ name: $N, version: $V }", &SilentSink)
            .unwrap();
        let mut sink = VecCaptureSink::new();
        c.match_into(br#"{"name":"alice","version":"1.2"}"#, 0, &mut sink);
        let names: Vec<&str> = sink.rows.iter().map(|r| &*r.name).collect();
        assert!(names.contains(&"N"));
        assert!(names.contains(&"V"));
    }

    #[test]
    fn captured_keys_fan_out() {
        let c = JsonDsl::new()
            .compile(b"{ deps: { $K: $V } }", &SilentSink)
            .unwrap();
        let mut sink = VecCaptureSink::new();
        c.match_into(br#"{"deps":{"foo":"1","bar":"2"}}"#, 0, &mut sink);
        let k_count = sink.rows.iter().filter(|r| &*r.name == "K").count();
        let v_count = sink.rows.iter().filter(|r| &*r.name == "V").count();
        assert_eq!(k_count, 2);
        assert_eq!(v_count, 2);
    }

    #[test]
    fn double_star_recurses() {
        let c = JsonDsl::new()
            .compile(b"{ **: { image: $I } }", &SilentSink)
            .unwrap();
        let mut sink = VecCaptureSink::new();
        c.match_into(br#"{"a":{"b":{"image":"nginx"}}}"#, 0, &mut sink);
        assert!(sink.rows.iter().any(|r| &*r.name == "I"));
    }

    #[test]
    fn no_match_emits_zero_rows() {
        let c = JsonDsl::new()
            .compile(b"{ paths: { /no-such: { get: { id: ${X?} } } } }", &SilentSink)
            .unwrap();
        let mut sink = VecCaptureSink::new();
        c.match_into(br#"{"paths":{"/users":{"get":{"id":"x"}}}}"#, 0, &mut sink);
        assert_eq!(sink.rows.len(), 0);
    }
}
