//! The `dl_*` scalar functions the sqlite lowering emits. Each body calls the
//! kernel row functions over the shared arena and returns NULL where the
//! kernel has no row, so the functions never reimplement kernel semantics.

use super::kernel::{
    any_lt_row, edge_ref_row, int_dot_add_row, intern_row, str_cons_row, str_op_row,
    tuple_row,
};
use super::str_ops::STR_OPS;
use super::term::{TermId, Universe};
use rusqlite::functions::FunctionFlags;
use rusqlite::types::Value as SqlValue;
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

type Solve = fn(&mut Universe, &[Option<TermId>]) -> Option<Vec<TermId>>;

fn flags() -> FunctionFlags {
    FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC
}

/// An SQL argument as an arena cell; anything but a non-negative integer is
/// no argument at all, and the function returns NULL.
fn cell(value: &SqlValue) -> Option<TermId> {
    match value {
        SqlValue::Integer(n) if *n >= 0 && *n <= i64::from(u32::MAX) => Some(TermId(*n as u32)),
        _ => None,
    }
}

fn scalar(
    connection: &Connection,
    name: &str,
    arguments: i32,
    body: impl Fn(&[SqlValue]) -> Option<SqlValue> + Send + 'static,
) -> rusqlite::Result<()> {
    connection.create_scalar_function(name, arguments, flags(), move |ctx| {
        let mut args = Vec::with_capacity(ctx.len());
        for at in 0..ctx.len() {
            args.push(SqlValue::try_from(ctx.get_raw(at)).unwrap_or(SqlValue::Null));
        }
        Ok(body(&args).unwrap_or(SqlValue::Null))
    })
}

/// One kernel call with a fixed argument shape; `out` picks which position of
/// the solved row the function returns.
fn row(
    args: &[SqlValue],
    out: usize,
    solve: Solve,
    arena: &Arc<Mutex<Universe>>,
) -> Option<SqlValue> {
    // A one-argument call deconstructs: the whole value sits in the kernel
    // row's last position. Two arguments construct into that position.
    let mut shaped: Vec<Option<TermId>> = vec![None; 3];
    match args {
        [whole] => shaped[2] = cell(whole),
        [first, second] => {
            shaped[0] = cell(first);
            shaped[1] = cell(second);
        }
        _ => return None,
    }
    let mut taken = arena.lock().ok()?;
    let solved = solve(&mut taken, &shaped)?;
    let term = solved.get(out)?;
    Some(SqlValue::Integer(term.0 as i64))
}

/// Every `dl_*` function, bound to one arena. Register on every connection
/// before any view reads them.
pub fn register(connection: &Connection, arena: Arc<Mutex<Universe>>) -> rusqlite::Result<()> {
    let deconstruct: [(&str, usize, Solve); 2] = [
        ("dl_str_first", 0, str_cons_row),
        ("dl_str_rest", 1, str_cons_row),
    ];
    for (name, out, solve) in deconstruct {
        let arena = arena.clone();
        scalar(connection, name, 1, move |args| {
            (args.len() == 1).then(|| row(args, out, solve, &arena))?
        })?;
    }
    let construct: [(&str, Solve); 4] = [
        ("dl_str_cat", str_cons_row),
        ("dl_edge_ref", edge_ref_row),
        ("dl_application", intern_row),
        ("dl_int_add", int_dot_add_row),
    ];
    for (name, solve) in construct {
        let arena = arena.clone();
        scalar(connection, name, 2, move |args| {
            (args.len() == 2).then(|| row(args, 2, solve, &arena))?
        })?;
    }
    for (owner, label, arity, op) in STR_OPS {
        let arena = arena.clone();
        let op = *op;
        let inputs = op.args().len();
        scalar(connection, &format!("dl_{owner}_{label}"), inputs as i32, move |args| {
            let mut shaped: Vec<Option<TermId>> = args.iter().map(cell).collect();
            if *arity > inputs {
                shaped.push(None);
            }
            let mut taken = arena.lock().ok()?;
            let solved = str_op_row(&mut taken, op, &shaped)?;
            if *arity > inputs {
                Some(SqlValue::Integer(solved.last()?.0 as i64))
            } else {
                Some(SqlValue::Integer(1))
            }
        })?;
    }
    // `dl_tuple(Element..)` builds the tuple term over the arena; `dl_tuple_at
    // (Whole, Position, Count)` reads one element, NULL on an arity mismatch.
    let building = arena.clone();
    scalar(connection, "dl_tuple", -1, move |args| {
        let mut shaped: Vec<Option<TermId>> = args.iter().map(cell).collect();
        shaped.push(None);
        let mut taken = building.lock().ok()?;
        let solved = tuple_row(&mut taken, &shaped)?;
        Some(SqlValue::Integer(solved.last()?.0 as i64))
    })?;
    let reading = arena.clone();
    scalar(connection, "dl_tuple_at", 3, move |args| {
        let [whole, SqlValue::Integer(position), SqlValue::Integer(count)] = args else {
            return None;
        };
        let mut shaped: Vec<Option<TermId>> = vec![None; usize::try_from(*count).ok()?];
        shaped.push(Some(cell(whole)?));
        let mut taken = reading.lock().ok()?;
        let solved = tuple_row(&mut taken, &shaped)?;
        let term = solved.get(usize::try_from(*position).ok()?)?;
        Some(SqlValue::Integer(term.0 as i64))
    })?;
    let decoding = arena.clone();
    scalar(connection, "dl_int", 1, move |args| {
        if args.len() != 1 {
            return None;
        }
        let term = cell(&args[0])?;
        let taken = decoding.lock().ok()?;
        let payload = taken.unary(term, "const")?;
        let n = taken.as_int(payload)?;
        Some(SqlValue::Integer(n))
    })?;
    let encoding = arena.clone();
    scalar(connection, "dl_int_term", 1, move |args| {
        let [SqlValue::Integer(n)] = args else {
            return None;
        };
        let mut taken = encoding.lock().ok()?;
        let payload = taken.int(*n);
        let term = taken.compound("const", vec![payload]);
        Some(SqlValue::Integer(term.0 as i64))
    })?;
    let ordering = arena.clone();
    scalar(connection, "dl_term_lt", 2, move |args| {
        if args.len() != 2 {
            return None;
        }
        let left = cell(&args[0])?;
        let right = cell(&args[1])?;
        let mut taken = ordering.lock().ok()?;
        let holds = any_lt_row(&mut taken, &[Some(left), Some(right)]).is_some();
        Some(SqlValue::Integer(i64::from(holds)))
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_function_registers() {
        let connection = Connection::open_in_memory().unwrap();
        let arena = Arc::new(Mutex::new(Universe::new()));
        register(&connection, arena).unwrap();
    }
}
