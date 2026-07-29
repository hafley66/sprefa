// C6 derived source in Rust: can a declaration's WIDTH be computed from
// another declaration? The only computed-width mechanism is a const generic
// length, and arithmetic on it is refused on stable.

struct Wider<const N: usize> {
    columns: [i64; N + 1],
}

trait HasRow {
    type Row;
}

struct Source;
impl HasRow for Source {
    type Row = (i64, i64);
}

// splicing an associated (derived) row into a struct declaration: no syntax
struct SplicedFromAssociated {
    ..<Source as HasRow>::Row,
    extra: i64,
}

fn main() {
    let _ = Wider::<2> { columns: [0; 3] };
    let _ = SplicedFromAssociated { extra: 1 };
}
