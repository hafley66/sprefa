//! Json operator - extracts from JSON/YAML/TOML.
//!
//! DESIGN: Transform operator. Reads file, extracts captures from pattern.
//! Handles cross-refs via Store.query_refs().
//! Reports EXACT byte ranges for path errors.
//!
//! Example: json({ package: { name: $NAME } })
//! Example with cross-ref: json({ uses: ${dep.NAME} })

use std::collections::HashMap;
use serde_json::Value;
use crate::{Context, CrossRef, Cursor, Diagnostic, Fix, Location, OpOutput, Operator, Severity, Store, CaptureValue, CaptureSource};
use crate::parse_utils::{did_you_mean, parse_cross_ref};

/// JSON extraction operator.
#[derive(Debug)]
pub struct JsonOp {
    /// Pattern components with byte offsets for precise errors
    components: Vec<Component>,
    cross_refs: Vec<CrossRef>,
    span: Location,
}

#[derive(Debug, Clone)]
struct Component {
    key: String,
    is_capture: bool,
    byte_offset: usize,
}

impl Operator for JsonOp {
    type Effect = ();
    
    fn parse(body: &str, span: Location) -> Result<Self, Vec<Diagnostic>> {
        // Parse simple pattern like { package: { name: $NAME } }
        // For v2: simplified, just extract keys and captures
        let mut components = vec![];
        let mut cross_refs = vec![];
        
        // Very simple tokenization for prototype
        let mut chars = body.char_indices().peekable();
        while let Some((offset, ch)) = chars.next() {
            match ch {
                '$' => {
                    // Capture variable
                    let start = offset;
                    let mut name = String::new();
                    while let Some((_, c)) = chars.peek() {
                        if c.is_ascii_alphanumeric() || *c == '_' {
                            name.push(*c);
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    components.push(Component {
                        key: name.clone(),
                        is_capture: true,
                        byte_offset: start,
                    });
                }
                '{' | '}' | ':' | ' ' | '"' | '\'' => continue,
                c if c.is_ascii_alphabetic() => {
                    // Key
                    let start = offset;
                    let mut key = c.to_string();
                    while let Some((_, c2)) = chars.peek() {
                        if c2.is_ascii_alphanumeric() || *c2 == '_' {
                            key.push(*c2);
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    components.push(Component {
                        key,
                        is_capture: false,
                        byte_offset: start,
                    });
                }
                _ => {}
            }
        }
        
        // Check for cross-refs like ${dep.NAME}
        if let Some(cref) = parse_cross_ref(body, &span) {
            cross_refs.push(cref);
        }
        
        Ok(JsonOp { components, cross_refs, span })
    }
    
    fn execute(&self, store: &dyn Store, input: Vec<Cursor>, _ctx: &Context) -> OpOutput<()> {
        let mut out = OpOutput::default();
        
        for cursor in input {
            if let Some(ref file) = cursor.file {
                let Some(content) = store.read_file(file) else {
                    out.diagnostics.push(Diagnostic {
                        message: format!("Cannot read: {}", file.display()),
                        rule_location: self.span.clone(),
                        target_location: None,
                        severity: Severity::Warning,
                        fix: None,
                    });
                    continue;
                };
                
                // Parse JSON
                let json: Value = match serde_json::from_str(&content) {
                    Ok(v) => v,
                    Err(e) => {
                        out.diagnostics.push(Diagnostic {
                            message: format!("JSON parse error: {}", e),
                            rule_location: self.span.clone(),
                            target_location: None,
                            severity: Severity::Error,
                            fix: None,
                        });
                        continue;
                    }
                };
                
                // Walk pattern and extract
                let (mut captures, errs) = self.extract(&json, file);
                
                // Add path errors with EXACT byte ranges
                for err in errs {
                    let loc = Location {
                        file: self.span.file.clone(),
                        byte_range: self.span.byte_range.start + err.component_offset
                            ..self.span.byte_range.start + err.component_offset + err.key.len(),
                        line: self.span.line,
                    };
                    
                    let available_refs: Vec<&str> = err.available.iter().map(|s| s.as_str()).collect();
                    let suggestion = did_you_mean(&err.key, &available_refs);
                    
                    out.diagnostics.push(Diagnostic {
                        message: format!("'{}' not found (have: {:?})", err.key, err.available),
                        rule_location: loc.clone(),
                        target_location: None,
                        severity: Severity::Error,
                        fix: suggestion.map(|s| Fix {
                            description: format!("Did you mean '{}' ?", s),
                            replacement: s.to_string(),
                            range: loc,
                        }),
                    });
                }
                
                // Resolve cross-refs from Store
                for cref in &self.cross_refs {
                    let values = store.query_refs(&cref.rule, &cref.var, &cursor.repo, &cursor.rev);
                    for val in values {
                        captures.insert(cref.var.clone(), CaptureValue {
                            value: val,
                            source: CaptureSource::Grounded { 
                                rule: cref.rule.clone(), 
                                var: cref.var.clone() 
                            },
                            scan: None,
                        });
                    }
                }
                
                out.cursors.push(Cursor {
                    repo: cursor.repo,
                    rev: cursor.rev,
                    file: cursor.file,
                    captures,
                });
            }
        }
        
        out
    }
}

impl JsonOp {
    fn extract(&self, json: &Value, _file: &std::path::PathBuf) 
        -> (HashMap<String, CaptureValue>, Vec<ExtractionError>) 
    {
        let mut captures = HashMap::new();
        let mut errors = vec![];
        
        let mut current = json;
        
        for comp in &self.components {
            if comp.is_capture {
                // Extract value at current position
                if let Some(s) = current.as_str() {
                    captures.insert(comp.key.clone(), CaptureValue {
                        value: s.to_string(),
                        source: CaptureSource::Extracted,
                        scan: None,
                    });
                } else {
                    captures.insert(comp.key.clone(), CaptureValue {
                        value: current.to_string(),
                        source: CaptureSource::Extracted,
                        scan: None,
                    });
                }
            } else {
                // Navigate to key
                match current {
                    Value::Object(map) => {
                        if let Some(next) = map.get(&comp.key) {
                            current = next;
                        } else {
                            let available: Vec<String> = map.keys().cloned().collect();
                            errors.push(ExtractionError {
                                key: comp.key.clone(),
                                component_offset: comp.byte_offset,
                                available,
                            });
                            return (captures, errors);
                        }
                    }
                    _ => {
                        errors.push(ExtractionError {
                            key: comp.key.clone(),
                            component_offset: comp.byte_offset,
                            available: vec![],
                        });
                        return (captures, errors);
                    }
                }
            }
        }
        
        (captures, errors)
    }
}

#[derive(Debug)]
struct ExtractionError {
    key: String,
    component_offset: usize,
    available: Vec<String>,
}
