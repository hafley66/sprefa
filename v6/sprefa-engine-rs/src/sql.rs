// The SQL seam. v6 law: async at the SQL seam only; in-memory row work above
// the seam stays plain sync. Each method is `async` outward and runs blocking
// rusqlite inward; none awaits inside a single statement, so a statement's
// effects are visible to the next statement in the same ordered batch.

use std::collections::HashMap;

use regex::Regex;
use rusqlite::functions::FunctionFlags;
use rusqlite::{params_from_iter, Connection, OptionalExtension, Row};
use std::sync::Arc;

use crate::types::{BoundaryError, BoundaryResult, QueryResult, ScalarValue, SqlStatement, Value};

pub type Error = rusqlite::Error;
pub type Result<T> = std::result::Result<T, Error>;

// SqlRunner is the trait analogue of ISqlSeam.runner. batch runs statements in
// order, one at a time, so a later statement observes this tick's earlier ones.

pub trait SqlRunner {
    fn execute(&self, statement: &SqlStatement) -> Result<QueryResult>;
    fn batch(&self, statements: &[SqlStatement]) -> Result<Vec<QueryResult>>;
    fn execute_multiple(&self, sql: &str) -> Result<()>;
    fn scalar(&self, sql: &str) -> Result<i64>;
}

pub struct SqliteSeam {
    conn: Connection,
}

impl SqliteSeam {
    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        install_regexp(&conn)?;
        Ok(SqliteSeam { conn })
    }

    pub fn open(url: &str) -> Result<Self> {
        let conn = Connection::open(url)?;
        install_regexp(&conn)?;
        Ok(SqliteSeam { conn })
    }

    pub fn run_ddl(&self, ddl: &[String]) -> Result<()> {
        for statement in ddl {
            self.conn.execute_batch(statement)?;
        }
        Ok(())
    }
}

fn install_regexp(conn: &Connection) -> Result<()> {
    conn.create_scalar_function(
        "regexp",
        2,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            let pattern: Arc<Regex> = ctx.get_or_create_aux(
                0,
                |value| -> std::result::Result<_, Box<dyn std::error::Error + Send + Sync>> {
                    Ok(Regex::new(value.as_str()?)?)
                },
            )?;
            let Ok(text) = ctx.get_raw(1).as_str() else {
                return Ok(false);
            };
            Ok(pattern.is_match(text))
        },
    )
}

fn to_param(value: &ScalarValue) -> rusqlite::types::Value {
    match value {
        ScalarValue::Integer(v) => rusqlite::types::Value::Integer(*v),
        ScalarValue::Real(v) => rusqlite::types::Value::Real(*v),
        ScalarValue::Bool(b) => rusqlite::types::Value::Integer(if *b { 1 } else { 0 }),
        ScalarValue::Text(v) => rusqlite::types::Value::Text(v.clone()),
        ScalarValue::Bytes(v) => rusqlite::types::Value::Blob(v.clone()),
    }
}

fn row_to_values(row: &Row, columns: &[String]) -> Vec<Value> {
    let mut out = Vec::with_capacity(columns.len());
    for index in 0..columns.len() {
        let value = match row.get_ref(index) {
            Ok(rusqlite::types::ValueRef::Integer(v)) => Value::Integer(v),
            Ok(rusqlite::types::ValueRef::Real(v)) => Value::Real(v),
            Ok(rusqlite::types::ValueRef::Text(v)) => {
                Value::Text(String::from_utf8_lossy(v).into_owned())
            }
            Ok(rusqlite::types::ValueRef::Blob(v)) => Value::Bytes(v.to_vec()),
            Ok(rusqlite::types::ValueRef::Null) => Value::Text(String::new()),
            Err(_) => Value::Text(String::new()),
        };
        out.push(value);
    }
    out
}

impl SqlRunner for SqliteSeam {
    fn execute(&self, statement: &SqlStatement) -> Result<QueryResult> {
        let mut stmt = self.conn.prepare(&statement.sql)?;
        let column_count = stmt.column_count();
        let columns: Vec<String> = (0..column_count)
            .map(|index| stmt.column_name(index).unwrap_or_default().to_string())
            .collect();
        // Projection statements receive the full trigger row even when their
        // generated SQL binds only a prefix of that row.
        let params: Vec<rusqlite::types::Value> = statement
            .args
            .iter()
            .take(stmt.parameter_count())
            .map(to_param)
            .collect();
        let mut rows = stmt.query(params_from_iter(params.iter()))?;
        let mut out_rows = Vec::new();
        while let Some(row) = rows.next()? {
            out_rows.push(row_to_values(row, &columns));
        }
        let rows_affected = self.conn.changes() as i64;
        Ok(QueryResult {
            rows: out_rows,
            columns,
            rows_affected,
        })
    }

    fn batch(&self, statements: &[SqlStatement]) -> Result<Vec<QueryResult>> {
        statements
            .iter()
            .map(|statement| self.execute(statement))
            .collect()
    }

    fn execute_multiple(&self, sql: &str) -> Result<()> {
        self.conn.execute_batch(sql)?;
        Ok(())
    }

    fn scalar(&self, sql: &str) -> Result<i64> {
        let value = self.conn.query_row(sql, [], |row| row.get(0)).optional()?;
        Ok(value.unwrap_or(0))
    }
}

// result_rows maps raw seam rows through declared column types, mirroring
// result_rows in 1_incremental.ts. bool columns normalize 0/1 to Bool; float
// columns normalize -0 to 0 and validate finite; int columns stay integer.

pub fn result_rows(
    result: &QueryResult,
    columns: &[String],
    column_types: &[crate::types::RowColumnType],
) -> BoundaryResult<Vec<Vec<Value>>> {
    result
        .rows
        .iter()
        .map(|row| {
            columns
                .iter()
                .enumerate()
                .map(|(index, _column)| {
                    let value = row
                        .get(index)
                        .cloned()
                        .unwrap_or(Value::Text(String::new()));
                    let ty = column_types.get(index).copied();
                    normalize_boundary_value(value, ty)
                })
                .collect()
        })
        .collect()
}

fn normalize_boundary_value(
    value: Value,
    ty: Option<crate::types::RowColumnType>,
) -> BoundaryResult<Value> {
    match (ty, value) {
        (Some(crate::types::RowColumnType::Bool), Value::Integer(v)) => Ok(Value::Bool(v != 0)),
        (Some(crate::types::RowColumnType::Float), Value::Real(v)) => {
            if !v.is_finite() {
                panic!("float column crossed SQLite with non-finite value");
            }
            if v == 0.0 {
                Ok(Value::Real(0.0))
            } else {
                Ok(Value::Real(v))
            }
        }
        (Some(crate::types::RowColumnType::Float), Value::Integer(v)) => Ok(Value::Real(v as f64)),
        // F3: the consumer gets Vec<T>, never the array TEXT the `__list_`
        // view aggregated and never the interned entity id.
        (Some(crate::types::RowColumnType::List), Value::Text(text)) => {
            match serde_json::from_str::<Vec<serde_json::Value>>(&text) {
                Ok(items) => Ok(Value::List(items)),
                Err(error) => Err(BoundaryError::ListColumnNotAnArray {
                    text,
                    detail: error.to_string(),
                }),
            }
        }
        (Some(crate::types::RowColumnType::Bytes), Value::Bytes(bytes)) => Ok(Value::Bytes(bytes)),
        (Some(crate::types::RowColumnType::Bytes), _value) => Err(
            BoundaryError::BytesAtScalarSeam(crate::types::ScalarSeam::SqlParameter),
        ),
        (_, value) => Ok(value),
    }
}

pub struct RawCols {
    pub map: HashMap<String, usize>,
}

pub fn column_index(result: &QueryResult, name: &str) -> Option<usize> {
    result.columns.iter().position(|column| column == name)
}
