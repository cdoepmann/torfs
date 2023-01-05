//! Implementation of simulated clients/users

use std::iter::Peekable;

use crate::observer::ClientObserver;
use crate::user::{Request, UserModel};

use tor_circuit_generator::CircuitGenerator;

use chrono::prelude::*;
#[allow(unused_imports)]
use log::{debug, info, trace, warn};

pub(crate) struct Client<U: UserModel> {
    id: u64,
    observer: ClientObserver,
    user_model: Peekable<U>,
}

impl<U: UserModel> Client<U> {
    /// Construct a new Client
    pub(crate) fn new(id: u64, user_model: U) -> Client<U> {
        Client {
            id,
            observer: ClientObserver::new(id),
            user_model: user_model.peekable(),
        }
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
        loop {
            // Look at the next request, but do not consume it yet
            let next = self.user_model.peek();

            let request = match next {
                Some(x) if (&x.time >= epoch_start && &x.time < epoch_end) => {
                    // use/consume this request element and advance the user model
                    self.user_model.next().unwrap() // cannot fail as peek() was Some(_)
                }
                _ => break,
            };

            self.generate_circuit(circuit_generator, request)?;
        }

        Ok(())
    }
    /// Generate a new circuit at the "current" time (`self.next_circuit_time`)
    fn generate_circuit(
        &mut self,
        circuit_generator: &CircuitGenerator,
        request: Request,
    ) -> anyhow::Result<()> {
        let circuit = circuit_generator
            .build_circuit(3, request.port)
            .map_err(|e| anyhow::anyhow!(format!("{:?}", e)))?;

        self.observer
            .notify_circuit(request.time, circuit, request.port);

        Ok(())
    }

    /// Get the client's ID
    pub(crate) fn get_id(&self) -> u64 {
        self.id
    }

    /// Finish this client and return its observer
    pub(crate) fn into_observer(self) -> ClientObserver {
        self.observer
    }
}
