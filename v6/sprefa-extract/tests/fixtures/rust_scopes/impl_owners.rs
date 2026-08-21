// One method name, five declarations: two traits declaring it, two self types
// implementing one trait, and an inherent impl. Name alone separates none.

struct Alpha;
struct Beta;

trait Draw {
    fn draw(&self);
}

trait Erase {
    fn draw(&self);
}

impl Draw for Alpha {
    fn draw(&self) {}
}

impl Draw for Beta {
    fn draw(&self) {}
}

impl Erase for Alpha {
    fn draw(&self) {}
}

impl Alpha {
    fn draw(&self) {}
}
