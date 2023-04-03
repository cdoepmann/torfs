//! The (abstract) simulator and simulation environment

use anyhow;
use anyhow::Context;
#[allow(unused_imports)]
use log::{debug, info, trace, warn};

use tor_circuit_generator::CircuitGenerator;

use crate::cli::Cli;
use crate::client::Client;
use crate::input::TorArchive;
use crate::observer::SimulationObserver;
use crate::user::DummyUser;

pub(crate) struct Simulator {
    cli: Cli,
}

impl Simulator {
    /// Construct a new simulator environment
    pub(crate) fn new(cli: Cli) -> Simulator {
        Simulator { cli }
    }

    /// Run the simulation
    pub(crate) fn run(self) -> anyhow::Result<SimulationObserver> {
        info!("Finding consensuses");
        let archive = TorArchive::new(self.cli.tor_data)?;
        let consensus_handles = archive.find_consensuses(&self.cli.from, &self.cli.to)?;
        info!("Found {} consensuses.", consensus_handles.len());

        // parse simulation time range into DateTime objects
        let start_time = self.cli.from.first_datetime();
        let end_time = self.cli.to.last_datetime();

        if end_time <= start_time {
            anyhow::bail!(
                "The simulation start time (given: {}) must be before the end time (given: {})",
                start_time,
                end_time
            );
        }

        info!("Creating {} clients", self.cli.clients);
        let mut clients: Vec<_> = (0..self.cli.clients)
            .map(|id| Client::new(id, DummyUser::new(start_time.clone())))
            .collect();

        // Iterate over the consensus handles for the simulation duration.
        // We make this peekable so we can see when the next consensus period starts.
        // Each item of this iterator is of type anyhow::Result<...>, so we keep
        // any errors that occured.
        let mut consensus_iterator = consensus_handles
            .into_iter()
            .map(|handle| -> anyhow::Result<_> {
                let (consensus, descriptors) = handle.load()?;
                anyhow::Ok((consensus, descriptors))
            })
            .peekable();

        while let Some(consensus_result) = consensus_iterator.next() {
            // we cannot use a for loop here because then we couldn't call .peek() on the iterator

            let (consensus, descriptors) = consensus_result?;

            let range_start = &consensus
                .valid_after
                .context("consensus missing valid_after")?;
            trace!(
                "Entering simulation epoch with consensus from {}",
                &range_start
            );

            let range_end = match consensus_iterator.peek() {
                Some(Ok((next_consensus, _))) => {
                    // If there is a next consensus, use its start time as our end time.
                    // This will ignore errors in the next consensus for now (we only
                    // have a reference, so cannot return them easily), but these
                    // will be handled in the next iteration
                    next_consensus
                        .valid_after
                        .context("consensus missing valid_after")?
                }
                _ => {
                    // Otherwise, use this consensus's valid_until
                    // TODO
                    *range_start + chrono::Duration::hours(3)
                }
            };
            let range_end = std::cmp::min(range_end, end_time);

            let circgen = CircuitGenerator::new(&consensus, descriptors, vec![443, 80, 22])
                .map_err(|e| anyhow::anyhow!(e))
                .context("Failed to construct circuit generator")?;

            // Trigger clients
            for client in clients.iter_mut() {
                client.handle_new_epoch(range_start, &range_end, &circgen)?;
            }
        }

        // Wrap up the simulation
        let observer =
            SimulationObserver::from_clients(clients.into_iter().map(|c| c.into_observer()));
        observer.print();

        Ok(observer)
    }
}
