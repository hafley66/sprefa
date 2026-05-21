use anyhow::Result;
use rusqlite::Connection;

pub fn open(path: Option<&str>) -> Result<Connection> {
    let conn = match path {
        Some(p) => Connection::open(p)?,
        None => Connection::open_in_memory()?,
    };
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
    Ok(conn)
}
