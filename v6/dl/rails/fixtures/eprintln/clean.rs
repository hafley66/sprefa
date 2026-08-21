// tracing only: the rail must say nothing about this file.
pub fn narrate(step: &str) {
    tracing::info!(step, "step");
}
