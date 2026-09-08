use serde_json::{json, Value};
use sqlite3_parser::{ast::*, lexer::sql::Parser, Bump, FallibleIterator};
use std::io::{self, Read};

fn name(raw: &str) -> String {
    let bytes = raw.as_bytes();
    if bytes.len() >= 2 {
        match (bytes[0], bytes[bytes.len() - 1]) {
            (b'"', b'"') => return raw[1..raw.len() - 1].replace("\"\"", "\""),
            (b'`', b'`') => return raw[1..raw.len() - 1].replace("``", "`"),
            (b'[', b']') => return raw[1..raw.len() - 1].to_owned(),
            _ => {}
        }
    }
    raw.to_owned()
}

fn source(table: &SelectTable<'_>) -> Result<(String, String), String> {
    let SelectTable::Table(q, alias, None) = table else {
        return Err("ordinary unhinted tables required".into());
    };
    if q.db_name
        .as_ref()
        .is_some_and(|n| !name(n.0).eq_ignore_ascii_case("main"))
        || q.alias.is_some()
    {
        return Err("main schema required".into());
    }
    let table = name(q.name.0);
    let alias = match alias {
        Some(As::As(n) | As::Elided(n)) => name(n.0),
        None => table.clone(),
    };
    Ok((table, alias))
}

fn compile(sql: &str) -> Result<Value, String> {
    if sql.len() > 4096 || sql.contains('\0') {
        return Err("SELECT limit is 4096 bytes without NUL".into());
    }
    let bump = Bump::new();
    let mut parser = Parser::new(&bump, sql.as_bytes());
    let cmd = parser.next().map_err(|e| e.to_string())?;
    if parser.next().map_err(|e| e.to_string())?.is_some() {
        return Err("exactly one SELECT required".into());
    }
    let Some(Cmd::Stmt(Stmt::Select(s))) = cmd else {
        return Err("SELECT required".into());
    };
    if s.with.is_some() || s.order_by.is_some() || s.limit.is_some() || s.body.compounds.is_some() {
        return Err("CTE/order/limit/compound unsupported".into());
    }
    let OneSelect::Select {
        distinctness: None,
        columns,
        from: Some(from),
        where_clause,
        group_by: None,
        having: None,
        window_clause: None,
    } = &s.body.select
    else {
        return Err("bag SELECT with FROM, without grouping/windows required".into());
    };
    if columns.len() != 2 {
        return Err("exactly two scalar projections required".into());
    }
    let mut projections = Vec::new();
    for column in columns.iter() {
        let ResultColumn::Expr(expr, _) = column else {
            return Err("star projection unsupported".into());
        };
        projections.push(expr.to_string());
    }
    let mut tables = vec![source(from.select.ok_or("missing source")?)?];
    let mut predicates = Vec::new();
    if let Some(joins) = &from.joins {
        for join in joins.iter() {
            match join.operator {
                JoinOperator::Comma | JoinOperator::TypedJoin(None) => {}
                JoinOperator::TypedJoin(Some(kind))
                    if (kind & !(JoinType::INNER | JoinType::CROSS)).is_empty() => {}
                _ => return Err("only inner/cross joins supported".into()),
            }
            tables.push(source(&join.table)?);
            match &join.constraint {
                Some(JoinConstraint::On(expr)) => predicates.push(format!("({expr})")),
                None => {}
                _ => return Err("USING unsupported; use qualified ON".into()),
            }
        }
    }
    if tables.len() > 3 {
        return Err("at most three table occurrences supported".into());
    }
    if let Some(expr) = where_clause {
        predicates.push(format!("({expr})"));
    }
    let mut sources: Vec<String> = Vec::new();
    let mut aliases: Vec<String> = Vec::new();
    let mut sides = Vec::new();
    for (table, alias) in tables {
        if aliases.iter().any(|a| a.eq_ignore_ascii_case(&alias)) {
            return Err("duplicate source alias".into());
        }
        aliases.push(alias);
        let side = match sources.iter().position(|s| s.eq_ignore_ascii_case(&table)) {
            Some(side) => side,
            None => {
                sources.push(table);
                sources.len() - 1
            }
        };
        sides.push(side);
    }
    let plan = json!({"version":1,"sides":sides,"aliases":aliases,"key":projections[0],"value":projections[1],"predicate":if predicates.is_empty(){"1".into()}else{predicates.join(" AND ")}});
    let encoded = plan.to_string();
    if encoded.len() > 4096 {
        return Err("compiled plan limit is 4096 bytes".into());
    }
    Ok(
        json!({"plan":plan,"sources":sources,"create_sql":format!("CREATE VIRTUAL TABLE compiled_result USING take2_lazy(plan,'{}');",encoded.replace('\'',"''"))}),
    )
}

fn main() {
    let mut sql = String::new();
    if let Err(e) = io::stdin().take(4097).read_to_string(&mut sql) {
        eprintln!("{e}");
        std::process::exit(2);
    }
    match compile(&sql) {
        Ok(plan) => println!("{plan}"),
        Err(error) => {
            eprintln!("unsupported SELECT: {error}");
            std::process::exit(2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeated_source_and_theta() {
        let out =
            compile("SELECT x.k+1, coalesce(y.v,0) FROM a x JOIN a y ON x.v<y.k WHERE x.k<>0")
                .unwrap();
        assert_eq!(out["plan"]["sides"], json!([0, 0]));
        assert_eq!(out["sources"], json!(["a"]));
        assert_eq!(out["plan"]["aliases"], json!(["x", "y"]));
    }
    #[test]
    fn unsupported_shapes() {
        for sql in [
            "DELETE FROM a",
            "SELECT k,v FROM a; SELECT k,v FROM a",
            "SELECT DISTINCT k,v FROM a",
            "SELECT k,v FROM a ORDER BY k",
            "SELECT * FROM a",
            "SELECT a.k,b.v FROM a LEFT JOIN b ON a.k=b.k",
            "SELECT k,v FROM a JOIN b USING(k)",
            "SELECT k,v FROM (SELECT k,v FROM a)",
            "SELECT k,v FROM a,b,c,d",
        ] {
            assert!(compile(sql).is_err(), "{sql}");
        }
    }
}
