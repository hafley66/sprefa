// Two statements per non-empty batch, flat in the DISTINCT arriving value
// count. Runs before apply_arrivals: content must not reach an id column.

use std::collections::HashMap;

// Column is byte-based: bytes since the preceding newline, not Unicode scalars.
#[derive(Debug, Clone, PartialEq, Eq)]
struct LinePosition {
    line: usize,
    column: usize,
}

// line_starts[i] is the byte offset where one-based line i begins; [0] is 0.
#[derive(Debug, Clone)]
pub struct LineOffsetIndex {
    line_starts: Vec<usize>,
}

impl LineOffsetIndex {
    pub fn build(text: &str) -> Self {
        let newline_count = text.bytes().filter(|byte| *byte == b'\n').count();
        let mut line_starts = Vec::with_capacity(newline_count + 1);
        line_starts.push(0);
        for (offset, byte) in text.bytes().enumerate() {
            if byte == b'\n' {
                line_starts.push(offset + 1);
            }
        }
        Self { line_starts }
    }

    fn position(&self, offset: usize) -> LinePosition {
        let line_index = self.line_starts.partition_point(|start| *start <= offset) - 1;
        let line_start = self.line_starts[line_index];
        LinePosition {
            line: line_index + 1,
            column: offset - line_start + 1,
        }
    }

    // Span is half-open [start, end): the end position is exclusive.
    pub fn project(&self, text: &str, start: usize, end: usize) -> TextProjection {
        assert!(start <= end, "span start {start} exceeds end {end}");
        assert!(
            end <= text.len(),
            "span end {end} exceeds text length {}",
            text.len()
        );
        let start_position = self.position(start);
        let end_position = self.position(end);
        TextProjection {
            start_line: start_position.line,
            start_column: start_position.column,
            end_line: end_position.line,
            end_column: end_position.column,
            text: text[start..end].to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextProjection {
    pub start_line: usize,
    pub start_column: usize,
    pub end_line: usize,
    pub end_column: usize,
    pub text: String,
}

use crate::sql::{SqlRunner, SqliteSeam};
use crate::types::{
    Arrival, BoundaryResult, Row, ScalarSeam, ScalarValue, SqlStatement, TextInternPlan, Value,
};

// A number reaching a text column interns as its rendering, the text the
// column carried before the storage flip.
fn content_of(value: &Value) -> BoundaryResult<String> {
    Ok(match ScalarValue::at_seam(value, ScalarSeam::TextIntern)? {
        ScalarValue::Text(text) => text,
        ScalarValue::Integer(number) => format!("{}", number),
        ScalarValue::Real(number) => crate::ticklog::js_float_text(number),
        ScalarValue::Bool(flag) => (if flag { "true" } else { "false" }).to_string(),
        ScalarValue::Bytes(_) => unreachable!("bytes cannot reach text intern"),
    })
}

fn collect_values(plan: &TextInternPlan, arrivals: &[Arrival]) -> BoundaryResult<Vec<String>> {
    let mut values: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for arrival in arrivals {
        let Some(flags) = plan.rel_columns.get(&arrival.rel) else {
            continue;
        };
        for (index, value) in arrival.row.iter().enumerate() {
            if flags.get(index) != Some(&true) {
                continue;
            }
            let content = content_of(value)?;
            if seen.insert(content.clone()) {
                values.push(content);
            }
        }
    }
    Ok(values)
}

fn rewrite_row(row: &Row, flags: &[bool], ids: &HashMap<String, i64>) -> BoundaryResult<Row> {
    row.iter()
        .enumerate()
        .map(|(index, value)| {
            if flags.get(index) != Some(&true) {
                return Ok(value.clone());
            }
            let content = content_of(value)?;
            let id = ids
                .get(&content)
                .unwrap_or_else(|| panic!("text intern lost the id for {:?}", content));
            Ok(Value::Integer(*id))
        })
        .collect()
}

pub fn intern(
    seam: &SqliteSeam,
    plan: &TextInternPlan,
    arrivals: &[Arrival],
) -> BoundaryResult<Vec<Arrival>> {
    if arrivals.is_empty() {
        return Ok(arrivals.to_vec());
    }
    let values = collect_values(plan, arrivals)?;
    if values.is_empty() {
        return Ok(arrivals.to_vec());
    }
    let encoded = ScalarValue::Text(crate::incremental::json_array_text(
        &values
            .iter()
            .map(|content| Value::Text(content.clone()))
            .collect::<Vec<_>>(),
    )?);
    seam.execute(&SqlStatement {
        sql: plan.intern_sql.clone(),
        args: vec![encoded.clone()],
    })
    .expect("text intern write failed");
    let result = seam
        .execute(&SqlStatement {
            sql: plan.lookup_sql.clone(),
            args: vec![encoded],
        })
        .expect("text intern lookup failed");
    let lookup_index = crate::sql::column_index(&result, "__lookup");
    let id_index = crate::sql::column_index(&result, "__id");
    let mut ids: HashMap<String, i64> = HashMap::new();
    if let (Some(lookup_index), Some(id_index)) = (lookup_index, id_index) {
        for row in &result.rows {
            let (Some(content), Some(id)) = (row.get(lookup_index), row.get(id_index)) else {
                continue;
            };
            ids.insert(content_of(content)?, id.as_i64().unwrap_or_default());
        }
    }
    arrivals
        .iter()
        .map(|arrival| match plan.rel_columns.get(&arrival.rel) {
            None => Ok(arrival.clone()),
            Some(flags) => Ok(Arrival {
                rel: arrival.rel.clone(),
                sign: arrival.sign,
                row: rewrite_row(&arrival.row, flags, &ids)?,
            }),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{LineOffsetIndex, TextProjection};

    #[test]
    fn text_empty_file_projects_line_one_column_one() {
        let index = LineOffsetIndex::build("");
        assert_eq!(
            index.project("", 0, 0),
            TextProjection {
                start_line: 1,
                start_column: 1,
                end_line: 1,
                end_column: 1,
                text: String::new(),
            }
        );
    }

    #[test]
    fn text_span_at_byte_zero_projects_first_line() {
        let source = "hello world";
        let index = LineOffsetIndex::build(source);
        assert_eq!(
            index.project(source, 0, 5),
            TextProjection {
                start_line: 1,
                start_column: 1,
                end_line: 1,
                end_column: 6,
                text: "hello".to_string(),
            }
        );
    }

    #[test]
    fn text_span_crossing_newline_reports_end_on_next_line() {
        let source = "ab\ncd";
        let index = LineOffsetIndex::build(source);
        assert_eq!(
            index.project(source, 1, 4),
            TextProjection {
                start_line: 1,
                start_column: 2,
                end_line: 2,
                end_column: 2,
                text: "b\nc".to_string(),
            }
        );
    }

    #[test]
    fn text_span_at_end_of_file_projects_eof_position() {
        let source = "abc";
        let index = LineOffsetIndex::build(source);
        assert_eq!(
            index.project(source, 0, 3),
            TextProjection {
                start_line: 1,
                start_column: 1,
                end_line: 1,
                end_column: 4,
                text: "abc".to_string(),
            }
        );
    }

    #[test]
    fn text_multibyte_utf8_before_span_widens_byte_column() {
        let source = "éabc";
        let index = LineOffsetIndex::build(source);
        let projection = index.project(source, 2, 5);
        assert_eq!(projection.start_column, 3);
        assert_eq!(projection.text, "abc");
    }

    #[test]
    fn text_crlf_file_reports_line_after_carriage_return_newline() {
        let source = "ab\r\ncd";
        let index = LineOffsetIndex::build(source);
        assert_eq!(
            index.project(source, 0, 6),
            TextProjection {
                start_line: 1,
                start_column: 1,
                end_line: 2,
                end_column: 3,
                text: "ab\r\ncd".to_string(),
            }
        );
    }

    #[test]
    fn text_one_index_reuses_across_many_lookups() {
        let source = "one\ntwo\nthree";
        let index = LineOffsetIndex::build(source);
        let first = index.project(source, 0, 3);
        let second = index.project(source, 4, 7);
        let third = index.project(source, 8, 13);
        assert_eq!((first.start_line, first.end_line), (1, 1));
        assert_eq!((second.start_line, second.end_line), (2, 2));
        assert_eq!((third.start_line, third.end_line), (3, 3));
    }
}
