#[derive(Debug, Clone)]
struct Point { x: i32, y: i32 }
fn use_it() -> String {
    let p = Point { x: 1, y: 2 };
    format!("{:?}", p)
}
