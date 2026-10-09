use clock_types::*;

fn main() {
    let edges: Rel<B, Tick> = input();
    let files: Rel<Z, Tick> = input();
    let shown = sample(&files);
    let joined = join(&edges, &shown);
    let gone = negate(&joined);
    let before = pre(&gone);
    let older = negate(&before);
    let reach = iterate(&edges, |inner| {
        let outer = enter(&edges);
        join(inner, &outer)
    });
    let _ = (older, reach);
}
