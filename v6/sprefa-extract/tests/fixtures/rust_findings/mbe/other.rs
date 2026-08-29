macro_rules! shout {
    () => { println!("hi"); };
}
pub fn shout() {
    shout!();
}
