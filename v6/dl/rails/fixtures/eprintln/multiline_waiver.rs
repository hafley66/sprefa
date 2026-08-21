// The marker sits on the closing line of a multi-line call. v5's [line-1, line]
// window against the `eprintln!` token line never saw it and reported the print;
// the statement's next sibling is that comment, so the structural rule waives it.
pub fn arg_error(flag: &str, got: usize, want: usize) {
    eprintln!(
        "{flag} carries {got} values and the rel declares {want} columns",
    ); // @eprintln-ok: CLI argument error before exit
}
