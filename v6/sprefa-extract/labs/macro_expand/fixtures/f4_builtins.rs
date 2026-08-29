fn go() {
    let v = vec![1, 2, 3];
    let s = format!("{} {}", v.len(), count(&v));
    assert!(s.len() > 0);
}
fn count(v: &Vec<i32>) -> usize { v.len() }
