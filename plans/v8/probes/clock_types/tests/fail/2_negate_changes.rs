use clock_types::*;

fn main() {
    let changes: Rel<Z, Tick> = input();
    negate(&changes);
}
