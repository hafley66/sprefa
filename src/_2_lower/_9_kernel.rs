//! The kernel relation tables. Port of `kernel_relation/2`
//! (`0_lowerer.pl:1958-1973`), `kernel_slot_label/3` (`:1278-1301`),
//! `kernel_relation_keys_for_expression/2` (`:1617-1627`) and
//! `kernel_return_position/2` (`:1902-1906`). `integer_comparison/3` is
//! `v7/src/1_libtime/0_evaluator.pl`.
//! Keys are `(owner, label)`: untyped `(None, "cons")` is `kernel(cons)`,
//! typed `(Some("int"), "add")` is `kernel(int, add)`.

pub const INTEGER_COMPARISONS: [&str; 6] = ["lt", "le", "gt", "ge", "eq", "ne"];

/// `int.lt` and its five siblings.
pub fn is_integer_comparison(owner: Option<&str>, label: &str) -> bool {
    owner == Some("int") && INTEGER_COMPARISONS.contains(&label)
}

/// `:1958`. Arity of a kernel relation, or `None` when the pair is not one.
pub fn kernel_relation(owner: Option<&str>, label: &str) -> Option<u32> {
    let arity = match (owner, label) {
        (None, "node" | "module" | "product" | "sum" | "nil") => 1,
        (None, ":" | "edge_snapshot" | "body") => 4,
        (None, "cons" | "edge_ref" | "intern" | "intern_snapshot") => 3,
        (None, "effect" | "def" | "head") => 2,
        (Some("int"), "add") => 3,
        (Some("any"), "lt") => 2,
        (Some("str"), "cons") => 3,
        (Some("str"), "nil") => 1,
        _ if is_integer_comparison(owner, label) => 2,
        _ => return None,
    };
    Some(arity)
}

/// `:1278`. `None` reproduces the `slot(Index, none)` fallback at `:1276`.
pub fn kernel_slot_label(owner: Option<&str>, label: &str, index: u32) -> Option<&'static str> {
    let slot = match (owner, label, index) {
        (None, ":" | "edge_snapshot", 0) => "owner",
        (None, ":" | "edge_snapshot", 1) => "name",
        (None, ":" | "edge_snapshot", 2) => "target",
        (None, ":" | "edge_snapshot", 3) => "index",
        (None | Some("str"), "nil", 0) => "return",
        (None | Some("str"), "cons", 0) => "head",
        (None | Some("str"), "cons", 1) => "tail",
        (None | Some("str"), "cons", 2) => "return",
        (None, "edge_ref" | "intern" | "intern_snapshot", 2) => "return",
        (None, "edge_ref", 0) => "owner",
        (None, "edge_ref", 1) => "label",
        (None, "intern" | "intern_snapshot", 0) => "constructor",
        (None, "intern" | "intern_snapshot", 1) => "arguments",
        (Some("int"), "add", 0) => "left",
        (Some("int"), "add", 1) => "right",
        (Some("int"), "add", 2) => "return",
        (None, "effect", 0) => "relation",
        (None, "effect", 1) => "application",
        (Some("any"), "lt", 0) => "left",
        (Some("any"), "lt", 1) => "right",
        (_, _, 0) if is_integer_comparison(owner, label) => "left",
        (_, _, 1) if is_integer_comparison(owner, label) => "right",
        _ => return None,
    };
    Some(slot)
}

/// `:1617`.
pub fn kernel_keys(owner: Option<&str>, label: &str) -> Vec<Vec<u32>> {
    match (owner, label) {
        (None, ":" | "edge_snapshot") => vec![vec![0, 1], vec![0, 3]],
        (None | Some("str"), "nil") => vec![vec![]],
        (None | Some("str"), "cons") => vec![vec![0, 1], vec![2]],
        (None, "edge_ref" | "intern" | "intern_snapshot") => vec![vec![0, 1]],
        (Some("int"), "add") | (None, "effect") | (Some("any"), "lt") => vec![vec![0, 1]],
        _ if is_integer_comparison(owner, label) => vec![vec![0, 1]],
        _ => vec![],
    }
}

/// `:1902`. At most one position per kernel op.
pub fn kernel_return_positions(owner: Option<&str>, label: &str) -> Vec<u32> {
    match (owner, label) {
        (None | Some("str"), "nil") => vec![0],
        (None | Some("str"), "cons") => vec![2],
        (None, "edge_ref" | "intern" | "intern_snapshot") => vec![2],
        (Some("int"), "add") => vec![2],
        _ => vec![],
    }
}
