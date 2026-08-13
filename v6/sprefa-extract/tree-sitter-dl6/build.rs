fn main() {
    let src_dir = std::path::Path::new("src");
    println!(
        "cargo:rerun-if-changed={}",
        src_dir.join("parser.c").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        src_dir.join("tree_sitter/parser.h").display()
    );

    let mut build = cc::Build::new();
    build
        .include(&src_dir)
        .file(src_dir.join("parser.c"))
        .warnings(false)
        .pic(true);

    if cfg!(target_os = "macos") {
        build.flag("-fvisibility=default");
    }

    build.compile("tree-sitter-dl6");
}
