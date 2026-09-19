//! `IRowStore` over one SQLite file. Table names are the program name, a dot,
//! then the object: `"1_settled.sym"`, `"1_settled.Body_a2"`, `"x.rel412_a2"`.

use super::store::{
    kernel_owned, nameable, CellKind, IRowStore, StoreError, Watermark, KERNEL_ARITY,
};
use crate::_6_eval::evaluate::Store;
use crate::_6_eval::{Sym, Term, TermId, Universe};
use ordered_float::OrderedFloat;
use rusqlite::types::Value;
use rusqlite::{params_from_iter, Connection};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::time::Instant;

const KIND_INT: i64 = 0;
const KIND_FLOAT: i64 = 1;
const KIND_BOOL: i64 = 2;
const KIND_ATOM: i64 = 3;
const KIND_STR: i64 = 4;
const KIND_COMPOUND: i64 = 5;
/// Caps chunk rounds in `append` so a broken row count or a stalled offset
/// can never spin the store; far above any real seed set.
const APPEND_ROUND_BUDGET: usize = 1_000_000;

/// The one connection contract, `open()` only. `page_size` first: it sticks
/// only before a file's first table. `recursive_triggers` and
/// `trusted_schema` are ON because sqlite_ivm requires them.
const CONTRACT_PRAGMAS: &str = "\
PRAGMA page_size = 65536;
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA mmap_size = 1073741824;
PRAGMA cache_size = -262144;
PRAGMA temp_store = MEMORY;
PRAGMA recursive_triggers = ON;
PRAGMA trusted_schema = ON;";

/// Every connection in the crate: contract pragmas, then the sqlite_ivm
/// extension. `:memory:` is accepted; on it `journal_mode` reads back
/// `memory`, so pragma assertions run on a file db.
pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    let connection = Connection::open(path)?;
    sql(&connection, "contract_pragmas", |connection| {
        connection.execute_batch(CONTRACT_PRAGMAS).map(|()| ((), 0))
    })?;
    let Some(extension) = sqlite_ivm_extension() else {
        return Err(rusqlite::Error::InvalidPath(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("sqlite_ivm/target/release")
                .join(if cfg!(target_os = "macos") {
                    "libsqlite_ivm.dylib"
                } else {
                    "libsqlite_ivm.so"
                }),
        ));
    };
    sql(&connection, "load_extension", |connection| {
        // SAFETY: the library is this repo's own sqlite_ivm build, loaded once
        // into this connection.
        unsafe { connection.load_extension(&extension, None::<&str>) }.map(|()| ((), 0))
    })?;
    Ok(connection)
}

/// `SQLITE_IVM_LIB`, else `libsqlite_ivm` in the release dir of
/// `CARGO_TARGET_DIR` or the sibling checkout symlink `sqlite_ivm/target`.
fn sqlite_ivm_extension() -> Option<PathBuf> {
    let file = if cfg!(target_os = "macos") {
        "libsqlite_ivm.dylib"
    } else {
        "libsqlite_ivm.so"
    };
    let mut candidates = Vec::new();
    if let Ok(path) = std::env::var("SQLITE_IVM_LIB") {
        candidates.push(PathBuf::from(path));
    }
    if let Ok(dir) = std::env::var("CARGO_TARGET_DIR") {
        candidates.push(Path::new(&dir).join("release").join(file));
    }
    candidates.push(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("sqlite_ivm/target/release")
            .join(file),
    );
    candidates.into_iter().find(|path| path.exists())
}

/// Every statement the crate runs: one `sql` span carrying the statement's
/// name, the rows it read or wrote, and its wall time in ms. A multi-row
/// INSERT of a seed set is one call with its row count.
pub fn sql<T>(
    connection: &Connection,
    name: &'static str,
    rows: impl FnOnce(&Connection) -> rusqlite::Result<(T, usize)>,
) -> rusqlite::Result<T> {
    let span = tracing::info_span!(
        "sql",
        name,
        rows = tracing::field::Empty,
        ms = tracing::field::Empty
    );
    let started = Instant::now();
    let outcome = {
        let _entered = span.enter();
        rows(connection)
    };
    let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
    let (count, error) = match &outcome {
        Ok((_, count)) => (*count as i64, None),
        Err(error) => (0i64, Some(error.to_string())),
    };
    span.record("rows", count);
    span.record("ms", elapsed_ms);
    // The stderr layer prints events only (`FmtSpan::NONE`); the span above
    // reaches OTLP in batches. This line is what a reader sees the moment the
    // statement returns, error included, without waiting for a flush.
    tracing::info!(
        target: "dl8::sql",
        name,
        rows = count,
        ms = format_args!("{elapsed_ms:.1}"),
        error = error.as_deref().unwrap_or("")
    );
    outcome.map(|(value, _)| value)
}

pub struct SqliteRowStore {
    connection: Connection,
    program: String,
    /// Rows of each relation already written: this store's own cursor, never
    /// `Table.frontier`, which is the semi-naive wavefront.
    durable: HashMap<TermId, usize>,
    columns: HashMap<(u32, usize), Vec<CellKind>>,
    /// Declared name per relation; a db name outranks the running program's.
    names: HashMap<TermId, String>,
    arena: Watermark,
    variable_limit: usize,
    in_tick: bool,
    insert_statements: std::cell::Cell<usize>,
}

impl SqliteRowStore {
    pub fn at(path: &Path) -> Result<SqliteRowStore, StoreError> {
        let connection = open(path)?;
        let variable_limit =
            connection.limit(rusqlite::limits::Limit::SQLITE_LIMIT_VARIABLE_NUMBER)? as usize;
        Ok(SqliteRowStore {
            connection,
            program: String::new(),
            durable: HashMap::new(),
            columns: HashMap::new(),
            names: HashMap::new(),
            arena: Watermark::default(),
            variable_limit: variable_limit.max(8),
            in_tick: false,
            insert_statements: std::cell::Cell::new(0),
        })
    }

    /// INSERT statements this connection has run, for COUNT tests.
    pub fn insert_statements(&self) -> usize {
        self.insert_statements.get()
    }

    fn table(&self, object: &str) -> String {
        format!("{}.{object}", self.program)
    }

    /// The arity suffix keeps a named table out of the reserved object names
    /// (`sym`, `term`, `term_arg`, `relation`, `kernel`), which carry none.
    fn table_for(&self, rel: TermId, arity: usize) -> String {
        match self.names.get(&rel) {
            Some(name) => self.table(&format!("{name}_a{arity}")),
            None => self.table(&format!("rel{}_a{arity}", rel.0)),
        }
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
        let mut rounds = 0usize;
        while offset < rows {
            rounds += 1;
            if rounds > APPEND_ROUND_BUDGET {
                tracing::error!(
                    target: "dl8::store",
                    diagnostic = "append_round_budget_exceeded",
                    table, rows, rounds,
                );
                break;
            }
            let count = batch.min(rows - offset);
            let tuple = format!("({})", vec!["?"; per_row].join(","));
            let tuples = vec![tuple; count].join(",");
            let insert_sql = format!("INSERT OR IGNORE INTO \"{table}\" ({names}) VALUES {tuples}");
            let slice = &values[offset * per_row..(offset + count) * per_row];
            written += sql(&self.connection, "insert", |connection| {
                connection
                    .execute(&insert_sql, params_from_iter(slice.iter()))
                    .map(|inserted| (inserted, inserted))
            })?;
            self.insert_statements.set(self.insert_statements.get() + 1);
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
        sql(&self.connection, "create_product", |connection| {
            connection
                .execute_batch(&format!(
                    "CREATE TABLE IF NOT EXISTS \"{table}\" ({});",
                    declarations.join(", ")
                ))
                .map(|()| ((), 0))
        })?;
        Ok(())
    }

    fn register_product(
        &mut self,
        rel: TermId,
        arity: usize,
        kinds: &[CellKind],
    ) -> Result<String, StoreError> {
        let table = self.table_for(rel, arity);
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
        self.insert_statements.set(self.insert_statements.get() + 1);
        sql(&self.connection, "declare_relation", |connection| {
            connection
                .execute(
                    &format!(
                        "INSERT OR IGNORE INTO \"{}\" (\"rel\",\"arity\",\"name\") VALUES (?,?,?)",
                        self.table("relation")
                    ),
                    (
                        rel.0 as i64,
                        arity as i64,
                        self.names.get(&rel).map(String::as_str),
                    ),
                )
                .map(|inserted| (inserted, inserted))
        })?;
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
            sql(&self.connection, "insert", |connection| {
                connection
                    .execute(
                        &format!("INSERT OR IGNORE INTO \"{table}\" (\"__id\") VALUES (1)"),
                        (),
                    )
                    .map(|inserted| ((), inserted))
            })?;
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
        sql(&self.connection, "load_syms", |connection| {
            let mut statement = connection.prepare(&format!(
                "SELECT \"id\",\"text\" FROM \"{}\" ORDER BY \"id\"",
                self.table("sym")
            ))?;
            let mut cursor = statement.query(())?;
            while let Some(row) = cursor.next()? {
                syms.push((row.get::<_, i64>(0)? as usize, row.get::<_, String>(1)?));
            }
            Ok(((), syms.len()))
        })?;
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
        sql(&self.connection, "load_term_args", |connection| {
            let mut statement = connection.prepare(&format!(
                "SELECT \"term\",\"child\" FROM \"{}\" ORDER BY \"term\",\"position\"",
                self.table("term_arg")
            ))?;
            let mut cursor = statement.query(())?;
            let mut count = 0;
            while let Some(row) = cursor.next()? {
                count += 1;
                args.entry(row.get::<_, i64>(0)?)
                    .or_default()
                    .push(TermId(row.get::<_, i64>(1)? as u32));
            }
            Ok(((), count))
        })?;
        Ok(args)
    }

    /// `ORDER BY id` needs no second pass: `CHECK (child < term)` makes every
    /// argument already interned when its compound arrives.
    fn load_terms(&self, u: &mut Universe) -> Result<usize, StoreError> {
        let mut args = self.term_args()?;
        let mut terms = Vec::new();
        sql(&self.connection, "load_terms", |connection| {
            let mut statement = connection.prepare(&format!(
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
            Ok(((), terms.len()))
        })?;
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
        sql(&self.connection, "load_kernel", |connection| {
            let mut statement = connection.prepare(&format!(
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
            Ok((loaded, loaded))
        })
        .map_err(StoreError::from)
    }

    fn product_kinds(&self, table: &str) -> Result<Vec<CellKind>, StoreError> {
        sql(&self.connection, "product_kinds", |connection| {
            let mut statement =
                connection.prepare("SELECT \"name\" FROM pragma_table_info(?) ORDER BY \"cid\"")?;
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
            let count = kinds.len();
            Ok((kinds, count))
        })
        .map_err(StoreError::from)
    }

    fn load_product(
        &self,
        u: &mut Universe,
        store: &mut Store,
        rel: TermId,
        arity: usize,
        kinds: &[CellKind],
    ) -> Result<usize, StoreError> {
        let table = self.table_for(rel, arity);
        if arity == 0 {
            let rows = sql(&self.connection, "count_product", |connection| {
                let rows: i64 = connection.query_row(
                    &format!("SELECT count(*) FROM \"{table}\""),
                    (),
                    |row| row.get(0),
                )?;
                Ok((rows, 1))
            })
            .map_err(StoreError::from)?;
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
        sql(&self.connection, "load_product", |connection| {
            let mut statement = connection.prepare(&format!(
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
            Ok((loaded, loaded))
        })
        .map_err(StoreError::from)
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
) -> rusqlite::Result<TermId> {
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
        sql(&self.connection, "declare_tables", |connection| {
            connection
                .execute_batch(&format!(
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
                       \"name\" TEXT,
                       UNIQUE (\"rel\", \"arity\"),
                       UNIQUE (\"name\", \"arity\"));
                     CREATE TABLE IF NOT EXISTS \"{kernel_table}\" (
                       \"__id\" INTEGER PRIMARY KEY, {kernel},
                       UNIQUE ({unique}));",
                    sym = self.table("sym"),
                    term = self.table("term"),
                    arg = self.table("term_arg"),
                    relation = self.table("relation"),
                    kernel_table = self.table("kernel"),
                ))
                .map(|()| ((), 0))
        })?;
        Ok(())
    }

    /// Sorted, so two names on one relation always pick the same table.
    fn name_relations(&mut self, names: &HashMap<String, TermId>) {
        let sorted: BTreeMap<&String, &TermId> = names.iter().collect();
        for (name, rel) in sorted {
            if nameable(name) {
                self.names.entry(*rel).or_insert_with(|| name.clone());
            }
        }
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
        sql(&self.connection, "load_relations", |connection| {
            let mut statement = connection.prepare(&format!(
                "SELECT \"rel\",\"arity\",\"name\" FROM \"{}\" ORDER BY \"__id\"",
                self.table("relation")
            ))?;
            let mut cursor = statement.query(())?;
            while let Some(row) = cursor.next()? {
                products.push((
                    TermId(row.get::<_, i64>(0)? as u32),
                    row.get::<_, i64>(1)? as usize,
                    row.get::<_, Option<String>>(2)?,
                ));
            }
            Ok(((), products.len()))
        })?;
        for (rel, _, name) in &products {
            if let Some(name) = name {
                self.names.insert(*rel, name.clone());
            }
        }
        for (rel, arity, _) in products {
            let kinds = self.product_kinds(&self.table_for(rel, arity))?;
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
        sql(&self.connection, "begin", |connection| {
            connection
                .execute_batch("BEGIN IMMEDIATE")
                .map(|()| ((), 0))
        })?;
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
        sql(&self.connection, "commit", |connection| {
            connection.execute_batch("COMMIT").map(|()| ((), 0))
        })?;
        self.in_tick = false;
        Ok(())
    }

    fn rollback_tick(&mut self) -> Result<(), StoreError> {
        sql(&self.connection, "rollback", |connection| {
            connection.execute_batch("ROLLBACK").map(|()| ((), 0))
        })?;
        self.in_tick = false;
        Ok(())
    }
}
