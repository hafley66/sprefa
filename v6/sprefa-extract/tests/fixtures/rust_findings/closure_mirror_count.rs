// closure_mirror_count.rs: the COUNT shape for kink 3. Three closure-caller
// edges, one closure nested inside another, so a mirror emitted per closure
// FRAME rather than per closure-caller EDGE over-counts and a mirror emitted
// per named def under-counts.
//
// EXPECTED, `extract --resolve --family call`: 8 resolved_edge rows.
//   5 primaries, one per site:
//     entry      -> spawn   (site 1, outer)
//     closure@a  -> spawn   (site 2, inside the outer closure)
//     closure@b  -> worker  (site 3, inside the nested closure)
//     entry      -> spawn   (site 4)
//     closure@c  -> other   (site 5)
//   3 mirrors, one per closure-caller primary, all naming `entry`:
//     entry -> spawn, entry -> worker, entry -> other
// OBSERVED at c60e5c4cc: 5 rows, no mirror, and `worker` and `other` are
// reachable from nothing a BFS over named defs can walk.

fn worker() -> u32 {
    1
}

fn other() -> u32 {
    2
}

fn spawn<F: Fn() -> u32>(f: F) -> u32 {
    f()
}

fn entry() -> u32 {
    spawn(|| spawn(|| worker())) + spawn(|| other())
}
