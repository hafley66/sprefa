pub trait Subcommand {
    fn execute(&self);
}

pub fn usage(name: &str) {
    println!("no such subcommand: {name}");
}
