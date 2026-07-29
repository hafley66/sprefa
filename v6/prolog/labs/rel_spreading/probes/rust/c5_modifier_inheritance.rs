// C5 modifier inheritance in Rust: does building one struct out of another's
// fields carry the source's DECLARED properties (derives, visibility)?
// EXPECTED: no. Field values move; declaration properties do not.

#[derive(Clone, Copy, Debug)]
struct SourceRow {
    id: i64,
}

// same field, no derives of its own
struct TargetRow {
    id: i64,
}

fn wants_copy<T: Copy>(_value: T) {}
fn wants_debug<T: std::fmt::Debug>(_value: T) {}

fn main() {
    let source = SourceRow { id: 1 };
    wants_copy(source);
    wants_debug(source);

    let target = TargetRow { id: source.id };
    // the target did not inherit Copy from the row it was built from
    wants_copy(target);
    let target2 = TargetRow { id: source.id };
    wants_debug(target2);
}
