use clock_types::*;

fn main() {
    let state: Rel<B, Tick> = input();
    let changes: Rel<Z, Tick> = input();
    join(&state, &changes);
}
