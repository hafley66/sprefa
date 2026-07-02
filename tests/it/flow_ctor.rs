//! Instantiation-centric value flow (`examples/flow-ctor.dl`).
//!
//! The lift marks instantiations as `new` df_nodes (Rust struct literal +
//! capitalized tuple-struct/variant ctor, TS `new Foo()` + object literal,
//! Kotlin capitalized ctor call), records positional argument slots in
//! `df_arg`, and named composite fills in `df_field`. `flow-ctor.dl` layers
//! the queries: ctor inventory, per-field fills, and field-SENSITIVE flow —
//! a value stored into field F reaches a member read of F, and only F.
//!
//! THE GATE (per language): `h` fills field `host` of a Cfg instantiation;
//! downstream only `.host` is read. field_flow must contain a `host` row and
//! must NOT invent a `port` row (no read of port exists).

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");
const PROG: &str = include_str!("../../examples/flow-ctor.dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("flow_ctor_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    dir
}

fn run(dir: &Path) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), PROG).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--root", dir.to_str().unwrap(), "--db", dir.join("db").to_str().unwrap()])
        .output()
        .expect("run dl");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// Split dl stdout into one section per `?` query (header + its data rows).
fn sections(out: &str) -> Vec<String> {
    let mut secs: Vec<String> = Vec::new();
    for line in out.lines() {
        if line.starts_with("? ") {
            secs.push(String::new());
        }
        if let Some(cur) = secs.last_mut() {
            if !cur.is_empty() {
                cur.push('\n');
            }
            cur.push_str(line);
        }
    }
    secs
}

fn rows(sec: &str) -> Vec<Vec<String>> {
    // strip only the 2-space indent, not leading tabs: an empty FIRST column
    // (the anonymous object-literal ty) must survive as "".
    sec.lines()
        .filter(|l| l.starts_with("  ") && l.contains('\t') && !l.contains("(0 rows)"))
        .map(|l| {
            l.strip_prefix("  ")
                .unwrap_or(l)
                .split('\t')
                .map(|s| s.trim_end().to_string())
                .collect()
        })
        .collect()
}

/// Instantiation + field-sensitive read, one fn, per language. Section order in
/// flow-ctor.dl: ctor, field_fill, arg_slot, field_flow.
#[test]
fn ctor_fields_flow_field_sensitively_per_language() {
    // In each: an instantiation fills host (from param h) and port (a literal);
    // only .host is read downstream.
    let rust = "struct Cfg { host: i32, port: i32 }\n\
                fn go(h: i32) {\n    let c = Cfg { host: h, port: 1 };\n    let x = c.host;\n    let _ = x;\n}\n";
    let ts = "function go(h: number): void {\n    const c = { host: h, port: 1 };\n    const x = c.host;\n    const _u = x;\n}\n";
    let kotlin = "class Cfg(val host: Int, val port: Int)\n\
                  fun go(h: Int) {\n    val c = Cfg(host = h, port = 1)\n    val x = c.host\n    val u = x\n}\n";

    for (lang, ext, srcfile, ty) in [
        ("rust", "rs", rust, "Cfg"),
        ("ts", "ts", ts, ""), // object literal: anonymous
        ("kotlin", "kt", kotlin, "Cfg"),
    ] {
        let d = sandbox(lang);
        fs::write(d.join(format!("src/lib.{ext}")), srcfile).unwrap();
        let (code, out, err) = run(&d);
        assert_eq!(code, 0, "[{lang}] must not error:\n{err}");

        let secs = sections(&out);
        assert!(secs.len() >= 4, "[{lang}] expected 4 query sections:\n{out}");

        // ctor inventory: one instantiation of the expected type, inside go.
        let ctors = rows(&secs[0]);
        assert!(
            ctors.iter().any(|r| r.len() >= 3 && r[1] == ty && r[2].contains("::go")),
            "[{lang}] expected a ctor row of type {ty:?} in go:\n{out}"
        );

        // field_fill: both fields recorded against the constructed type.
        let fills = rows(&secs[1]);
        let filled: HashSet<(&str, &str)> = fills
            .iter()
            .filter(|r| r.len() >= 2)
            .map(|r| (r[0].as_str(), r[1].as_str()))
            .collect();
        assert!(filled.contains(&(ty, "host")), "[{lang}] expected field_fill host:\n{out}");
        assert!(filled.contains(&(ty, "port")), "[{lang}] expected field_fill port:\n{out}");

        // DECISIVE: field-sensitive flow fires for host (stored AND read) ...
        let flows = rows(&secs[3]);
        assert!(
            flows.iter().any(|r| r.len() >= 3 && r[2] == "host"),
            "[{lang}] expected field_flow for host (value -> ctor -> .host read):\n{out}"
        );
        // ... and NOT for port: no member read of port exists, so matching the
        // field name is what separates this from the blanket edge.
        assert!(
            !flows.iter().any(|r| r.len() >= 3 && r[2] == "port"),
            "[{lang}] field_flow must NOT invent a port row (no .port read):\n{out}"
        );
    }
}

/// Positional slots: `df_arg` records which slot each value feeds — 0-based
/// args, method receiver at -1 — for calls and instantiations alike. TS `new`
/// carries the class name; a Rust tuple-struct ctor (`Some(x)` shape) is a
/// `new` node too.
#[test]
fn arg_slots_cover_calls_receivers_and_ctors() {
    let d = sandbox("slots_rust");
    fs::write(
        d.join("src/lib.rs"),
        "struct Wrap(i32);\n\
         fn eat(v: i32) {}\n\
         fn go(a: i32, items: Vec<i32>) {\n    \
             let w = Wrap(a);\n    \
             let n = items.len();\n    \
             eat(n as i32);\n\
         }\n",
    )
    .unwrap();
    let (code, out, err) = run(&d);
    assert_eq!(code, 0, "must not error:\n{err}");
    let secs = sections(&out);

    // ctor: Wrap(a) is a `new` node (capitalized tuple-struct callee).
    let ctors = rows(&secs[0]);
    assert!(
        ctors.iter().any(|r| r.len() >= 2 && r[1] == "Wrap"),
        "expected Wrap tuple-struct ctor as a new node:\n{out}"
    );

    // arg_slot: slot 0 rows exist (Wrap's arg + eat's arg), and the method
    // receiver of items.len() sits at slot -1 as a var_read.
    let slots = rows(&secs[2]);
    assert!(
        slots.iter().any(|r| r.len() >= 4 && r[1] == "0"),
        "expected slot-0 arg rows:\n{out}"
    );
    assert!(
        slots.iter().any(|r| r.len() >= 4 && r[1] == "-1" && r[3] == "var_read"),
        "expected the method receiver at slot -1:\n{out}"
    );

    // TS: `new Widget(h)` carries the class name on the new node.
    let d = sandbox("slots_ts");
    fs::write(
        d.join("src/lib.ts"),
        "class Widget { constructor(public size: number) {} }\n\
         function go(h: number): void {\n    const w = new Widget(h);\n    const _u = w;\n}\n",
    )
    .unwrap();
    let (code, out, err) = run(&d);
    assert_eq!(code, 0, "ts must not error:\n{err}");
    let secs = sections(&out);
    let ctors = rows(&secs[0]);
    assert!(
        ctors.iter().any(|r| r.len() >= 2 && r[1] == "Widget"),
        "expected new Widget(...) as a new node with the class name:\n{out}"
    );
    let slots = rows(&secs[2]);
    assert!(
        slots.iter().any(|r| r.len() >= 4 && r[1] == "0" && r[3] == "var_read"),
        "expected Widget's ctor arg at slot 0:\n{out}"
    );
}
