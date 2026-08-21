// the trailing form. v5's comment op could not see this one and needed a
// second match_line rule; one `any` covers both here.
pub fn report(error: &str) {
    eprintln!("{error}"); // @eprintln-ok: top-level CLI error before exit
}
