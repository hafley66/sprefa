// C4 width subtyping in Rust: is a wider struct accepted where the narrower
// one is wanted? EXPECTED: refused, struct types are nominal.

struct Narrow {
    id: i64,
}

struct Wide {
    id: i64,
    extra: i64,
}

fn takes_narrow(_row: Narrow) {}

fn main() {
    let wide = Wide { id: 1, extra: 2 };
    takes_narrow(wide);

    // even a structurally IDENTICAL second struct is a different type
    struct AlsoNarrow {
        id: i64,
    }
    let also = AlsoNarrow { id: 1 };
    takes_narrow(also);

    // tuples are structural, and there width is exact
    fn takes_two(_t: (i64, i64)) {}
    takes_two((1, 2, 3));
}
