//! Python call edges the PyCG micro-suite oracle wants beyond a bare callee
//! name, each leg a same-file syntax rule. FAIL-FIRST receipts (suite case ->
//! recall before this arc, scored by `plans/extract-bench-2026-08-29/
//! pycg_score.py` at aggregate 69.49%):
//!
//! - container literals bound to a name: dicts/call 0%, dicts/type_coercion
//!   0%, dicts/nested 0%, lists/simple 25%, lists/nested 0%, dicts/param 50%.
//! - lambdas named `<lambdaN>` per scope, bound, passed, returned:
//!   lambdas/call 0%, lambdas/parameter_call 50%, lambdas/chained_calls 50%,
//!   lambdas/return_call 50%.
//! - call results as values (`x = f()`, `g(f())`, `{"a": f()}`, `return f()`)
//!   through the def's single return: returns/call 50%,
//!   direct_calls/assigned_call 50%, dicts/return 50%, args/param_call
//!   66.67%, dicts/return_assign 50%, returns/return_complex 83.33%.
//! - attribute values (`b = a.func`, `return self.func`, `self.x = self.f`):
//!   classes/assigned_call 0%, classes/tuple_assignment 25%,
//!   classes/return_call 50%, classes/return_call_direct 50%,
//!   classes/self_assignment 66.67%.
//! - the param rule through a callee bound to the def: args/nested_call
//!   33.33%, kwargs/assigned_call 50%.
//! - calling builtins (`map`, `filter`): builtins/map 25%.
//! - a parameter forwarded as an argument: lambdas/chained_calls 50%,
//!   classes/parameter_call 33.33%.
//! - `__init__` through same-file bases: mro/basic_init 50%, mro/two_parents
//!   50%, mro/parents_same_superclass 50%.
//! - in-place mutation (`d.update({...})`, `ls.append(f)`): dicts/update 0%
//!   and a wrong `func1` row (precision 98.99 after the container leg alone).

use std::process::Command;

fn edges(path: &str) -> Vec<(String, String)> {
    let out = Command::new(env!("CARGO_BIN_EXE_extract"))
        .arg("--resolve")
        .args([path, path])
        .output()
        .expect("extract binary runs");
    assert!(out.status.success());
    let mut pairs: Vec<(String, String)> = Vec::new();
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let Ok(fact) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if fact["record"] != "resolved_edge" {
            continue;
        }
        pairs.push((
            fact["caller_name"].as_str().unwrap_or("").to_string(),
            fact["callee_name"].as_str().unwrap_or("").to_string(),
        ));
    }
    pairs.sort();
    pairs.dedup();
    pairs
}

fn has(pairs: &[(String, String)], caller: &str, callee: &str) -> bool {
    pairs.iter().any(|(c, d)| c == caller && d == callee)
}

#[test]
fn container_literals_bind_their_slots() {
    let pairs = edges("tests/fixtures/py_call_grind/containers.py");
    assert!(
        has(&pairs, "", "func1"),
        "table[\"a\"]() -> func1: {pairs:?}"
    );
    assert!(
        has(&pairs, "", "func2"),
        "table[1]() -> func2, the int key: {pairs:?}"
    );
    assert!(has(&pairs, "", "func3"), "slots[2]() -> func3: {pairs:?}");
    assert!(
        has(&pairs, "", "func4"),
        "nested literal rebound then called: {pairs:?}"
    );
    assert!(
        has(&pairs, "by_param", "func1"),
        "a container parameter's element: {pairs:?}"
    );
}

#[test]
fn int_and_string_keys_stay_distinct() {
    let src = "def func1():\n    pass\n\ndef func2():\n    pass\n\nd = {1: func1, \"1\": func2}\n\nd[1]()\n";
    let dir = std::env::temp_dir().join("py_call_grind_keys");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("main.py");
    std::fs::write(&path, src).unwrap();
    let pairs = edges(path.to_str().unwrap());
    assert!(has(&pairs, "", "func1"), "{pairs:?}");
    assert!(
        !has(&pairs, "", "func2"),
        "the string key must not answer d[1]: {pairs:?}"
    );
}

#[test]
fn lambdas_are_named_per_scope_and_flow_by_value() {
    let pairs = edges("tests/fixtures/py_call_grind/lambdas.py");
    assert!(has(&pairs, "", "<lambda1>"), "module_lambda(1): {pairs:?}");
    assert!(
        has(&pairs, "func", "<lambda1>"),
        "func(module_lambda): {pairs:?}"
    );
    assert!(
        has(&pairs, "inline_func", "<lambda2>"),
        "an inline lambda argument: {pairs:?}"
    );
    // `make` returns its own scope's <lambda1>; `made()` reaches it and the
    // module-level `<lambda1>` row already covers that spelling.
    assert!(has(&pairs, "", "make"), "{pairs:?}");
}

#[test]
fn call_result_bindings_follow_the_single_return() {
    let pairs = edges("tests/fixtures/py_call_grind/call_result.py");
    assert!(
        has(&pairs, "", "return_func"),
        "bound = func(); bound(): {pairs:?}"
    );
    assert!(
        has(&pairs, "", "other_func"),
        "alias()() through a def-local binding: {pairs:?}"
    );
    assert!(has(&pairs, "", "dict_maker"), "{pairs:?}");
}

#[test]
fn attribute_values_bind_by_trailing_name() {
    let pairs = edges("tests/fixtures/py_call_grind/attributes.py");
    assert!(
        has(&pairs, "", "func3"),
        "handle = instance.func3; handle(): {pairs:?}"
    );
    assert!(
        has(&pairs, "", "func1"),
        "tuple-unpacked attribute, and func2()(): {pairs:?}"
    );
    assert!(
        has(&pairs, "run", "func3"),
        "self.stored = self.func3; self.stored(): {pairs:?}"
    );
}

#[test]
fn param_rule_sees_calls_through_a_bound_callee() {
    let pairs = edges("tests/fixtures/py_call_grind/alias_param.py");
    assert!(
        has(&pairs, "func", "param_func"),
        "bound_func(bound_param): {pairs:?}"
    );
    assert!(
        has(&pairs, "param_func", "nested_func"),
        "callback(nested_func) two hops in: {pairs:?}"
    );
    assert!(
        has(&pairs, "keyword_func", "keyword_target"),
        "keyword through an alias: {pairs:?}"
    );
}

#[test]
fn calling_builtins_call_their_named_arguments() {
    let pairs = edges("tests/fixtures/py_call_grind/builtin_map.py");
    assert!(has(&pairs, "", "func"), "{pairs:?}");
    assert!(has(&pairs, "", "func2"), "{pairs:?}");
    assert!(has(&pairs, "", "keep"), "filter: {pairs:?}");
}

#[test]
fn forwarded_parameters_carry_lambdas_two_hops() {
    let pairs = edges("tests/fixtures/py_call_grind/chained_params.py");
    assert!(has(&pairs, "func1", "<lambda1>"), "{pairs:?}");
    assert!(has(&pairs, "func2", "<lambda2>"), "{pairs:?}");
    assert!(has(&pairs, "func3", "<lambda3>"), "{pairs:?}");
}

#[test]
fn update_rebinds_the_slot_and_the_stale_slot_is_gone() {
    let pairs = edges("tests/fixtures/py_call_grind/chained_params.py");
    assert!(has(&pairs, "", "func2"), "{pairs:?}");
    assert!(
        !has(&pairs, "", "func3"),
        "the pre-update slot must not answer: {pairs:?}"
    );
}

#[test]
fn constructor_reaches_the_first_base_init() {
    let pairs = edges("tests/fixtures/py_call_grind/mro.py");
    assert!(
        has(&pairs, "", "__init__"),
        "Leaf() -> Base.__init__ through Left: {pairs:?}"
    );
}
