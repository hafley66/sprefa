// Function bodies, the plane the checker walk pays for. Every callee path here
// is reached twice by `descendants()`: once as its `CallExpr` and once bare.

pub struct Point {
    pub x: u32,
    pub y: u32,
}

pub fn origin() -> Point {
    Point { x: 0, y: 0 }
}

pub fn shifted(base: Point, dx: u32, dy: u32) -> Point {
    let x = base.x + dx;
    let y = base.y + dy;
    Point { x, y }
}

pub fn walked(steps: u32) -> Point {
    let mut here = origin();
    let mut step = 0;
    while step < steps {
        here = shifted(here, step, step);
        step = step + 1;
    }
    here
}

pub fn described(point: &Point) -> String {
    let mut text = String::new();
    text.push_str("x");
    text.push_str("y");
    text.push_str("z");
    let _ = point.x;
    text
}
