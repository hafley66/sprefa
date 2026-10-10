use clock_types::*;

fn main() {
    let state: Rel<Present, Tick> = input();
    let changes: Rel<Change, Tick> = input();
    join(&state, &changes);
}
