use clock_types::*;

fn main() {
    let now: Rel<B, Tick> = input();
    let before = pre(&now);
    join(&now, &before);
}
