//! Silent observer that collects the simulated behavior and generates insight from that.
//!
//! Every client has their own `ClientObserver` to collect events locally.
//! When the simulation finishes, these are collected into an overall observer object.

use std::cmp::Ordering;

use chrono::{DateTime, Utc};
use tor_circuit_generator::TorCircuit;
use tordoc::Fingerprint;

use crate::client;
use crate::user::Request;

#[allow(unused_imports)]
use log::{debug, info, trace, warn};

pub(crate) struct SimulationObserver {
    pub circuit_events: Vec<NewCircuitEvent>,
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
                co.events_new_circuit.sort_unstable();
                co.events_new_circuit.into_iter()
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

pub(crate) struct NewCircuitEvent {
    pub time: DateTime<Utc>,
    pub client_id: u64,
    pub circuit: TorCircuit,
    pub port: u16,
}

impl Ord for NewCircuitEvent {
    fn cmp(&self, other: &Self) -> Ordering {
        self.time
            .cmp(&other.time)
            .then(self.client_id.cmp(&other.client_id))
            .then(self.port.cmp(&other.port))
    }
}

impl PartialOrd for NewCircuitEvent {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for NewCircuitEvent {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(&other) == Ordering::Equal
    }
}

impl Eq for NewCircuitEvent {}

/// A snapshot of a ShallowCircuit at some time. In the first place, this
/// doesn't save a (clone) covered_needs reference, but only a snapshot
/// serialized to a String
#[derive(Clone, Debug)]
struct ShallowCircuitSnapshot {
    pub guard: Fingerprint,
    pub middle: Fingerprint,
    pub exit: Fingerprint,
    time: DateTime<Utc>,
    dirty_time: Option<DateTime<Utc>>,
    is_internal: bool,
    is_stable: bool,
    is_fast: bool,
    covered_needs: Vec<String>,
}

impl From<&client::ShallowCircuit> for ShallowCircuitSnapshot {
    fn from(circuit: &client::ShallowCircuit) -> Self {
        ShallowCircuitSnapshot {
            guard: circuit.guard.clone(),
            middle: circuit.middle.clone(),
            exit: circuit.exit.clone(),
            time: circuit.time.clone(),
            dirty_time: circuit.dirty_time.clone(),
            is_internal: circuit.is_internal.clone(),
            is_stable: circuit.is_stable.clone(),
            is_fast: circuit.is_fast.clone(),
            covered_needs: circuit
                .covered_needs
                .iter()
                .map(|x| x.to_string())
                .collect(),
        }
    }
}

struct CircuitUsedEvent {
    time: DateTime<Utc>,
    client_id: u64,
    circuit: ShallowCircuitSnapshot,
    request: Request,
}

impl Ord for CircuitUsedEvent {
    fn cmp(&self, other: &Self) -> Ordering {
        self.time
            .cmp(&other.time)
            .then(self.client_id.cmp(&other.client_id))
            .then(self.request.port.cmp(&other.request.port))
    }
}

impl PartialOrd for CircuitUsedEvent {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for CircuitUsedEvent {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(&other) == Ordering::Equal
    }
}

impl Eq for CircuitUsedEvent {}

struct CircuitClosedEvent {
    time: DateTime<Utc>,
    client_id: u64,
    circuit: ShallowCircuitSnapshot,
    reason: CircuitCloseReason,
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug)]
pub(crate) enum CircuitCloseReason {
    OldDirty,
    OldClean,
    Down,
}

impl Ord for CircuitClosedEvent {
    fn cmp(&self, other: &Self) -> Ordering {
        self.time
            .cmp(&other.time)
            .then(self.client_id.cmp(&other.client_id))
            .then(self.reason.cmp(&other.reason))
    }
}

impl PartialOrd for CircuitClosedEvent {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for CircuitClosedEvent {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(&other) == Ordering::Equal
    }
}

impl Eq for CircuitClosedEvent {}

/// An observer object used by a single client to collect their events (locally).
pub(crate) struct ClientObserver {
    client_id: u64,
    events_new_circuit: Vec<NewCircuitEvent>,
    events_circuit_used: Vec<CircuitUsedEvent>,
    events_circuit_closed: Vec<CircuitClosedEvent>,
}

impl ClientObserver {
    /// Create a new `ClientObserver` with no events.
    pub(crate) fn new(client_id: u64) -> ClientObserver {
        ClientObserver {
            client_id,
            events_new_circuit: Vec::new(),
            events_circuit_used: Vec::new(),
            events_circuit_closed: Vec::new(),
        }
    }

    /// Notify the observer that a new circuit was created
    pub(crate) fn notify_new_circuit(
        &mut self,
        time: DateTime<Utc>,
        circuit: &TorCircuit,
        port: u16,
        reason: String,
    ) {
        trace!(
            "[{}] Client {} built circuit: {} {} {} [reason: {}]",
            &time,
            self.client_id,
            circuit.guard.fingerprint,
            circuit.middle[0].fingerprint,
            circuit.exit.fingerprint,
            reason,
        );

        self.events_new_circuit.push(NewCircuitEvent {
            time,
            client_id: self.client_id,
            circuit: circuit.clone(),
            port,
        });
    }

    /// Notify the observer that a circuit was used to carry a new stream
    pub(crate) fn notify_circuit_used(
        &mut self,
        circuit: &client::ShallowCircuit,
        request: &Request,
    ) {
        trace!(
            "[{}] Client {} uses the following circuit for a stream request: {} {} {}",
            &request.time,
            self.client_id,
            circuit.guard,
            circuit.middle,
            circuit.exit,
        );

        self.events_circuit_used.push(CircuitUsedEvent {
            time: request.time.clone(),
            client_id: self.client_id,
            circuit: circuit.into(),
            request: request.clone(),
        });
    }

    /// Notify the observer that a circuit was closed
    pub(crate) fn notify_circuit_closed(
        &mut self,
        time: &DateTime<Utc>,
        circuit: &client::ShallowCircuit,
        reason: CircuitCloseReason,
    ) {
        trace!(
            "[{}] Client {} closed the following circuit because of \"{:?}\": {} {} {}",
            &time,
            self.client_id,
            reason,
            circuit.guard,
            circuit.middle,
            circuit.exit,
        );

        self.events_circuit_closed.push(CircuitClosedEvent {
            time: time.clone(),
            client_id: self.client_id,
            circuit: circuit.into(),
            reason,
        });
    }

    pub(crate) fn notify_new_need(&mut self, time: &DateTime<Utc>, need: String) {
        trace!("[{}] Client {}: new {}.", &time, self.client_id, need);
    }

    pub(crate) fn notify_need_expired(&mut self, time: &DateTime<Utc>, need: String) {
        trace!("[{}] Client {}: {} expired.", &time, self.client_id, need);
    }

    pub(crate) fn notify_guard_removed_offline(&mut self, time: &DateTime<Utc>, fp: &Fingerprint) {
        trace!(
            "[{}] Client {}: Removed guard {} because it has been offline for too long.",
            &time,
            self.client_id,
            fp,
        );
    }

    pub(crate) fn notify_guard_removed_too_old(&mut self, time: &DateTime<Utc>, fp: &Fingerprint) {
        trace!(
            "[{}] Client {}: Removed guard {} because it is too old.",
            &time,
            self.client_id,
            fp,
        );
    }
}
