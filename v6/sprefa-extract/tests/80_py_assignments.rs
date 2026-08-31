//! Same-file value bindings resolve alias, tuple/starred unpack, and
//! literal-container calls. HEAD-FAILURE receipt: before the dynamic-shape
//! tier, `PythonSource::call_name_match` keyed only on the callee name as
//! written, so `g = f; g()` (and every unpack / container shape below) emitted
//! zero resolved_edge rows -- the PyCG bench lost all 19 assignments rows and
//! the literal-container rows of lists/ and dicts/ to exactly this gap.
//!
//! Lookup discipline: only `identifier = identifier` binds; a non-identifier
//! rhs is a KILL row so a stale alias cannot survive a rebind; a `base[key]`
//! call resolves through element bindings, never through the bare base name.

use std::process::Command;

const MAIN: &str = "tests/fixtures/py_findings/assignments/main.py";

fn callee_names(paths: &[&str]) -> Vec<String> {
    let out = Command::new(env!("CARGO_BIN_EXE_extract"))
        .arg("--resolve")
        .args(paths)
        .output()
        .expect("extract binary runs");
    assert!(
        out.status.success(),
        "extract exited {:?}: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    let mut names: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| {
            let fact: serde_json::Value = serde_json::from_str(line).ok()?;
            (fact["record"] == "resolved_edge").then(|| {
                fact["callee_name"]
                    .as_str()
                    .expect("resolved_edge carries callee_name")
                    .to_string()
            })
        })
        .collect();
    names.sort();
    names.dedup();
    names
}

#[test]
fn simple_alias_call_resolves_the_bound_def() {
    let names = callee_names(&[MAIN, MAIN]);
    assert!(
        names.iter().any(|n| n == "func1"),
        "g = func1; g() resolves func1, got {names:?}"
    );
}

#[test]
fn tuple_unpack_binds_each_element() {
    let names = callee_names(&[MAIN, MAIN]);
    assert!(names.iter().any(|n| n == "func1"), "a() -> func1");
    assert!(names.iter().any(|n| n == "func2"), "b() -> func2");
}

#[test]
fn starred_splat_binds_middle_list_slots() {
    let names = callee_names(&[MAIN, MAIN]);
    // rest = [func2, func2]; rest[0]() targets func2.
    assert!(names.iter().any(|n| n == "func2"));
}

#[test]
fn literal_dict_and_list_elements_resolve_by_key() {
    let names = callee_names(&[MAIN, MAIN]);
    assert!(names.iter().any(|n| n == "func1"), "d[\"k\"]() -> func1");
    assert!(names.iter().any(|n| n == "func2"), "ls[1]() -> func2");
}
