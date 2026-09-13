//! The kernel relation tables. Port of `kernel_relation/2`
//! (`0_lowerer.pl:1958-1973`), `kernel_slot_label/3` (`:1278-1301`),
//! `kernel_relation_keys_for_expression/2` (`:1617-1627`) and
//! `kernel_return_position/2` (`:1902-1906`). `integer_comparison/3` is
//! `v7/src/1_libtime/0_evaluator.pl`.

pub const INTEGER_COMPARISONS: [&str; 6] =
    ["int_lt", "int_le", "int_gt", "int_ge", "int_eq", "int_ne"];

pub fn is_integer_comparison(name: &str) -> bool {
    INTEGER_COMPARISONS.contains(&name)
}

/// `:1958`. Arity of a kernel relation, or `None` when the name is not one.
pub fn kernel_relation(name: &str) -> Option<u32> {
    let arity = match name {
        "node" | "module" | "product" | "sum" | "nil" => 1,
        ":" | "edge_snapshot" | "body" => 4,
        "cons" | "edge_ref" | "intern" | "intern_snapshot" => 3,
        "def" | "head" => 2,
        _ if is_integer_comparison(name) => 2,
        _ => return None,
    };
    Some(arity)
}

/// `:1278`. `None` reproduces the `slot(Index, none)` fallback at `:1276`.
pub fn kernel_slot_label(name: &str, index: u32) -> Option<&'static str> {
    let label = match (name, index) {
        (":", 0) | ("edge_snapshot", 0) => "owner",
        (":", 1) | ("edge_snapshot", 1) => "name",
        (":", 2) | ("edge_snapshot", 2) => "target",
        (":", 3) | ("edge_snapshot", 3) => "index",
        ("nil", 0) => "return",
        ("cons", 0) => "head",
        ("cons", 1) => "tail",
        ("cons", 2) | ("edge_ref", 2) | ("intern", 2) | ("intern_snapshot", 2) => "return",
        ("edge_ref", 0) => "owner",
        ("edge_ref", 1) => "label",
        ("intern", 0) | ("intern_snapshot", 0) => "constructor",
        ("intern", 1) | ("intern_snapshot", 1) => "arguments",
        (other, 0) if is_integer_comparison(other) => "left",
        (other, 1) if is_integer_comparison(other) => "right",
        _ => return None,
    };
    Some(label)
}

/// `:1617`.
pub fn kernel_keys(name: &str) -> Vec<Vec<u32>> {
    match name {
        ":" | "edge_snapshot" => vec![vec![0, 1], vec![0, 3]],
        "nil" => vec![vec![]],
        "cons" => vec![vec![0, 1], vec![2]],
        "edge_ref" | "intern" | "intern_snapshot" => vec![vec![0, 1]],
        other if is_integer_comparison(other) => vec![vec![0, 1]],
        _ => vec![],
    }
}

/// `:1902`. At most one position per kernel name.
pub fn kernel_return_positions(name: &str) -> Vec<u32> {
    match name {
        "nil" => vec![0],
        "cons" | "edge_ref" | "intern" | "intern_snapshot" => vec![2],
        _ => vec![],
    }
}
