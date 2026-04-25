use pipeline::op_languages;
use sprefa_parse::host_parse_with_injections;
use std::path::PathBuf;
use std::sync::Arc;

#[test]
fn glob_literal_with_dot() {
    let src = "glob(file.txt)";
    let file: Arc<std::path::Path> = Arc::from(PathBuf::from("test.sprf").as_path());
    let (parsed, errs) = host_parse_with_injections(
        src,
        file,
        &op_languages::language_of,
    );
    eprintln!("errors: {:#?}", errs);
    eprintln!("pipes: {}", parsed.pipes.len());
    eprintln!("injected: {}", parsed.injected.len());
    if let Some(inj) = parsed.injected.first() {
        eprintln!("injected sexp: {}", inj.tree.root_node().to_sexp());
    }
    if let Some(pipe) = parsed.pipes.first() {
        for (i, op) in pipe.ops.iter().enumerate() {
            eprintln!("op[{i}] name={}", op.name);
        }
        eprintln!("host sexp: {}", parsed.tree.root_node().to_sexp());
    }
    assert!(errs.is_empty(), "errs: {errs:?}");
}
