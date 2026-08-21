// TEST FIXTURE. Three subcommands: `alpha` is wired by name, `beta` only
// through a trait object, `gamma` only from a test.

mod alpha;
mod beta;
mod gamma;
mod plumbing;

use plumbing::Subcommand;

fn main() {
    let name = std::env::args().nth(1).unwrap_or_default();
    match name.as_str() {
        "alpha" => alpha::run_alpha(&name),
        "beta" => {
            let chosen: Box<dyn Subcommand> = Box::new(beta::BetaSubcommand);
            chosen.execute();
        }
        _ => plumbing::usage(&name),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn gamma_is_reachable_from_a_test_only() {
        super::gamma::run_gamma();
    }
}
