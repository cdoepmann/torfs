//! Silent observer that collects the simulated behavior and generates insight from that.
//!
//! Every client has their own `ClientObserver` to collect events locally.
//! When the simulation finishes, these are collected into an overall observer object.

use std::cmp::Ordering;

use chrono::{DateTime, Utc};
use tor_circuit_generator::TorCircuit;

#[allow(unused_imports)]
use log::{debug, info, trace, warn};

pub(crate) struct SimulationObserver {
    pub circuit_events: Vec<CircuitEvent>,
}

impl SimulationObserver {
    /// Construct a new `SimulationObserver` from the finished `ClientObserver`s.
    pub(crate) fn from_clients(
        client_observers: impl IntoIterator<Item = ClientObserver>,
    ) -> SimulationObserver {
        // merge the sorted event vectors into a single one
        use itertools::Itertools;
        let merged_iterator = client_observers
            .into_iter()
            .map(|mut co| {
                co.circuit_events.sort_unstable();
                co.circuit_events.into_iter()
            })
            .kmerge();

        SimulationObserver {
            circuit_events: merged_iterator.collect(),
        }
    }

    pub(crate) fn print(&self) {
        for circuit_event in self.circuit_events.iter() {
            println!(
                "[{}] Client {} built circuit: {} {} {}",
                &circuit_event.time,
                &circuit_event.client_id,
                circuit_event.circuit.guard.fingerprint,
                circuit_event.circuit.middle[0].fingerprint,
                circuit_event.circuit.exit.fingerprint,
            );
        }
    }
}

pub(crate) struct CircuitEvent {
    pub time: DateTime<Utc>,
    pub client_id: u64,
    pub circuit: TorCircuit,
    pub port: u16,
}

impl Ord for CircuitEvent {
    fn cmp(&self, other: &Self) -> Ordering {
        self.time
            .cmp(&other.time)
            .then(self.client_id.cmp(&other.client_id))
            .then(self.port.cmp(&other.port))
    }
}

impl PartialOrd for CircuitEvent {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for CircuitEvent {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(&other) == Ordering::Equal
    }
}

impl Eq for CircuitEvent {}

/// An observer object used by a single client to collect their events (locally).
pub(crate) struct ClientObserver {
    client_id: u64,
    circuit_events: Vec<CircuitEvent>,
}

impl ClientObserver {
    /// Create a new `ClientObserver` with no events.
    pub(crate) fn new(client_id: u64) -> ClientObserver {
        ClientObserver {
            client_id,
            circuit_events: Vec::new(),
        }
    }

    /// Notify the observer of a new circuit event
    pub(crate) fn notify_circuit(&mut self, time: DateTime<Utc>, circuit: TorCircuit, port: u16) {
        trace!(
            "[{}] Client {} built circuit: {} {} {}",
            &time,
            self.client_id,
            circuit.guard.fingerprint,
            circuit.middle[0].fingerprint,
            circuit.exit.fingerprint,
        );

        self.circuit_events.push(CircuitEvent {
            time,
            client_id: self.client_id,
            circuit,
            port,
        });
    }
}
