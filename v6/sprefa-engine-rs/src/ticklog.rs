// Byte-identical port of v6/tsv2/runtime/ticklog.ts, graded against the oracle
// jsonl written by v6/prolog/conformance/ticklog.pl. The envelope:
//
//   {"tick":N,"deltas":{"relName":{"add":[[..],...],"del":[[..],...]}}}
//
// Rel names ascending; only rels with a nonempty add or del; rows sorted by
// their JSON text; no spaces; no trailing newline (the caller adds it). Number
// rendering matches the oracle's JS-float text so the diff stays empty.

use crate::types::{bytes_to_base64, RelDelta, Row, RowColumnType, TickDeltas, Value};

pub fn tick_line(
    tick: usize,
    deltas: &TickDeltas,
    rel_column_types: &std::collections::HashMap<String, Vec<RowColumnType>>,
    rel_declared_types: &std::collections::HashMap<String, Vec<RowColumnType>>,
) -> String {
    let mut non_empty: Vec<&RelDelta> = deltas
        .rels
        .iter()
        .filter(|delta| !delta.add.is_empty() || !delta.del.is_empty())
        .collect();
    non_empty.sort_by(|left, right| left.rel.cmp(&right.rel));
    let mut rels_text = String::new();
    for (index, delta) in non_empty.iter().enumerate() {
        if index > 0 {
            rels_text.push(',');
        }
        rels_text.push_str(&encode_rel(
            delta,
            rel_types_for(rel_column_types, rel_declared_types, &delta.rel),
        ));
    }
    format!("{{\"tick\":{},\"deltas\":{{{}}}}}", tick, rels_text)
}

fn rel_types_for<'a>(
    column_types: &'a std::collections::HashMap<String, Vec<RowColumnType>>,
    declared: &'a std::collections::HashMap<String, Vec<RowColumnType>>,
    rel: &str,
) -> &'a [RowColumnType] {
    if let Some(types) = column_types.get(rel) {
        types
    } else if let Some(types) = declared.get(rel) {
        types
    } else {
        &[]
    }
}

fn encode_rel(delta: &RelDelta, types: &[RowColumnType]) -> String {
    let mut add: Vec<String> = delta.add.iter().map(|row| encode_row(row, types)).collect();
    let mut del: Vec<String> = delta.del.iter().map(|row| encode_row(row, types)).collect();
    add.sort();
    del.sort();
    format!(
        "{}:{{\"add\":[{}],\"del\":[{}]}}",
        json_string(&delta.rel),
        add.join(","),
        del.join(",")
    )
}

/// One buffer per row, not one String per column plus a join.
fn encode_row(row: &Row, types: &[RowColumnType]) -> String {
    let mut out = String::with_capacity(row.len() * 12 + 2);
    out.push('[');
    for (index, value) in row.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        push_value(&mut out, value, types.get(index).copied());
    }
    out.push(']');
    out
}

fn push_value(out: &mut String, value: &Value, ty: Option<RowColumnType>) {
    match value {
        Value::Integer(v) => {
            use std::fmt::Write;
            let _ = write!(out, "{}", v);
        }
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Text(text) if ty != Some(RowColumnType::Json) && ty != Some(RowColumnType::Ref) => {
            push_json_string(out, text)
        }
        other => out.push_str(&encode_value(other, ty)),
    }
}

fn encode_value(value: &Value, ty: Option<RowColumnType>) -> String {
    match value {
        Value::Bytes(bytes) => format!("{{\"$bytes\":{}}}", json_string(&bytes_to_base64(bytes))),
        Value::Integer(v) => format!("{}", v),
        Value::Real(v) => {
            if !v.is_finite() {
                panic!("non-finite float at tick boundary: {}", v);
            }
            js_float_text(*v)
        }
        Value::Bool(b) => (if *b { "true" } else { "false" }).to_string(),
        // Already parsed at the read seam, so the elements canonicalize
        // directly and never through a second text re-parse.
        Value::List(items) => {
            let mut parts = Vec::with_capacity(items.len());
            for item in items {
                parts.push(canonical_json_value(item));
            }
            format!("[{}]", parts.join(","))
        }
        Value::Text(text) => {
            if ty == Some(RowColumnType::Json) || ty == Some(RowColumnType::Ref) {
                match canonical_json_text(text) {
                    Some(canonical) => canonical,
                    None => json_string(text),
                }
            } else {
                json_string(text)
            }
        }
    }
}

pub fn canonical_json_text(value: &str) -> Option<String> {
    match serde_json::from_str::<serde_json::Value>(value) {
        Ok(parsed) => Some(canonical_json_value(&parsed)),
        Err(_) => None,
    }
}

fn canonical_json_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Bool(b) => (if *b { "true" } else { "false" }).to_string(),
        serde_json::Value::Number(n) => json_number(n),
        serde_json::Value::String(s) => json_string(s),
        serde_json::Value::Array(items) => {
            let mut parts = Vec::with_capacity(items.len());
            for item in items {
                parts.push(canonical_json_value(item));
            }
            format!("[{}]", parts.join(","))
        }
        serde_json::Value::Object(map) => {
            // serde_json's Object is sorted when preserve_order is off.
            let mut entries = Vec::new();
            for (key, item) in map {
                entries.push(format!(
                    "{}:{}",
                    json_string(key),
                    canonical_json_value(item)
                ));
            }
            format!("{{{}}}", entries.join(","))
        }
    }
}

fn json_number(number: &serde_json::Number) -> String {
    if let Some(integer) = number.as_i64() {
        return format!("{}", integer);
    }
    if let Some(unsigned) = number.as_u64() {
        return format!("{}", unsigned);
    }
    if let Some(real) = number.as_f64() {
        return js_float_text(real);
    }
    number.to_string()
}

// js_float_text ports v6/prolog/0_dot_expand/0_type_plane.pl:js_float_text/2 so the Rust arm
// renders f64s byte-identically to the oracle (and to JS JSON.stringify).
pub fn js_float_text(value: f64) -> String {
    if value == 0.0 {
        return "0".to_string();
    }
    let negative = value.is_sign_negative();
    // LowerExp, not Display: Display never reaches exponent form, so 1e-7 would
    // render 0.0000001 and js_float_digits would never see a position to place.
    let raw = format!("{:e}", value.abs());
    if let Some((mantissa, exponent)) = split_float_exponent(&raw) {
        let (digits, integer_digits) = mantissa_digits(mantissa);
        let decimal_position = integer_digits + exponent;
        let mut out = js_float_digits(&digits, decimal_position);
        if negative {
            out.insert(0, '-');
        }
        out
    } else {
        let unsigned = normalize_fixed_float_atom(&raw);
        if negative {
            format!("-{}", unsigned)
        } else {
            unsigned
        }
    }
}

fn split_float_exponent(raw: &str) -> Option<(&str, i64)> {
    let position = raw.find('e').or_else(|| raw.find('E'))?;
    let mantissa = &raw[..position];
    let exponent_text = &raw[position + 1..];
    let exponent: i64 = exponent_text.parse().ok()?;
    Some((mantissa, exponent))
}

fn mantissa_digits(mantissa: &str) -> (Vec<char>, i64) {
    let mut digits = Vec::new();
    let mut integer_digits = 0i64;
    let mut in_fraction = false;
    for ch in mantissa.chars() {
        if ch == '.' {
            in_fraction = true;
        } else if !in_fraction {
            integer_digits += 1;
            digits.push(ch);
        } else {
            digits.push(ch);
        }
    }
    (digits, integer_digits)
}

fn js_float_digits(digits: &[char], position: i64) -> String {
    if position > 0 && position <= 21 {
        let prefix_len = position as usize;
        if prefix_len >= digits.len() {
            let mut text: String = digits.iter().collect();
            let zeros = prefix_len - digits.len();
            for _ in 0..zeros {
                text.push('0');
            }
            text
        } else {
            let prefix: String = digits[..prefix_len].iter().collect();
            let mut suffix: Vec<char> = digits[prefix_len..].to_vec();
            strip_trailing_zeros(&mut suffix);
            if suffix.is_empty() {
                prefix
            } else {
                format!("{}.{}", prefix, suffix.iter().collect::<String>())
            }
        }
    } else if (-5..=0).contains(&position) {
        let zeros = (-position) as usize;
        let mut all = Vec::new();
        all.push('0');
        all.push('.');
        all.extend((0..zeros).map(|_| '0'));
        all.extend_from_slice(digits);
        strip_trailing_zeros(&mut all);
        all.iter().collect()
    } else {
        let first = digits[0];
        let mut rest: Vec<char> = digits[1..].to_vec();
        strip_trailing_zeros(&mut rest);
        let mantissa = if rest.is_empty() {
            format!("{}", first)
        } else {
            format!("{}.{}", first, rest.iter().collect::<String>())
        };
        let exponent = position - 1;
        if exponent >= 0 {
            format!("{}e+{}", mantissa, exponent)
        } else {
            format!("{}e{}", mantissa, exponent)
        }
    }
}

fn strip_trailing_zeros(digits: &mut Vec<char>) {
    while digits.last() == Some(&'0') {
        digits.pop();
    }
}

fn normalize_fixed_float_atom(raw: &str) -> String {
    if let Some(stripped) = raw.strip_suffix(".0") {
        stripped.to_string()
    } else {
        raw.to_string()
    }
}

// json_string replicates JSON.stringify's escaping, the byte contract for text
// values and object keys.
pub fn json_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    push_json_string(&mut out, value);
    out
}

fn push_json_string(out: &mut String, value: &str) {
    out.reserve(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}
