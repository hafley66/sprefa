use std::collections::HashMap;
use crate::{Capture, Context, Cursor, Diagnostic, Location, OpResult, Operator, Severity, Store};
use crate::_2_operator::ParseError;
use crate::_4_parse_utils::levenshtein;
use serde_json::Value;

#[derive(Debug)]
pub struct JsonOp {
    keys: Vec<(String, bool, usize)>, // (key, is_capture, byte_offset)
}

impl Operator for JsonOp {
    type Effect = ();
    
    fn parse(body: &str) -> Result<Self, ParseError> {
        let mut keys = vec![];
        let mut chars = body.char_indices().peekable();
        
        while let Some((offset, ch)) = chars.next() {
            match ch {
                '$' => {
                    // Capture variable
                    let mut name = String::new();
                    while let Some((_, c)) = chars.peek() {
                        if c.is_ascii_alphanumeric() || *c == '_' {
                            name.push(*c);
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    if !name.is_empty() {
                        keys.push((name, true, offset));
                    }
                }
                '{' | '}' | ':' | ' ' | '"' | '\'' => continue,
                c if c.is_ascii_alphabetic() => {
                    let mut key = c.to_string();
                    while let Some((_, c2)) = chars.peek() {
                        if c2.is_ascii_alphanumeric() || *c2 == '_' {
                            key.push(*c2);
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    keys.push((key, false, offset));
                }
                _ => {}
            }
        }
        
        if keys.is_empty() {
            return Err(ParseError { message: "Empty json pattern".into(), offset: 0 });
        }
        
        Ok(JsonOp { keys })
    }
    
    fn execute(&self, store: &dyn Store, input: Vec<Cursor>, _ctx: &Context) -> OpResult<()> {
        let mut out = OpResult::default();
        
        for cursor in input {
            let Some(ref file) = cursor.file else {
                continue;
            };
            
            let Some(content) = store.read_file(file) else {
                out.diagnostics.push(Diagnostic {
                    message: format!("Cannot read: {}", file.display()),
                    rule_location: Location {
                        file: file.clone(),
                        byte_range: 0..0,
                        line: 0,
                    },
                    severity: Severity::Warning,
                });
                continue;
            };
            
            let json: Value = match serde_json::from_str(&content) {
                Ok(v) => v,
                Err(e) => {
                    out.diagnostics.push(Diagnostic {
                        message: format!("JSON parse error: {}", e),
                        rule_location: Location {
                            file: file.clone(),
                            byte_range: 0..0,
                            line: 0,
                        },
                        severity: Severity::Error,
                    });
                    continue;
                }
            };
            
            // Walk pattern
            let (captures, err) = self.walk(&json);
            
            if let Some((key, offset, available)) = err {
                let loc = Location {
                    file: std::path::PathBuf::from("test.spf"),
                    byte_range: offset..offset + key.len(),
                    line: 0,
                };
                
                // Find closest match
                let suggestion = if !available.is_empty() {
                    available.iter()
                        .min_by_key(|a| levenshtein(&key, a))
                        .map(|s| format!(" Did you mean '{}' ?", s))
                        .unwrap_or_default()
                } else {
                    String::new()
                };
                
                out.diagnostics.push(Diagnostic {
                    message: format!("'{}' not found (have: {:?}){}", key, available, suggestion),
                    rule_location: loc,
                    severity: Severity::Error,
                });
            }
            
            if !captures.is_empty() {
                let mut new_cursor = cursor.clone();
                new_cursor.captures.extend(captures);
                out.cursors.push(new_cursor);
            }
        }
        
        out
    }
}

impl JsonOp {
    fn walk(&self, json: &Value) -> (HashMap<String, Capture>, Option<(String, usize, Vec<String>)>) {
        let mut current = json;
        let mut captures = HashMap::new();
        
        for (key, is_capture, offset) in &self.keys {
            if *is_capture {
                // Extract value at current position
                let val = match current {
                    Value::String(s) => s.clone(),
                    v => v.to_string(),
                };
                captures.insert(key.clone(), Capture {
                    value: val,
                    scan: None,
                });
            } else {
                // Navigate to key
                match current {
                    Value::Object(map) => {
                        if let Some(next) = map.get(key) {
                            current = next;
                        } else {
                            let available: Vec<String> = map.keys().cloned().collect();
                            return (captures, Some((key.clone(), *offset, available)));
                        }
                    }
                    _ => {
                        return (captures, Some((key.clone(), *offset, vec![])));
                    }
                }
            }
        }
        
        (captures, None)
    }
}
