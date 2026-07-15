use anyhow::{bail, Context, Result};
use serde::Serialize;
use serde_json::Value;
use sprefa_v5::ast::{Item, Program, RelDecl};
use sprefa_v5::engine::{all_builtin_decls, Engine};
use std::collections::BTreeMap;

use super::OBSERVABLE_RELATIONS;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct SemanticSnapshot {
    pub(crate) relations: Vec<SemanticRelation>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct SemanticRelation {
    pub(crate) name: String,
    pub(crate) columns: Vec<SemanticColumn>,
    pub(crate) rows: Vec<Vec<TypedCell>>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct SemanticColumn {
    name: String,
    declared_type: String,
    brand: Option<String>,
    raw_text: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
pub(crate) struct TypedCell {
    sqlite_type: &'static str,
    value: String,
}

pub(crate) fn observable_schemas(program: &Program) -> Result<Vec<(String, Vec<SemanticColumn>)>> {
    let mut declarations: BTreeMap<String, RelDecl> = all_builtin_decls()
        .into_iter()
        .map(|decl| (decl.name.clone(), decl))
        .collect();
    for item in &program.items {
        if let Item::Rel(decl) = item {
            declarations.insert(decl.name.clone(), decl.clone());
        }
    }
    OBSERVABLE_RELATIONS
        .iter()
        .map(|name| {
            let declaration = declarations
                .get(*name)
                .with_context(|| format!("observable relation {name} has no declaration"))?;
            let columns = declaration
                .cols
                .iter()
                .map(|column| SemanticColumn {
                    name: column.name.clone(),
                    declared_type: column.ty.name().to_string(),
                    brand: column.brand.clone(),
                    raw_text: column.raw,
                })
                .collect();
            Ok(((*name).to_string(), columns))
        })
        .collect()
}

pub(crate) fn semantic_snapshot(
    engine: &Engine,
    schemas: &[(String, Vec<SemanticColumn>)],
) -> Result<SemanticSnapshot> {
    let mut relations = Vec::with_capacity(schemas.len());
    for (name, columns) in schemas {
        let sql = format!("SELECT * FROM rel_{name}_txt");
        let raw_rows = engine
            .query_sql(&sql, &[])
            .with_context(|| format!("read observable relation {name}"))?;
        let mut rows = Vec::with_capacity(raw_rows.len());
        for raw_row in raw_rows {
            if raw_row.len() != columns.len() {
                bail!(
                    "observable relation {name} returned {} columns, declared {}",
                    raw_row.len(),
                    columns.len()
                );
            }
            rows.push(raw_row.into_iter().map(typed_cell).collect::<Result<Vec<_>>>()?);
        }
        rows.sort();
        relations.push(SemanticRelation {
            name: name.clone(),
            columns: columns.clone(),
            rows,
        });
    }
    Ok(SemanticSnapshot { relations })
}

fn typed_cell(value: Value) -> Result<TypedCell> {
    Ok(match value {
        Value::Null => TypedCell {
            sqlite_type: "null",
            value: String::new(),
        },
        Value::Bool(value) => TypedCell {
            sqlite_type: "boolean",
            value: value.to_string(),
        },
        Value::String(value) => TypedCell {
            sqlite_type: "text",
            value,
        },
        Value::Number(number) if number.is_i64() || number.is_u64() => TypedCell {
            sqlite_type: "integer",
            value: number.to_string(),
        },
        Value::Number(number) => TypedCell {
            sqlite_type: "real",
            value: number.to_string(),
        },
        Value::Array(_) | Value::Object(_) => {
            bail!("query_sql returned a nested JSON value instead of a SQLite scalar")
        }
    })
}
