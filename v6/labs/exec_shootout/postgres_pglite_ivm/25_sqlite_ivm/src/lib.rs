#[path = "1_bind.rs"]
mod bind;
#[path = "2_lower.rs"]
mod lower;
#[path = "3_telemetry.rs"]
mod telemetry;
#[path = "0_types.rs"]
mod types;
use rusqlite::{ffi, functions::FunctionFlags, Connection, Result};
use std::{
    os::raw::{c_char, c_int},
    sync::atomic::Ordering,
    time::Instant,
};
use types::*;

/// # Safety
/// Called by SQLite with the ABI table. Bindings/initialization are rusqlite's.
#[no_mangle]
pub unsafe extern "C" fn sqlite3_extension_init(
    db: *mut ffi::sqlite3,
    error: *mut *mut c_char,
    api: *mut ffi::sqlite3_api_routines,
) -> c_int {
    Connection::extension_init2(db, error, api, init)
}
fn atomic<T>(db: &Connection, operation: impl FnOnce() -> Result<T>) -> Result<T> {
    db.execute_batch("SAVEPOINT __ivm_install")?;
    match operation() {
        Ok(value) => {
            db.execute_batch("RELEASE __ivm_install")?;
            Ok(value)
        }
        Err(error) => {
            db.execute_batch("ROLLBACK TO __ivm_install; RELEASE __ivm_install")?;
            Err(error)
        }
    }
}
fn init(db: Connection) -> Result<bool> {
    if rusqlite::version_number() < 3037000 {
        return Err(reject("SQLite >= 3.37 required"));
    }
    let telemetry = telemetry::Telemetry::new();
    telemetry.event("load", 0, "", Instant::now(), 0);
    let flags = FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DIRECTONLY;
    let log = telemetry.clone();
    db.create_scalar_function(c"sqlite_ivm_create",2,flags,move |ctx| {
        let view:String = ctx.get(0)?; let sql:String = ctx.get(1)?;
        let started = Instant::now(); let op = log.next();
        valid_name(&view)?;
        let id = lower::prefix(&view);
        log.event("install",op,&id,started,0);
        let db = unsafe {ctx.get_connection()?};
        let result = atomic(&db,|| {
            let plan = bind::bind(&db,&sql)?;
            log.event("bind",op,&id,started,0);
            lower::check_initial(&db,&plan)?;
            for (_,object) in lower::objects(&view) {
                let exists:bool = db.query_row("SELECT EXISTS(SELECT 1 FROM main.sqlite_schema WHERE name=? COLLATE NOCASE) OR EXISTS(SELECT 1 FROM temp.sqlite_schema WHERE name=? COLLATE NOCASE)",[&object,&object],|r|r.get(0))?;
                if exists { return Err(reject("schema name collision")); }
            }
            let ddl = lower::lower(&plan,&view);
            log.event("lower",op,&id,started,0);
            for sql in ddl { db.execute_batch(&sql)?; }
            log.event("schema",op,&id,started,0);
            Ok(view.clone())
        });
        let code = result.as_ref().err().map(|e|e.sqlite_error().map(|e|e.extended_code).unwrap_or(ffi::SQLITE_ERROR)).unwrap_or(0);
        log.event(if result.is_ok(){"install_release"}else{"install_rollback"},op,&id,started,code);
        result
    })?;
    let log = telemetry.clone();
    db.create_scalar_function(c"sqlite_ivm_drop", 1, flags, move |ctx| {
        let view: String = ctx.get(0)?;
        valid_name(&view)?;
        let started = Instant::now();
        let op = log.next();
        let id = lower::prefix(&view);
        let db = unsafe { ctx.get_connection()? };
        let result = atomic(&db, || {
            let version: i64 = db.query_row(
                &format!(
                    "SELECT version FROM main.{} WHERE id=1",
                    ident(&format!("{id}_meta"))
                ),
                [],
                |r| r.get(0),
            )?;
            if version != 1 {
                return Err(reject("unknown metadata version"));
            }
            for (kind, object) in lower::objects(&view) {
                db.execute_batch(&format!("DROP {kind} main.{}", ident(&object)))?;
            }
            Ok(view.clone())
        });
        let code = result
            .as_ref()
            .err()
            .map(|e| {
                e.sqlite_error()
                    .map(|e| e.extended_code)
                    .unwrap_or(ffi::SQLITE_ERROR)
            })
            .unwrap_or(0);
        log.event(
            if result.is_ok() {
                "drop_release"
            } else {
                "drop_rollback"
            },
            op,
            &id,
            started,
            code,
        );
        result
    })?;
    let log = telemetry.clone();
    db.create_scalar_function(c"sqlite_ivm_log", 1, flags, move |ctx| {
        let limit: i64 = ctx.get(0)?;
        if !(0..=10000).contains(&limit) {
            return Err(reject("log limit must be 0..10000"));
        }
        log.remaining.store(limit as u64, Ordering::Relaxed);
        Ok(limit)
    })?;
    db.create_scalar_function(c"sqlite_ivm_metrics", 2, flags, move |ctx| {
        let view: String = ctx.get(0)?;
        valid_name(&view)?;
        let enabled: i64 = ctx.get(1)?;
        if !(0..=1).contains(&enabled) {
            return Err(reject("metrics enabled must be 0 or 1"));
        }
        let db = unsafe { ctx.get_connection()? };
        db.execute(
            &format!(
                "UPDATE main.{} SET enabled=? WHERE id=1",
                ident(&format!("{}_meta", lower::prefix(&view)))
            ),
            [enabled],
        )?;
        Ok(enabled)
    })?;
    db.create_scalar_function(c"sqlite_ivm_version",0,flags,move |_| {
        Ok(serde_json::json!({"extension":env!("CARGO_PKG_VERSION"),"sqlite":rusqlite::version(),"sqlite_source_id":unsafe {std::ffi::CStr::from_ptr(ffi::sqlite3_sourceid())}.to_string_lossy(),"parser":"sqlite3-parser=0.17.0","abi":"rusqlite=0.40.2/libsqlite3-sys loadable_extension","writer":"recursive_triggers=ON; trusted_schema=ON","algorithm":"signed_join_count_sum"}).to_string())
    })?;
    Ok(false)
}
