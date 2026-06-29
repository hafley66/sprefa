// Rust core. The shared `PaymentClient` name also appears in the TS gateway and
// the Kotlin mobile client (the cross-language seam).
pub struct PaymentClient {
    pub policy: RetryPolicy,
}

impl PaymentClient {
    pub fn charge(&self, amount: u64) -> bool {
        retry(amount)
    }
}
