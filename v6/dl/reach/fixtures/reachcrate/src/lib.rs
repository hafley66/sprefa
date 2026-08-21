// TEST FIXTURE. The lib root: `library_entry` is the crate's second entry point.

pub mod alpha;
pub mod beta;
pub mod gamma;
pub mod plumbing;

pub fn library_entry() {
    alpha::run_alpha("lib");
}
