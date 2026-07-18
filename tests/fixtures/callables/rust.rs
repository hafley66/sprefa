// Fixture: every EMITTED Rust callable kind for examples/callable-coverage.dl.
// Kinds map to call_def.kind: free/nested fn -> function, impl/trait/operator ->
// method, closure -> lambda. See docs/callable-coverage.md.

pub fn free_function(seed: i32) -> i32 {
    // nested named fn -> call_def kind "function"
    fn nested_helper(inner: i32) -> i32 {
        inner + 1
    }
    // closure bound to a variable -> call_def kind "lambda"
    let bound_closure = |factor: i32| factor * 2;
    // unbound closure passed to an adaptor -> call_def kind "lambda"
    let mapped: i32 = [1, 2, 3].iter().map(|value| value + seed).sum();
    nested_helper(bound_closure(mapped))
}

pub async fn async_free(payload: i32) -> i32 {
    payload
}

pub struct Widget {
    size: i32,
}

impl Widget {
    // associated ("static") fn -> method
    pub fn new(size: i32) -> Self {
        Widget { size }
    }
    // instance method -> method
    pub fn area(&self) -> i32 {
        self.size * self.size
    }
}

impl std::ops::Add for Widget {
    type Output = Widget;
    // operator overload -> method
    fn add(self, other: Widget) -> Widget {
        Widget { size: self.size + other.size }
    }
}

pub trait Shape {
    // trait method DECLARATION (no body) -> method
    fn perimeter(&self) -> i32;
    // trait DEFAULT body -> method
    fn describe(&self) -> i32 {
        self.perimeter() * 2
    }
}
