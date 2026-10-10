use clock_types::*;

fn main() {
    let now: Rel<Present, Tick> = input();
    let before = pre(&now);
    join(&now, &before);
}
