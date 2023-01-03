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
use sim::Simulator;

fn main() -> anyhow::Result<()> {
    // Initialize logging system
    env_logger::init();

    let cli = Cli::parse();

    let simulator = Simulator::new(cli);
    simulator.run()?;

    Ok(())
}
