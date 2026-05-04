// Walk `src/cst/dsls/<name>/src/parser.c` (+ optional scanner.c) and compile
// each as a static lib. Walker source is shared with the lib via `include!`.

include!("src/cst/build/grammar_walk.rs");

fn main() {
    compile_grammars("src/cst/dsls");
}
