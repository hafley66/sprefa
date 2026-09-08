use crate::types::*;
use rusqlite::{Connection, Result};
use sqlite3_parser::{ast::*, lexer::sql::Parser, Bump, FallibleIterator};

fn table(db: &Connection, ast: &SelectTable<'_>) -> Result<Table> {
    let SelectTable::Table(q, alias, None) = ast else {
        return Err(reject("ordinary tables only"));
    };
    if q.db_name.as_ref().is_some_and(|n| n != &"main") || q.alias.is_some() {
        return Err(reject("main schema only"));
    }
    let requested = name(q.name.0);
    valid_name(&requested)?;
    let (actual, sql): (String, String) = db.query_row(
        "SELECT name,sql FROM main.sqlite_schema WHERE type='table' AND name=? COLLATE NOCASE",
        [&requested],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let shadow: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM temp.sqlite_schema WHERE name=? COLLATE NOCASE)",
        [&actual],
        |r| r.get(0),
    )?;
    if shadow {
        return Err(reject("TEMP source shadow unsupported"));
    }
    let external_trigger: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM main.sqlite_schema WHERE type='trigger' AND tbl_name=? COLLATE NOCASE AND substr(name,1,6)<>'__ivm_')", [&actual], |r|r.get(0))?;
    if external_trigger {
        return Err(reject("existing unmanaged source triggers unsupported"));
    }
    let bump = Bump::new();
    let mut parser = Parser::new(&bump, sql.as_bytes());
    let Some(Cmd::Stmt(Stmt::CreateTable {
        body: CreateTableBody::ColumnsAndConstraints { columns, .. },
        ..
    })) = parser.next().map_err(|_| reject("catalog parse failed"))?
    else {
        return Err(reject("virtual/derived tables unsupported"));
    };
    for col in &columns {
        for c in col.constraints {
            match &c.constraint {
                ColumnConstraint::Collate { collation_name } if collation_name != &"BINARY" => {
                    return Err(reject("non-BINARY collation unsupported"))
                }
                ColumnConstraint::ForeignKey { .. } => {
                    return Err(reject("source foreign keys unsupported"))
                }
                ColumnConstraint::Generated { .. } => {
                    return Err(reject("generated columns unsupported"))
                }
                _ => {}
            }
        }
    }
    let mut stmt = db.prepare("SELECT name,type,hidden FROM pragma_table_xinfo(?, 'main')")?;
    let cols = stmt
        .query_map([&actual], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>>>()?;
    if cols
        .iter()
        .any(|(_, t, h)| !t.eq_ignore_ascii_case("INTEGER") || *h != 0)
    {
        return Err(reject(
            "all source columns must declare INTEGER; hidden columns unsupported",
        ));
    }
    let alias = match alias {
        Some(As::As(n) | As::Elided(n)) => name(n.0),
        None => actual.clone(),
    };
    Ok(Table {
        name: actual,
        alias,
        columns: cols.into_iter().map(|(n, _, _)| n).collect(),
    })
}
fn col(e: &Expr<'_>, tables: &[Table; 2], using: Option<&str>) -> Result<Column> {
    let (qualifier, column) = match e {
        Expr::Qualified(q, n) => (Some(name(q.0)), name(n.0)),
        Expr::Name(n) => (None, name(n.0)),
        Expr::Id(n) => (None, name(n.0)),
        _ => return Err(reject("column reference required")),
    };
    let matches: Vec<_> = tables
        .iter()
        .enumerate()
        .flat_map(|(side, t)| {
            let qualifier = &qualifier;
            let column = &column;
            t.columns
                .iter()
                .filter(move |n| {
                    n.eq_ignore_ascii_case(column)
                        && qualifier
                            .as_ref()
                            .is_none_or(|q| q.eq_ignore_ascii_case(&t.alias))
                })
                .map(move |n| Column {
                    side,
                    name: n.clone(),
                })
        })
        .collect();
    if matches.len() == 1 {
        return Ok(matches[0].clone());
    }
    if matches.len() == 2
        && qualifier.is_none()
        && using.is_some_and(|u| u.eq_ignore_ascii_case(&column))
    {
        return Ok(matches[0].clone());
    }
    Err(reject("unknown or ambiguous column"))
}
pub fn bind(db: &Connection, sql: &str) -> Result<Plan> {
    if sql.len() > 16384 || sql.contains('\0') {
        return Err(reject("SELECT limited to 16384 bytes without NUL"));
    }
    let bump = Bump::new();
    let mut parser = Parser::new(&bump, sql.as_bytes());
    let cmd = parser.next().map_err(|_| reject("SELECT parse failed"))?;
    if parser
        .next()
        .map_err(|_| reject("trailing SQL rejected"))?
        .is_some()
    {
        return Err(reject("exactly one SELECT required"));
    }
    let Some(Cmd::Stmt(Stmt::Select(s))) = cmd else {
        return Err(reject("SELECT required"));
    };
    if s.with.is_some() || s.order_by.is_some() || s.limit.is_some() || s.body.compounds.is_some() {
        return Err(reject("CTE/order/limit/set operations unsupported"));
    }
    let OneSelect::Select {
        distinctness: None,
        columns,
        from: Some(from),
        where_clause: None,
        group_by: Some(groups),
        having: None,
        window_clause: None,
    } = &s.body.select
    else {
        return Err(reject(
            "grouped SELECT only; filter/distinct/global/window unsupported",
        ));
    };
    let joins = from
        .joins
        .as_ref()
        .ok_or_else(|| reject("one inner join required"))?;
    if joins.len() != 1 || groups.len() != 1 || columns.len() != 3 {
        return Err(reject(
            "one join, one group key, key/COUNT/SUM projection required",
        ));
    }
    let join = &joins[0];
    if !matches!(join.operator, JoinOperator::TypedJoin(None))
        && !matches!(join.operator, JoinOperator::TypedJoin(Some(t)) if t == JoinType::INNER)
    {
        return Err(reject("inner join only"));
    }
    let tables = [
        table(db, from.select.ok_or_else(|| reject("missing table"))?)?,
        table(db, &join.table)?,
    ];
    if tables[0].name.eq_ignore_ascii_case(&tables[1].name)
        || tables[0].alias.eq_ignore_ascii_case(&tables[1].alias)
    {
        return Err(reject("self join / duplicate alias unsupported"));
    }
    let using = match &join.constraint {
        Some(JoinConstraint::Using(ns)) if ns.len() == 1 => Some(name(ns.iter().next().unwrap().0)),
        _ => None,
    };
    let keys = match &join.constraint {
        Some(JoinConstraint::On(Expr::Binary(a, Operator::Equals, b))) => {
            let a = col(a, &tables, None)?;
            let b = col(b, &tables, None)?;
            if a.side == b.side {
                return Err(reject("join must compare opposite sources"));
            }
            if a.side == 0 {
                [a, b]
            } else {
                [b, a]
            }
        }
        Some(JoinConstraint::Using(_)) if using.is_some() => {
            let u = using.as_ref().unwrap();
            let mut keys = Vec::new();
            for (side, t) in tables.iter().enumerate() {
                let c = t
                    .columns
                    .iter()
                    .find(|c| c.eq_ignore_ascii_case(u))
                    .ok_or_else(|| reject("USING column absent"))?;
                keys.push(Column {
                    side,
                    name: c.clone(),
                });
            }
            [keys.remove(0), keys.remove(0)]
        }
        _ => return Err(reject("single equijoin ON or USING required")),
    };
    let group = col(&groups[0], &tables, using.as_deref())?;
    if !keys.contains(&group) {
        return Err(reject("group key must be join key"));
    }
    let mut outputs = Vec::new();
    let mut expressions = Vec::new();
    for result in *columns {
        let ResultColumn::Expr(e, alias) = result else {
            return Err(reject("explicit projections required"));
        };
        let output = match alias {
            Some(As::As(n) | As::Elided(n)) => name(n.0),
            None if outputs.is_empty() => col(e, &tables, using.as_deref())?.name,
            _ => return Err(reject("aggregate aliases required")),
        };
        valid_name(&output)?;
        if outputs
            .iter()
            .any(|n: &String| n.eq_ignore_ascii_case(&output))
        {
            return Err(reject("duplicate output name"));
        }
        outputs.push(output);
        expressions.push(e);
    }
    if col(expressions[0], &tables, using.as_deref())? != group {
        return Err(reject("projection must match group key"));
    }
    if !matches!(expressions[1],Expr::FunctionCallStar { name: n, filter_over: None } if name(n.0).eq_ignore_ascii_case("count"))
    {
        return Err(reject("COUNT(*) required"));
    }
    let Expr::FunctionCall {
        name: n,
        distinctness: None,
        args: Some(args),
        order_by: None,
        filter_over: None,
    } = expressions[2]
    else {
        return Err(reject("SUM(column or column*column) required"));
    };
    if !name(n.0).eq_ignore_ascii_case("sum") || args.len() != 1 {
        return Err(reject("SUM required"));
    }
    let values = match &args[0] {
        Expr::Binary(a, Operator::Multiply, b) => vec![
            col(a, &tables, using.as_deref())?,
            col(b, &tables, using.as_deref())?,
        ],
        e => vec![col(e, &tables, using.as_deref())?],
    };
    let plan = Plan {
        tables,
        keys,
        values,
        outputs: outputs.try_into().unwrap(),
    };
    // SQLite's own binder checks the original single statement; it is never used as generated DDL text.
    db.prepare(sql)?;
    Ok(plan)
}
