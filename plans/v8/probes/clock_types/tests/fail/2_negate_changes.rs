use clock_types::*;

fn main() {
    let changes: Rel<Change, Tick> = input();
    negate(&changes);
}
