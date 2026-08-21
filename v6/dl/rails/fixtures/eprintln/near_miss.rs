// an @eprintln-ok comment that waives NOTHING because it neighbours a
// different statement. Both prints stay findings.
pub fn two(kind: &str) {
    // @eprintln-ok: this marker sits above the tracing call, not a print
    tracing::info!(kind, "kind");
    eprintln!("a");
    eprintln!("b");
}
