//! `V=v1|v2` bakes the answer prefix in at build time; rebuild over the same
//! output path with a different `V` to exercise the host's reload.
fn main() {
    println!("cargo:rerun-if-env-changed=V");
    let v = std::env::var("V").unwrap_or_else(|_| "v1".to_string());
    println!("cargo:rustc-env=DYLIB_ECHO_PREFIX={v}");
}
