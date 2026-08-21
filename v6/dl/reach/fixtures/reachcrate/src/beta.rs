use crate::plumbing::Subcommand;

pub struct BetaSubcommand;

impl Subcommand for BetaSubcommand {
    fn execute(&self) {
        beta_work();
    }
}

fn beta_work() {
    println!("beta");
}
