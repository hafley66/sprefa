use rusqlite::{Error, Result};

pub fn reject(message: &str) -> Error {
    Error::UserFunctionError(std::io::Error::other(format!("sqlite_ivm: {message}")).into())
}
pub fn ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}
// Decode an identifier token already recognized by the upstream parser.
pub fn name(token: &str) -> String {
    match token.as_bytes().first() {
        Some(b'"' | b'\'' | b'`') => {
            let q = &token[..1];
            token[1..token.len() - 1].replace(&q.repeat(2), q)
        }
        Some(b'[') => token[1..token.len() - 1].to_owned(),
        _ => token.to_owned(),
    }
}
pub fn valid_name(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 120
        || value.contains('\0')
        || value.to_ascii_lowercase().starts_with("sqlite_")
        || value.starts_with("__ivm_")
    {
        return Err(reject("invalid or reserved name (1..120 bytes required)"));
    }
    Ok(())
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Column {
    pub side: usize,
    pub name: String,
}
#[derive(Clone, Debug)]
pub struct Table {
    pub name: String,
    pub alias: String,
    pub columns: Vec<String>,
}
#[derive(Debug)]
pub struct Plan {
    pub tables: [Table; 2],
    pub keys: [Column; 2],
    pub values: Vec<Column>,
    pub outputs: [String; 3],
}
impl Plan {
    pub fn column(&self, c: &Column, image: Option<(usize, &str)>) -> String {
        let prefix = match image {
            Some((s, i)) if s == c.side => i.to_owned(),
            _ => format!("b{}", c.side),
        };
        format!("{prefix}.{}", ident(&c.name))
    }
    pub fn value(&self, image: Option<(usize, &str)>) -> String {
        self.values
            .iter()
            .map(|c| self.column(c, image))
            .collect::<Vec<_>>()
            .join(" * ")
    }
    pub fn select(&self) -> String {
        format!(
            "SELECT {}, count(*), sum({}) FROM main.{} b0 JOIN main.{} b1 ON {} = {} GROUP BY {}",
            self.column(&self.keys[0], None),
            self.value(None),
            ident(&self.tables[0].name),
            ident(&self.tables[1].name),
            self.column(&self.keys[0], None),
            self.column(&self.keys[1], None),
            self.column(&self.keys[0], None)
        )
    }
}
