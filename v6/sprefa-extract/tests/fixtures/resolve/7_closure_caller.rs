pub fn run(values: &[u32]) -> Vec<u32> {
    values.iter().map(|value| helper(*value)).collect()
}
