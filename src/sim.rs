//! The (abstract) simulator and simulation environment

use anyhow;
#[allow(unused_imports)]
use log::{debug, info, trace, warn};

use tor_circuit_generator::CircuitGenerator;

use crate::cli::Cli;
use crate::client::Client;
use crate::input::{ConsensusHandle, TorArchive};

pub(crate) struct Simulator {
    cli: Cli,
}

impl Simulator {
    /// Construct a new simulator environment
    pub(crate) fn new(cli: Cli) -> Simulator {
        Simulator { cli }
    }

    /// Run the simulation
    pub(crate) fn run(self) -> anyhow::Result<()> {
        info!("Finding consensuses");
        let archive = TorArchive::new(self.cli.tor_data)?;
        let consensus_handles = archive.find_consensuses(self.cli.from, self.cli.to)?;
        info!("Found {} consensuses.", consensus_handles.len());

        info!("Creating {} clients", self.cli.clients);
        let clients: Vec<_> = (0..self.cli.clients).map(|id| Client::new(id)).collect();

        for handle in consensus_handles {
            dbg!(&handle);
            let (consensus, descriptors) = handle.load()?;
            let circgen = CircuitGenerator::new(&consensus, descriptors, vec![443, 80, 22]);
            circgen
                .build_circuit(3, 443)
                .map_err(|_| anyhow::anyhow!("error building circuit"))?;
        }
        Ok(())
    }
}
