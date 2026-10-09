use clock_types::*;

fn main() {
    let edges: Rel<B, Tick> = input();
    iterate(&edges, |inner| negate(inner));
}
