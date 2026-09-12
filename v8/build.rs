// The DL7 grammar is v7's; v8 links the generated parser rather than copying it.
fn main() {
    let src = std::path::Path::new("../v7/tree-sitter-dl7/src");
    let parser = src.join("parser.c");
    println!("cargo:rerun-if-changed={}", parser.display());
    cc::Build::new()
        .include(src)
        .file(&parser)
        .warnings(false)
        .compile("tree_sitter_dl7");
}
