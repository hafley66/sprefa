//! `IRowStore` over one SQLite file. Table names are the program name, a dot,
//! then the object: `"fetch_json.sym"`, `"fetch_json.rel412_a2"`.

use super::store::{kernel_owned, CellKind, IRowStore, StoreError, Watermark, KERNEL_ARITY};
use crate::_6_eval::evaluate::Store;
use crate::_6_eval::{Sym, Term, TermId, Universe};
use ordered_float::OrderedFloat;
use rusqlite::types::Value;
use rusqlite::{params_from_iter, Connection};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

const KIND_INT: i64 = 0;
const KIND_FLOAT: i64 = 1;
const KIND_BOOL: i64 = 2;
const KIND_ATOM: i64 = 3;
const KIND_STR: i64 = 4;
const KIND_COMPOUND: i64 = 5;

pub struct SqliteRowStore {
    connection: Connection,
    program: String,
    /// Rows of each relation already written: this store's own cursor, never
    /// `Table.frontier`, which is the semi-naive wavefront.
    durable: HashMap<TermId, usize>,
    columns: HashMap<(u32, usize), Vec<CellKind>>,
    arena: Watermark,
    variable_limit: usize,
    in_tick: bool,
}

impl SqliteRowStore {
    pub fn at(path: &Path) -> Result<SqliteRowStore, StoreError> {
        let connection = Connection::open(path)?;
        let variable_limit =
            connection.limit(rusqlite::limits::Limit::SQLITE_LIMIT_VARIABLE_NUMBER)? as usize;
        Ok(SqliteRowStore {
            connection,
            program: String::new(),
            durable: HashMap::new(),
            columns: HashMap::new(),
            arena: Watermark::default(),
            variable_limit: variable_limit.max(8),
            in_tick: false,
        })
    }

    fn table(&self, object: &str) -> String {
        format!("{}.{object}", self.program)
    }

    fn product_table(&self, rel: TermId, arity: usize) -> String {
        self.table(&format!("rel{}_a{arity}", rel.0))
    }

    /// One INSERT per chunk of `variable_limit / per_row` rows, so the
    /// statement count is a function of the chunk size and never of the rows.
    fn append(
        &self,
        table: &str,
        columns: &[String],
        values: &[Value],
        per_row: usize,
    ) -> Result<usize, StoreError> {
        if values.is_empty() {
            return Ok(0);
        }
        let names = columns
            .iter()
            .map(|c| format!("\"{c}\""))
            .collect::<Vec<_>>()
            .join(",");
        let rows = values.len() / per_row;
        let batch = (self.variable_limit / per_row).max(1);
        let mut written = 0;
        let mut offset = 0;
        while offset < rows {
            let count = batch.min(rows - offset);
            let tuple = format!("({})", vec!["?"; per_row].join(","));
            let tuples = vec![tuple; count].join(",");
            let sql = format!("INSERT OR IGNORE INTO \"{table}\" ({names}) VALUES {tuples}");
            let slice = &values[offset * per_row..(offset + count) * per_row];
            written += self
                .connection
                .execute(&sql, params_from_iter(slice.iter()))?;
            tracing::info!(target: "dl8::store", statement = "insert", table, rows = count);
            offset += count;
        }
        Ok(written)
    }

    fn kernel_columns() -> Vec<String> {
        let mut names = vec!["rel".to_string(), "arity".to_string()];
        names.extend((0..KERNEL_ARITY).map(|position| format!("a{position}")));
        names
    }

    fn create_product(&self, table: &str, kinds: &[CellKind]) -> Result<(), StoreError> {
        let names = product_columns(kinds);
        let mut declarations = vec!["\"__id\" INTEGER PRIMARY KEY".to_string()];
        for (name, kind) in names.iter().zip(kinds) {
            declarations.push(format!("\"{name}\" {} NOT NULL", kind.sql_type()));
        }
        if !names.is_empty() {
            let unique = names
                .iter()
                .map(|n| format!("\"{n}\""))
                .collect::<Vec<_>>()
                .join(",");
            declarations.push(format!("UNIQUE ({unique})"));
        }
        self.connection.execute_batch(&format!(
            "CREATE TABLE IF NOT EXISTS \"{table}\" ({});",
            declarations.join(", ")
        ))?;
        Ok(())
    }

    fn register_product(
        &mut self,
        rel: TermId,
        arity: usize,
        kinds: &[CellKind],
    ) -> Result<String, StoreError> {
        let table = self.product_table(rel, arity);
        if let Some(stored) = self.columns.get(&(rel.0, arity)) {
            for (position, (stored, arriving)) in stored.iter().zip(kinds).enumerate() {
                if stored != arriving {
                    return Err(StoreError::ColumnKind {
                        table,
                        position,
                        stored: *stored,
                        arriving: *arriving,
                    });
                }
            }
            return Ok(table);
        }
        self.create_product(&table, kinds)?;
        self.connection.execute(
            &format!(
                "INSERT OR IGNORE INTO \"{}\" (\"rel\",\"arity\") VALUES (?,?)",
                self.table("relation")
            ),
            (rel.0 as i64, arity as i64),
        )?;
        self.columns.insert((rel.0, arity), kinds.to_vec());
        Ok(table)
    }

    fn append_product(
        &mut self,
        u: &Universe,
        rel: TermId,
        arity: usize,
        rows: &[&[TermId]],
    ) -> Result<usize, StoreError> {
        let kinds: Vec<CellKind> = rows[0].iter().map(|c| CellKind::of(u.get(*c))).collect();
        let table = self.register_product(rel, arity, &kinds)?;
        if arity == 0 {
            self.connection.execute(
                &format!("INSERT OR IGNORE INTO \"{table}\" (\"__id\") VALUES (1)"),
                (),
            )?;
            tracing::info!(target: "dl8::store", statement = "insert", table, rows = 1);
            return Ok(1);
        }
        let mut values = Vec::with_capacity(rows.len() * arity);
        for row in rows {
            for (position, cell) in row.iter().enumerate() {
                let kind = CellKind::of(u.get(*cell));
                if kind != kinds[position] {
                    return Err(StoreError::ColumnKind {
                        table,
                        position,
                        stored: kinds[position],
                        arriving: kind,
                    });
                }
                values.push(encode(u, *cell));
            }
        }
        self.append(&table, &product_columns(&kinds), &values, arity)
    }

    fn append_kernel(
        &self,
        rel: TermId,
        arity: usize,
        rows: &[&[TermId]],
    ) -> Result<usize, StoreError> {
        if arity > KERNEL_ARITY {
            return Err(StoreError::ArityOverflow { arity });
        }
        let width = KERNEL_ARITY + 2;
        let mut values = Vec::with_capacity(rows.len() * width);
        for row in rows {
            values.push(Value::Integer(rel.0 as i64));
            values.push(Value::Integer(arity as i64));
            for position in 0..KERNEL_ARITY {
                let cell = row.get(position).map(|c| c.0 as i64).unwrap_or(0);
                values.push(Value::Integer(cell));
            }
        }
        self.append(
            &self.table("kernel"),
            &Self::kernel_columns(),
            &values,
            width,
        )
    }

    fn load_syms(&self, u: &mut Universe) -> Result<usize, StoreError> {
        let mut syms = Vec::new();
        {
            let mut statement = self.connection.prepare(&format!(
                "SELECT \"id\",\"text\" FROM \"{}\" ORDER BY \"id\"",
                self.table("sym")
            ))?;
            let mut cursor = statement.query(())?;
            while let Some(row) = cursor.next()? {
                syms.push((row.get::<_, i64>(0)? as usize, row.get::<_, String>(1)?));
            }
        }
        let durable = syms.len();
        for (id, text) in syms {
            let got = u.sym(&text);
            if got.0 as usize != id {
                return Err(StoreError::ArenaMismatch {
                    at: id,
                    stored: text,
                    expected: format!("sym {}", got.0),
                });
            }
        }
        Ok(durable)
    }

    fn term_args(&self) -> Result<HashMap<i64, Vec<TermId>>, StoreError> {
        let mut args: HashMap<i64, Vec<TermId>> = HashMap::new();
        let mut statement = self.connection.prepare(&format!(
            "SELECT \"term\",\"child\" FROM \"{}\" ORDER BY \"term\",\"position\"",
            self.table("term_arg")
        ))?;
        let mut cursor = statement.query(())?;
        while let Some(row) = cursor.next()? {
            args.entry(row.get::<_, i64>(0)?)
                .or_default()
                .push(TermId(row.get::<_, i64>(1)? as u32));
        }
        Ok(args)
    }

    /// `ORDER BY id` needs no second pass: `CHECK (child < term)` makes every
    /// argument already interned when its compound arrives.
    fn load_terms(&self, u: &mut Universe) -> Result<usize, StoreError> {
        let mut args = self.term_args()?;
        let mut terms = Vec::new();
        {
            let mut statement = self.connection.prepare(&format!(
                "SELECT \"id\",\"kind\",\"ival\",\"rval\",\"sym\" FROM \"{}\" ORDER BY \"id\"",
                self.table("term")
            ))?;
            let mut cursor = statement.query(())?;
            while let Some(row) = cursor.next()? {
                terms.push((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, f64>(3)?,
                    row.get::<_, i64>(4)? as u32,
                ));
            }
        }
        let durable = terms.len();
        for (id, kind, ival, rval, sym) in terms {
            let term = match kind {
                KIND_INT => Term::Int(ival),
                KIND_FLOAT => Term::Float(OrderedFloat(rval)),
                KIND_BOOL => Term::Bool(ival != 0),
                KIND_ATOM => Term::Atom(Sym(sym)),
                KIND_STR => Term::Str(Sym(sym)),
                _ => Term::Compound(Sym(sym), args.remove(&id).unwrap_or_default()),
            };
            let got = u.intern(term);
            if got.0 as i64 != id {
                return Err(StoreError::ArenaMismatch {
                    at: id as usize,
                    stored: u.display(TermId(id as u32)).to_string(),
                    expected: u.display(got).to_string(),
                });
            }
        }
        Ok(durable)
    }

    fn load_kernel(&self, store: &mut Store) -> Result<usize, StoreError> {
        let names = Self::kernel_columns()
            .iter()
            .map(|n| format!("\"{n}\""))
            .collect::<Vec<_>>()
            .join(",");
        let mut statement = self.connection.prepare(&format!(
            "SELECT {names} FROM \"{}\" ORDER BY \"__id\"",
            self.table("kernel")
        ))?;
        let mut cursor = statement.query(())?;
        let mut loaded = 0;
        while let Some(row) = cursor.next()? {
            let rel = TermId(row.get::<_, i64>(0)? as u32);
            let arity = row.get::<_, i64>(1)? as usize;
            let mut cells = Vec::with_capacity(arity);
            for position in 0..arity {
                cells.push(TermId(row.get::<_, i64>(2 + position)? as u32));
            }
            store.insert(rel, cells.into_boxed_slice());
            loaded += 1;
        }
        Ok(loaded)
    }

    fn product_kinds(&self, table: &str) -> Result<Vec<CellKind>, StoreError> {
        let mut statement = self
            .connection
            .prepare("SELECT \"name\" FROM pragma_table_info(?) ORDER BY \"cid\"")?;
        let mut cursor = statement.query((table,))?;
        let mut kinds = Vec::new();
        while let Some(row) = cursor.next()? {
            let name: String = row.get(0)?;
            if let Some(kind) = name
                .split_once('_')
                .and_then(|(_, s)| CellKind::of_suffix(s))
            {
                kinds.push(kind);
            }
        }
        Ok(kinds)
    }

    fn load_product(
        &self,
        u: &mut Universe,
        store: &mut Store,
        rel: TermId,
        arity: usize,
        kinds: &[CellKind],
    ) -> Result<usize, StoreError> {
        let table = self.product_table(rel, arity);
        if arity == 0 {
            let rows: i64 = self.connection.query_row(
                &format!("SELECT count(*) FROM \"{table}\""),
                (),
                |row| row.get(0),
            )?;
            if rows > 0 {
                store.insert(rel, Vec::new().into_boxed_slice());
            }
            return Ok(rows as usize);
        }
        let names = product_columns(kinds)
            .iter()
            .map(|n| format!("\"{n}\""))
            .collect::<Vec<_>>()
            .join(",");
        let mut statement = self.connection.prepare(&format!(
            "SELECT {names} FROM \"{table}\" ORDER BY \"__id\""
        ))?;
        let mut cursor = statement.query(())?;
        let mut loaded = 0;
        while let Some(row) = cursor.next()? {
            let mut cells = Vec::with_capacity(arity);
            for (position, kind) in kinds.iter().enumerate() {
                cells.push(decode(u, *kind, row, position)?);
            }
            store.insert(rel, cells.into_boxed_slice());
            loaded += 1;
        }
        Ok(loaded)
    }
}

fn product_columns(kinds: &[CellKind]) -> Vec<String> {
    kinds
        .iter()
        .enumerate()
        .map(|(position, kind)| format!("c{position}_{}", kind.suffix()))
        .collect()
}

fn encode(u: &Universe, cell: TermId) -> Value {
    match u.get(cell) {
        Term::Int(n) => Value::Integer(*n),
        Term::Float(x) => Value::Real(x.0),
        Term::Bool(b) => Value::Integer(*b as i64),
        _ => Value::Integer(cell.0 as i64),
    }
}

fn decode(
    u: &mut Universe,
    kind: CellKind,
    row: &rusqlite::Row<'_>,
    position: usize,
) -> Result<TermId, StoreError> {
    Ok(match kind {
        CellKind::Int => u.int(row.get::<_, i64>(position)?),
        CellKind::Float => u.float(row.get::<_, f64>(position)?),
        CellKind::Bool => u.boolean(row.get::<_, i64>(position)? != 0),
        CellKind::Term => TermId(row.get::<_, i64>(position)? as u32),
    })
}

fn term_row(term: &Term) -> (i64, i64, f64, i64) {
    match term {
        Term::Int(n) => (KIND_INT, *n, 0.0, 0),
        Term::Float(x) => (KIND_FLOAT, 0, x.0, 0),
        Term::Bool(b) => (KIND_BOOL, *b as i64, 0.0, 0),
        Term::Atom(s) => (KIND_ATOM, 0, 0.0, s.0 as i64),
        Term::Str(s) => (KIND_STR, 0, 0.0, s.0 as i64),
        Term::Compound(s, _) => (KIND_COMPOUND, 0, 0.0, s.0 as i64),
    }
}

impl IRowStore for SqliteRowStore {
    fn open(&mut self, program: &str) -> Result<(), StoreError> {
        self.program = program.to_string();
        let kernel = Self::kernel_columns()
            .iter()
            .map(|n| format!("\"{n}\" INTEGER NOT NULL"))
            .collect::<Vec<_>>()
            .join(", ");
        let unique = Self::kernel_columns()
            .iter()
            .map(|n| format!("\"{n}\""))
            .collect::<Vec<_>>()
            .join(",");
        self.connection.execute_batch(&format!(
            "CREATE TABLE IF NOT EXISTS \"{sym}\" (
               \"id\" INTEGER PRIMARY KEY,
               \"text\" TEXT NOT NULL UNIQUE);
             CREATE TABLE IF NOT EXISTS \"{term}\" (
               \"id\" INTEGER PRIMARY KEY,
               \"kind\" INTEGER NOT NULL,
               \"ival\" INTEGER NOT NULL,
               \"rval\" REAL NOT NULL,
               \"sym\" INTEGER NOT NULL REFERENCES \"{sym}\"(\"id\"));
             CREATE TABLE IF NOT EXISTS \"{arg}\" (
               \"term\" INTEGER NOT NULL REFERENCES \"{term}\"(\"id\"),
               \"position\" INTEGER NOT NULL,
               \"child\" INTEGER NOT NULL REFERENCES \"{term}\"(\"id\"),
               PRIMARY KEY (\"term\", \"position\"),
               CHECK (\"child\" < \"term\")) WITHOUT ROWID;
             CREATE TABLE IF NOT EXISTS \"{relation}\" (
               \"__id\" INTEGER PRIMARY KEY,
               \"rel\" INTEGER NOT NULL REFERENCES \"{term}\"(\"id\"),
               \"arity\" INTEGER NOT NULL,
               UNIQUE (\"rel\", \"arity\"));
             CREATE TABLE IF NOT EXISTS \"{kernel_table}\" (
               \"__id\" INTEGER PRIMARY KEY, {kernel},
               UNIQUE ({unique}));",
            sym = self.table("sym"),
            term = self.table("term"),
            arg = self.table("term_arg"),
            relation = self.table("relation"),
            kernel_table = self.table("kernel"),
        ))?;
        Ok(())
    }

    fn load_arena(&mut self, u: &mut Universe) -> Result<Watermark, StoreError> {
        self.arena = Watermark {
            syms: self.load_syms(u)?,
            terms: self.load_terms(u)?,
        };
        Ok(self.arena)
    }

    fn load_rows(&mut self, u: &mut Universe, store: &mut Store) -> Result<usize, StoreError> {
        let mut loaded = self.load_kernel(store)?;
        let mut products = Vec::new();
        {
            let mut statement = self.connection.prepare(&format!(
                "SELECT \"rel\",\"arity\" FROM \"{}\" ORDER BY \"__id\"",
                self.table("relation")
            ))?;
            let mut cursor = statement.query(())?;
            while let Some(row) = cursor.next()? {
                products.push((
                    TermId(row.get::<_, i64>(0)? as u32),
                    row.get::<_, i64>(1)? as usize,
                ));
            }
        }
        for (rel, arity) in products {
            let kinds = self.product_kinds(&self.product_table(rel, arity))?;
            loaded += self.load_product(u, store, rel, arity, &kinds)?;
            self.columns.insert((rel.0, arity), kinds);
        }
        for (rel, table) in &store.tables {
            self.durable.insert(*rel, table.len());
        }
        Ok(loaded)
    }

    fn watermark(&self) -> Watermark {
        self.arena
    }

    fn begin_tick(&mut self) -> Result<(), StoreError> {
        if self.in_tick {
            return Err(StoreError::NestedBegin);
        }
        self.connection.execute_batch("BEGIN IMMEDIATE")?;
        self.in_tick = true;
        Ok(())
    }

    fn commit_arena(&mut self, u: &Universe, from: Watermark) -> Result<Watermark, StoreError> {
        let to = Watermark {
            syms: u.syms.len(),
            terms: u.terms.len(),
        };
        if to == from {
            return Ok(to);
        }
        let mut syms = Vec::with_capacity((to.syms - from.syms) * 2);
        for index in from.syms..to.syms {
            syms.push(Value::Integer(index as i64));
            syms.push(Value::Text(u.syms[index].clone()));
        }
        self.append(
            &self.table("sym"),
            &["id".to_string(), "text".to_string()],
            &syms,
            2,
        )?;
        let mut terms = Vec::with_capacity((to.terms - from.terms) * 5);
        let mut args = Vec::new();
        for index in from.terms..to.terms {
            let term = &u.terms[index];
            let (kind, ival, rval, sym) = term_row(term);
            terms.extend([
                Value::Integer(index as i64),
                Value::Integer(kind),
                Value::Integer(ival),
                Value::Real(rval),
                Value::Integer(sym),
            ]);
            if let Term::Compound(_, children) = term {
                for (position, child) in children.iter().enumerate() {
                    args.extend([
                        Value::Integer(index as i64),
                        Value::Integer(position as i64),
                        Value::Integer(child.0 as i64),
                    ]);
                }
            }
        }
        let term_names = ["id", "kind", "ival", "rval", "sym"].map(String::from);
        self.append(&self.table("term"), &term_names, &terms, 5)?;
        let arg_names = ["term", "position", "child"].map(String::from);
        self.append(&self.table("term_arg"), &arg_names, &args, 3)?;
        self.arena = to;
        Ok(to)
    }

    fn commit_rows(&mut self, u: &Universe, store: &Store) -> Result<usize, StoreError> {
        let mut relations: Vec<TermId> = store.tables.keys().copied().collect();
        relations.sort_by_key(|rel| rel.0);
        let mut written = 0;
        for rel in relations {
            let table = &store.tables[&rel];
            let from = self.durable.get(&rel).copied().unwrap_or(0);
            if from >= table.len() {
                continue;
            }
            let mut by_arity: BTreeMap<usize, Vec<&[TermId]>> = BTreeMap::new();
            for index in from..table.len() {
                let row = table.row(index as u32);
                by_arity.entry(row.len()).or_default().push(row);
            }
            for (arity, rows) in by_arity {
                written += if kernel_owned(u, rel) {
                    self.append_kernel(rel, arity, &rows)?
                } else {
                    self.append_product(u, rel, arity, &rows)?
                };
            }
            self.durable.insert(rel, table.len());
        }
        Ok(written)
    }

    fn commit_tick(&mut self) -> Result<(), StoreError> {
        self.connection.execute_batch("COMMIT")?;
        self.in_tick = false;
        Ok(())
    }

    fn rollback_tick(&mut self) -> Result<(), StoreError> {
        self.connection.execute_batch("ROLLBACK")?;
        self.in_tick = false;
        Ok(())
    }
}
