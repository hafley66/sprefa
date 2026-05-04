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
}

impl Dsl for JsonDsl {
    fn id(&self) -> &'static str { "json" }

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cst::diag::SilentSink;
    use crate::cst::dsl::VecCaptureSink;

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
