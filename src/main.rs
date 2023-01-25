use anyhow;
use env_logger;

#[allow(unused_imports)]
use log::{debug, info, trace, warn};

mod cli;
use cli::Cli;
mod client;
mod input;
mod observer;
mod sim;
mod user;
use sim::Simulator;
mod seeded_rand;
mod utils;

fn main() -> anyhow::Result<()> {
    // Initialize logging system
    env_logger::init();

    let cli = Cli::parse();

    seeded_rand::set_seed(if cli.seed == 0 {
        let new_seed = seeded_rand::generate_random_seed();
        info!(
            "No seed was given. Call with \"--seed {}\" to reproduce this run.",
            new_seed
        );
        new_seed
    } else {
        cli.seed
    });

    let simulator = Simulator::new(cli);
    simulator.run()?;

    Ok(())
}
