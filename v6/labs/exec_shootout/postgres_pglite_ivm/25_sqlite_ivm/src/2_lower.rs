//! SQL state follows the compiler's accumulator pattern (lower.pl
//! avg_accumulator_update_sql): signed contribution sum/count, then publish.
//! This lab adds row OLD/NEW join binding. No compiler or kernel is changed.
use crate::types::*;
use rusqlite::{Connection, Result};

pub fn prefix(view: &str) -> String {
    format!(
        "__ivm_{}",
        view.bytes().map(|b| format!("{b:02x}")).collect::<String>()
    )
}
pub fn domain(table: &Table, prefix: &str) -> String {
    table
        .columns
        .iter()
        .map(|c| {
            let c = format!("{prefix}{}", ident(c));
            format!("typeof({c}) <> 'integer' OR {c} NOT BETWEEN -1000000 AND 1000000")
        })
        .collect::<Vec<_>>()
        .join(" OR ")
}
pub fn objects(view: &str) -> Vec<(String, String)> {
    let p = prefix(view);
    let mut list = vec![("VIEW".into(), view.into())];
    for side in 0..2 {
        for event in ["INSERT", "UPDATE", "DELETE"] {
            for when in ["BEFORE", "AFTER"] {
                list.push(("TRIGGER".into(), format!("{p}_{side}_{event}_{when}")));
            }
        }
    }
    for side in 0..2 {
        list.push(("INDEX".into(), format!("{p}_index_{side}")));
    }
    for suffix in ["acc", "delta", "meta"] {
        list.push(("TABLE".into(), format!("{p}_{suffix}")));
    }
    list
}
pub fn lower(plan: &Plan, view: &str) -> Vec<String> {
    let p = prefix(view);
    let acc = ident(&format!("{p}_acc"));
    let delta = ident(&format!("{p}_delta"));
    let meta = ident(&format!("{p}_meta"));
    let mut ddl = vec![
        format!("CREATE TABLE main.{acc}(k INTEGER PRIMARY KEY,n INTEGER NOT NULL,s INTEGER NOT NULL)"),
        format!("CREATE TABLE main.{delta}(k INTEGER PRIMARY KEY,n INTEGER NOT NULL,s INTEGER NOT NULL)"),
        format!("CREATE TABLE main.{meta}(id INTEGER PRIMARY KEY CHECK(id=1),version INTEGER NOT NULL,enabled INTEGER NOT NULL,operations INTEGER NOT NULL,contributions INTEGER NOT NULL,groups_touched INTEGER NOT NULL)"),
        format!("INSERT INTO {meta} VALUES(1,1,0,0,0,0)"),
        format!("INSERT INTO {acc} {}",plan.select()),
        format!("CREATE VIEW main.{} AS SELECT k AS {},n AS {},s AS {} FROM {acc}",ident(view),ident(&plan.outputs[0]),ident(&plan.outputs[1]),ident(&plan.outputs[2])),
    ];
    for side in 0..2 {
        ddl.push(format!(
            "CREATE INDEX main.{} ON {}({})",
            ident(&format!("{p}_index_{side}")),
            ident(&plan.tables[side].name),
            ident(&plan.keys[side].name)
        ));
        for event in ["INSERT", "UPDATE", "DELETE"] {
            let mut guard = vec!["SELECT CASE WHEN (SELECT recursive_triggers FROM pragma_recursive_triggers)=0 THEN RAISE(ABORT,'sqlite_ivm: recursive_triggers required') END".to_owned()];
            if event != "DELETE" {
                guard.push(format!("SELECT CASE WHEN {} THEN RAISE(ABORT,'sqlite_ivm: non-NULL INTEGER in [-1000000,1000000] required') END",domain(&plan.tables[side],"NEW.")));
            }
            ddl.push(format!(
                "CREATE TRIGGER main.{} BEFORE {event} ON {} BEGIN {}; END",
                ident(&format!("{p}_{side}_{event}_BEFORE")),
                ident(&plan.tables[side].name),
                guard.join(";")
            ));
            let images = match event {
                "INSERT" => vec![("NEW", 1)],
                "DELETE" => vec![("OLD", -1)],
                _ => vec![("OLD", -1), ("NEW", 1)],
            };
            let mut body = Vec::new();
            // An omitted INTEGER PRIMARY KEY is assigned after BEFORE INSERT
            // (NEW.rowid may be -1 there). Validate the actual stored image.
            if event != "DELETE" {
                body.push(format!("SELECT CASE WHEN {} THEN RAISE(ABORT,'sqlite_ivm: assigned row outside integer bound') END", domain(&plan.tables[side], "NEW.")));
            }
            for (image, sign) in images {
                let other = 1 - side;
                let key = plan.column(&plan.keys[side], Some((side, image)));
                let value = plan.value(Some((side, image)));
                let counterpart = plan.column(&plan.keys[other], None);
                body.push(format!("DELETE FROM {delta}"));
                body.push(format!("INSERT INTO {delta} SELECT {key},{sign} * count(*),{sign} * sum({value}) FROM {} b{other} WHERE {counterpart}={key} HAVING count(*)>0",ident(&plan.tables[other].name)));
                // No INSERT OR IGNORE: the outer writer conflict policy can override it.
                body.push(format!("UPDATE {acc} SET n=n+(SELECT n FROM {delta} WHERE k={acc}.k),s=s+(SELECT s FROM {delta} WHERE k={acc}.k) WHERE k IN (SELECT k FROM {delta})"));
                body.push(format!("INSERT INTO {acc} SELECT k,n,s FROM {delta} WHERE NOT EXISTS(SELECT 1 FROM {acc} WHERE k={delta}.k)"));
                body.push(format!("SELECT CASE WHEN EXISTS(SELECT 1 FROM {acc} WHERE k IN (SELECT k FROM {delta}) AND (typeof(n)<>'integer' OR n NOT BETWEEN 0 AND 1000000 OR typeof(s)<>'integer' OR s NOT BETWEEN -1000000000000000000 AND 1000000000000000000 OR (n=0 AND s<>0))) THEN RAISE(ABORT,'sqlite_ivm: aggregate bound/invariant exceeded') END"));
                body.push(format!(
                    "DELETE FROM {acc} WHERE k IN (SELECT k FROM {delta}) AND n=0"
                ));
                // Fixed-size, saturating counters. They share the writer transaction.
                body.push(format!("UPDATE {meta} SET contributions=min(1000000000000000,contributions+coalesce((SELECT abs(n) FROM {delta}),0)),groups_touched=min(1000000000000000,groups_touched+(SELECT count(*) FROM {delta})) WHERE enabled=1"));
                body.push(format!("DELETE FROM {delta}"));
            }
            body.push(format!(
                "UPDATE {meta} SET operations=min(1000000000000000,operations+1) WHERE enabled=1"
            ));
            ddl.push(format!(
                "CREATE TRIGGER main.{} AFTER {event} ON {} BEGIN {}; END",
                ident(&format!("{p}_{side}_{event}_AFTER")),
                ident(&plan.tables[side].name),
                body.join(";")
            ));
        }
    }
    ddl
}
pub fn check_initial(db: &Connection, plan: &Plan) -> Result<()> {
    for table in &plan.tables {
        let invalid: bool = db.query_row(
            &format!(
                "SELECT EXISTS(SELECT 1 FROM main.{} WHERE {})",
                ident(&table.name),
                domain(table, "")
            ),
            [],
            |r| r.get(0),
        )?;
        if invalid {
            return Err(reject("initial source outside non-NULL integer bound"));
        }
    }
    let invalid: bool = db.query_row(
        &format!(
            "SELECT EXISTS(SELECT 1 FROM ({}) AS initial WHERE initial.\"count(*)\" > 1000000)",
            plan.select()
        ),
        [],
        |r| r.get(0),
    )?;
    if invalid {
        return Err(reject("initial group count exceeds 1000000"));
    }
    Ok(())
}
