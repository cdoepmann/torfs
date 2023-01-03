//! Implementation of simulated clients/users

use crate::seeded_rand::get_rng;

use tor_circuit_generator::CircuitGenerator;

use chrono::prelude::*;
#[allow(unused_imports)]
use log::{debug, info, trace, warn};
use rand::Rng;

pub(crate) struct Client {
    id: u64,
    next_circuit_time: DateTime<Utc>,
}

impl Client {
    /// Construct a new Client
    pub(crate) fn new(id: u64, start_time: &DateTime<Utc>) -> Client {
        Client {
            id,
            next_circuit_time: *start_time + Self::sample_intercircuit_delay(),
        }
    }

    /// Get the next time to build a circuit TODO based on a user model
    fn sample_intercircuit_delay() -> chrono::Duration {
        // TODO
        let mut rng = get_rng();

        // three days for now
        chrono::Duration::seconds(rng.gen_range(0..=60 * 60 * 24 * 3))
    }

    /// Called from outside when the simulation enters a new epoch,
    /// with a new consensus being available.
    /// TODO: Clarify what an epoch is
    pub(crate) fn trigger_new_epoch(
        &mut self,
        epoch_start: &DateTime<Utc>,
        epoch_end: &DateTime<Utc>,
        circuit_generator: &CircuitGenerator,
    ) -> anyhow::Result<()> {
        // TODO use user model to decide whether to build a circuit and what kind

        // construct all the circuits in this time frame
        while epoch_start <= &self.next_circuit_time && &self.next_circuit_time <= epoch_end {
            self.generate_circuit(circuit_generator, &self.next_circuit_time)?;

            self.next_circuit_time += Self::sample_intercircuit_delay()
        }

        Ok(())
    }

    fn generate_circuit(
        &self,
        circuit_generator: &CircuitGenerator,
        timestamp: &DateTime<Utc>,
    ) -> anyhow::Result<()> {
        // TODO: port handling
        let circuit = circuit_generator
            .build_circuit(3, 443)
            .map_err(|e| anyhow::anyhow!(format!("{:?}", e)))?;

        trace!(
            "[{}] Client {} built circuit: {} {} {}",
            timestamp,
            self.get_id(),
            circuit.guard.fingerprint,
            circuit.middle[0].fingerprint,
            circuit.exit.fingerprint,
        );

        Ok(())
    }

    /// Get the client's ID
    pub(crate) fn get_id(&self) -> u64 {
        self.id
    }
}
