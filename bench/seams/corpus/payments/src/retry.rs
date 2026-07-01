pub struct RetryPolicy {
    pub max_attempts: u32,
}

pub fn retry(amount: u64) -> bool {
    amount > 0
}
