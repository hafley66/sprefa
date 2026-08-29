use serde::Serialize;
#[derive(Serialize)]
struct Msg { id: u32 }
fn send(m: Msg) {}
fn main() { send(Msg { id: 1 }); }
