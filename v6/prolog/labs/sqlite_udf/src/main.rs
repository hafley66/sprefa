use rusqlite::params;
use serde_json::{json, Value};
use sprefa_v5::db;
use std::env;
use std::fs;
use std::path::PathBuf;

fn arg(name: &str) -> String {
    let args: Vec<String> = env::args().collect();
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].clone())
        .unwrap_or_else(|| panic!("missing {name}"))
}

fn decode(value: &str) -> String {
    value.replace("\\n", "\n").replace("\\t", "\t")
}

fn opt_text(row: &rusqlite::Row<'_>) -> rusqlite::Result<Option<String>> {
    row.get::<_, Option<String>>(0)
}

fn call_text(conn: &rusqlite::Connection, sql: &str, values: &[&str]) -> Value {
    let result: rusqlite::Result<Option<String>> = match values {
        [a] => conn.query_row(sql, params![a], opt_text),
        [a, b] => conn.query_row(sql, params![a, b], opt_text),
        [a, b, c] => conn.query_row(sql, params![a, b, c], opt_text),
        _ => unreachable!(),
    };
    match result {
        Ok(Some(value)) => json!(value),
        Ok(None) => Value::Null,
        Err(error) => json!({"error": error.to_string()}),
    }
}

fn call_split(conn: &rusqlite::Connection, text: &str) -> Value {
    let result: rusqlite::Result<Option<String>> = conn.query_row(
        "SELECT sprf_split(?1, ?2, ?3)",
        params![text, "/", -1_i64],
        opt_text,
    );
    match result {
        Ok(Some(value)) => json!(value),
        Ok(None) => Value::Null,
        Err(error) => json!({"error": error.to_string()}),
    }
}

fn call_lines(conn: &rusqlite::Connection, value: &str) -> Value {
    let result: rusqlite::Result<Option<i64>> = conn.query_row("SELECT sprf_lines(?1)", params![value], |row| row.get(0));
    match result {
        Ok(Some(value)) => json!(value),
        Ok(None) => Value::Null,
        Err(error) => json!({"error": error.to_string()}),
    }
}

fn call_bool(conn: &rusqlite::Connection, sql: &str, values: &[&str]) -> Value {
    let result: rusqlite::Result<Option<bool>> = conn.query_row(sql, params![values[0], values[1]], |row| row.get(0));
    match result {
        Ok(Some(value)) => json!(value),
        Ok(None) => Value::Null,
        Err(error) => json!({"error": error.to_string()}),
    }
}

fn call_sym(conn: &rusqlite::Connection, sql: &str, value: &str) -> Value {
    let result: rusqlite::Result<Option<i64>> = conn.query_row(sql, params![value], |row| row.get(0));
    match result {
        Ok(Some(value)) => json!(value),
        Ok(None) => Value::Null,
        Err(error) => json!({"error": error.to_string()}),
    }
}

fn selected_functions(conn: &rusqlite::Connection) -> rusqlite::Result<Vec<Value>> {
    let mut selected = Vec::new();
    let mut functions = conn.prepare("SELECT name, narg FROM pragma_function_list ORDER BY name, narg")?;
    let rows = functions.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))?;
    for row in rows {
        let (name, narg) = row?;
        if ["lower", "upper", "trim", "replace", "regexp", "split", "sprf_split"].contains(&name.as_str()) {
            selected.push(json!({"name": name, "narg": narg}));
        }
    }
    Ok(selected)
}

fn main() -> anyhow::Result<()> {
    let db_path = arg("--db");
    let out_path = PathBuf::from(arg("--out"));
    let corpus_path = PathBuf::from(env::var("SPREFA_UDF_CORPUS")?);
    let bare = rusqlite::Connection::open_in_memory()?;
    let bare_sqlite_version: String = bare.query_row("SELECT sqlite_version()", [], |row| row.get(0))?;
    let bare_core = selected_functions(&bare)?;

    let database = db::open(Some(&db_path))?;
    let conn = database.conn();

    let sqlite_version: String = conn.query_row("SELECT sqlite_version()", [], |row| row.get(0))?;
    let registered_functions = selected_functions(conn)?;

    let mut output = vec![json!({
        "kind": "meta",
        "sqlite_version": sqlite_version,
        "bare_sqlite_version": bare_sqlite_version,
        "bare_core_functions": bare_core,
        "registered_function_subset": registered_functions,
        "v5_registrations": [
            "regexp", "sprf_split", "sprf_sym_intern", "sprf_lower", "sprf_upper",
            "sprf_lcfirst", "sprf_ucfirst", "sprf_trim", "sprf_norm", "sprf_strip_prefix",
            "sprf_strip_suffix", "sprf_sym", "sprf_lines", "sprf_replace_re"
        ]
    })];

    for line in fs::read_to_string(corpus_path)?.lines() {
        if line.trim().is_empty() || line.starts_with('#') { continue; }
        let fields: Vec<&str> = line.split("\\t").collect();
        if fields.len() != 6 { anyhow::bail!("bad corpus line: {line}"); }
        let id = fields[0];
        let text = decode(fields[1]);
        let prefix = decode(fields[2]);
        let suffix = decode(fields[3]);
        let pattern = decode(fields[4]);
        let replacement = decode(fields[5]);
        let text_ref = text.as_str();
        let prefix_ref = prefix.as_str();
        let suffix_ref = suffix.as_str();
        let pattern_ref = pattern.as_str();
        let replacement_ref = replacement.as_str();
        let push = |function: &str, result: Value, output: &mut Vec<Value>| {
            output.push(json!({"kind": "value", "id": id, "function": function, "result": result}));
        };
        push("sprf_lower", call_text(conn, "SELECT sprf_lower(?1)", &[text_ref]), &mut output);
        push("sprf_upper", call_text(conn, "SELECT sprf_upper(?1)", &[text_ref]), &mut output);
        push("sprf_lcfirst", call_text(conn, "SELECT sprf_lcfirst(?1)", &[text_ref]), &mut output);
        push("sprf_ucfirst", call_text(conn, "SELECT sprf_ucfirst(?1)", &[text_ref]), &mut output);
        push("sprf_trim", call_text(conn, "SELECT sprf_trim(?1)", &[text_ref]), &mut output);
        push("sprf_norm", call_text(conn, "SELECT sprf_norm(?1)", &[text_ref]), &mut output);
        push("sprf_strip_prefix", call_text(conn, "SELECT sprf_strip_prefix(?1, ?2)", &[text_ref, prefix_ref]), &mut output);
        push("sprf_strip_suffix", call_text(conn, "SELECT sprf_strip_suffix(?1, ?2)", &[text_ref, suffix_ref]), &mut output);
        push("sprf_sym", call_sym(conn, "SELECT sprf_sym(?1)", text_ref), &mut output);
        push("sprf_sym_intern", call_sym(conn, "SELECT sprf_sym_intern(?1)", text_ref), &mut output);
        push("sprf_lines", call_lines(conn, text_ref), &mut output);
        push("sprf_replace_re", call_text(conn, "SELECT sprf_replace_re(?1, ?2, ?3)", &[text_ref, pattern_ref, replacement_ref]), &mut output);
        push("regexp", call_bool(conn, "SELECT regexp(?1, ?2)", &[pattern_ref, text_ref]), &mut output);
        push("sprf_split", call_split(conn, text_ref), &mut output);
    }

    // A sidecar extract process has the same rusqlite registration seam as this
    // capture binary. Keep a direct registration probe in the receipt.
    conn.create_scalar_function(
        "sidecar_probe",
        1,
        rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| Ok(Some(format!("sidecar:{}", ctx.get::<String>(0)?))),
    )?;
    let sidecar: String = conn.query_row("SELECT sidecar_probe('ok')", [], |row| row.get(0))?;
    output.push(json!({"kind": "sidecar_probe", "result": sidecar}));

    let mut text = String::new();
    for value in output {
        text.push_str(&serde_json::to_string(&value)?);
        text.push('\n');
    }
    fs::write(out_path, text)?;
    Ok(())
}
