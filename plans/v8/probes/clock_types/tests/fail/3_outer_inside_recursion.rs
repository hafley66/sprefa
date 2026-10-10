use clock_types::*;

fn main() {
    let edges: Rel<Present, Tick> = input();
    iterate(&edges, |inner| join(inner, &edges));
}
