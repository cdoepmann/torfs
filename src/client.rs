//! Implementation of simulated clients/users

use std::iter::Peekable;

use crate::guard::GuardHandling;
use crate::needs::{NeedHandle, NeedsContainer};
use crate::observer::{CircuitCloseReason, ClientObserver, ExitFingerprintSerializer};
use crate::trace::MemoryCsvWriter;
use crate::user::{Request, UserModel};
use crate::utils::*;

use tor_circuit_generator::CircuitGenerator;
use tordoc::{consensus::Flag, Fingerprint};

use chrono::prelude::*;
use chrono::Duration;
#[allow(unused_imports)]
use log::{debug, info, trace, warn};

use lazy_static::lazy_static;

lazy_static! {
    static ref MAX_CIRCUIT_DIRTINESS: Duration = Duration::minutes(10);
    static ref CIRCUIT_IDLE_TIMEOUT: Duration = Duration::minutes(60);
    static ref LONG_LIVED_PORTS: Vec<u16> =
        [21, 22, 706, 1863, 5050, 5190, 5222, 5223, 6523, 6667, 6697, 8300]
            .into_iter()
            .collect();
}

/// A simulated Tor client.
///
/// This implements Tor's behavior of handling circuits, streams, etc. for
/// handling the requests made by a user, as modelled by a given `UserModel`.
pub(crate) struct Client<U: UserModel> {
    id: u64,
    observer: ClientObserver,
    user_model: Peekable<U>,
    circuit_manager: CircuitManager,
}

impl<U: UserModel> Client<U> {
    /// Construct a new Client
    pub(crate) fn new(id: u64, user_model: U) -> Client<U> {
        Client {
            id,
            observer: ClientObserver::new(id),
            user_model: user_model.peekable(),
            circuit_manager: CircuitManager::new(),
        }
    }

    /// Called from outside when the simulation enters a new epoch,
    /// with a new consensus being available.
    /// TODO: Clarify what an epoch is
    pub(crate) fn handle_new_epoch(
        &mut self,
        epoch_start: &DateTime<Utc>,
        epoch_end: &DateTime<Utc>,
        circuit_generator: &CircuitGenerator,
        csv_writer: &mut MemoryCsvWriter,
        exit_ids: &ExitFingerprintSerializer,
    ) -> anyhow::Result<()> {
        // TODO: period_client_update
        // TODO: update guard set

        // TODO: cover uncovered ports while fewer than
        // TODO: TorOptions.max_unused_open_circuits clean

        // Do time-based maintaining at least once per epoch
        self.circuit_manager.timed_client_updates(
            &epoch_start,
            circuit_generator,
            &mut self.observer,
        )?;

        // construct all the circuits in this time frame
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

            // Do time-based maintaining. TorPS does this once per minute but it
            // **should** be ok to do this only when actually needed.
            // TODO: Maybe this is not true anymore when we introduce need covering
            self.circuit_manager.timed_client_updates(
                &request.time,
                circuit_generator,
                &mut self.observer,
            )?;

            self.circuit_manager.handle_request(
                request,
                circuit_generator,
                &mut self.observer,
                csv_writer,
                exit_ids,
            )?;
        }

        Ok(())
    }

    /// Get the client's ID
    #[allow(unused)]
    pub(crate) fn get_id(&self) -> u64 {
        self.id
    }

    /// Finish this client and return its observer
    pub(crate) fn into_observer(self) -> ClientObserver {
        self.observer
    }
}

/// A Tor circuit defined only by its relays' fingerprint, not tied to a consensus.
///
/// We use this to persist circuits over different consensuses, without needing
/// to keep track of the consensus itself. This also makes sure we do not
/// accidentally use stale consensus information of the relays at some point.
#[derive(Debug)]
pub(crate) struct ShallowCircuit {
    pub(crate) guard: Fingerprint,
    pub(crate) middle: Fingerprint,
    pub(crate) exit: Fingerprint,
    // TODO: do we need to remember exit policy, ports, etc.?
    /// Time when this circuit was created
    pub(crate) time: DateTime<Utc>,
    /// Time the circuit became "dirty". If this is None, circuit is clean.
    pub(crate) dirty_time: Option<DateTime<Utc>>,
    /// Is this circuit intended for name resolution and onion services only?
    pub(crate) is_internal: bool,
    /// Is this circuit expected to have only stable relays?
    pub(crate) is_stable: bool,
    /// Is this circuit expected to have only fast relays?
    pub(crate) is_fast: bool,
    /// Port needs that are covered by this circuit
    pub(crate) covered_needs: Vec<NeedHandle>,
}

impl ShallowCircuit {
    /// Construct from a circuit as generated by the CircuitGenerator
    fn from_generated_circuit(
        circgen_circuit: tor_circuit_generator::TorCircuit,
        stable: bool,
        fast: bool,
        time: DateTime<Utc>,
        dirty_time: Option<DateTime<Utc>>,
        covered_need: Option<NeedHandle>,
    ) -> ShallowCircuit {
        if circgen_circuit.middle.len() != 1 {
            panic!("We only support 3-hop circuits at the moment");
        }
        ShallowCircuit {
            guard: circgen_circuit.guard.fingerprint.clone(),
            middle: circgen_circuit.middle[0].fingerprint.clone(),
            exit: circgen_circuit.exit.fingerprint.clone(),
            time,
            dirty_time,
            is_internal: false,
            is_stable: stable,
            is_fast: fast,
            covered_needs: covered_need.into_iter().collect(),
        }
    }

    /// Check if this circuit can accommodate a given stream request.
    ///
    /// # Panics
    ///
    /// _May_ panic if the circuit's relays aren't part of the consensus.
    fn supports_stream(&self, request: &Request, circgen: &CircuitGenerator) -> bool {
        if self.is_internal {
            return false;
        }

        if !self.is_stable && LONG_LIVED_PORTS.contains(&request.port) {
            return false;
        }

        let exit = circgen.lookup_relay(&self.exit).unwrap();
        if !(*exit).exit_policy.allows_port(request.port) {
            return false;
        }

        true
    }
}

/// A container for circuits currently maintained by the client
struct CircuitManager {
    /// Circuits that have already been constructed (both, clean & dirty)
    circuits: Vec<ShallowCircuit>,
    /// Ports that need to be covered by circuits proactively
    port_needs: NeedsContainer,
    /// Last time the time-based update was triggered
    last_triggered: Option<DateTime<Utc>>,
    /// Handler for this client's guard set
    guards: GuardHandling,
}

impl CircuitManager {
    /// Construct a new circuit manager from scratch for a new client
    fn new() -> CircuitManager {
        CircuitManager {
            circuits: Vec::new(),
            port_needs: NeedsContainer::new(),
            last_triggered: None,
            guards: GuardHandling::new(),
        }
    }

    /// When entering a new epoch, carry out the housekeeping of currently
    /// maintained circuits, etc.
    fn timed_client_updates(
        &mut self,
        time: &DateTime<Utc>,
        circgen: &CircuitGenerator,
        observer: &mut ClientObserver,
    ) -> anyhow::Result<()> {
        // When being used for the first time, set an initial port need.
        if let None = self.last_triggered {
            let need_string = self.port_needs.add_need(80, time, true, false);
            observer.notify_new_need(time, need_string);
        };
        self.last_triggered = Some(time.clone());

        // expire port needs
        self.port_needs
            .remove_expired(time, |need| observer.notify_need_expired(time, need));

        // remove old dirty circuits
        self.circuits.retain_or_else(
            |circuit| {
                if let Some(dirty_time) = circuit.dirty_time {
                    // dirty circuits
                    return dirty_time + *MAX_CIRCUIT_DIRTINESS >= *time;
                } else {
                    true
                }
            },
            |circuit| observer.notify_circuit_closed(time, circuit, CircuitCloseReason::OldDirty),
        );

        // remove old clean circuits
        // Note that, when the circuits are destroyed, the `NeedHandle`s are dropped as well,
        // so the "covered" count of each need is decremented as well.
        self.circuits.retain_or_else(
            |circuit| {
                if let None = circuit.dirty_time {
                    // clean circuits
                    return circuit.time + *CIRCUIT_IDLE_TIMEOUT >= *time;
                } else {
                    true
                }
            },
            |circuit| observer.notify_circuit_closed(time, circuit, CircuitCloseReason::OldClean),
        );

        // remove circuits whose relays have gone missing
        self.circuits.retain_or_else(
            |circuit| {
                for relay in [&circuit.guard, &circuit.middle, &circuit.exit] {
                    match circgen.lookup_relay(relay) {
                        None => {
                            return false;
                        }
                        Some(x) => {
                            // TODO: check old hibernating flag from descriptor
                            if !(x.flags.contains(&Flag::Running))
                                || !(x.flags.contains(&Flag::Valid))
                            {
                                return false;
                            }
                        }
                    }
                }
                true
            },
            |circuit| observer.notify_circuit_closed(time, circuit, CircuitCloseReason::Down),
        );

        // Trigger the guard handling
        self.guards.timed_updates(time, circgen, observer);

        // Cover uncovered port needs
        while let Some(need_handle) = self.port_needs.get_uncovered_need() {
            // build a suitable circuit for this need

            // these unwraps never fail as we have just got an existing need
            let port = need_handle.get_port().unwrap();
            let need_stable = need_handle.get_stable().unwrap();
            let need_fast = need_handle.get_fast().unwrap();

            let guard = self.guards.get_guard_for_circuit(time, circgen);

            let circuit = circgen
                .build_circuit_with_flags_and_guard(3, port, Some(&guard), need_fast, need_stable)
                .map_err(|e| anyhow::anyhow!(format!("{:?}", e)))?;
            observer.notify_new_circuit(
                time.clone(),
                &circuit,
                port,
                format!("to cover need {}", need_handle.to_string()),
            );
            self.circuits.push(ShallowCircuit::from_generated_circuit(
                circuit,
                need_stable,
                need_fast,
                time.clone(),
                None,              // circuit is clean
                Some(need_handle), // this is to cover a port need
            ));
        }

        Ok(())
    }

    /// Try to accommodate a stream request, using the existing circuits etc.
    fn handle_request(
        &mut self,
        request: Request,
        circgen: &CircuitGenerator,
        observer: &mut ClientObserver,
        csv_writer: &mut MemoryCsvWriter,
        exit_ids: &ExitFingerprintSerializer,
    ) -> anyhow::Result<()> {
        // Unfortunately, we have to split the following two criteria into
        // separate functions to work around one of the current
        // limitations of the borrow checker.
        //
        // See https://blog.rust-lang.org/2022/08/05/nll-by-default.html#looking-forward-what-can-we-expect-for-the-borrow-checker-of-the-future
        // This limitation keeps us from writing the two mutable loops (with early return)
        // in a single function.
        let mut request = request;

        // first check if a dirty circuit is usable
        let mut chosen_circ = self.get_suitable_dirty_circuit(&request, circgen);

        // check if a clean circuit is usable
        if chosen_circ.is_none() {
            chosen_circ = self.get_suitable_clean_circuit(&request, circgen);
        }

        // otherwise, make a new circuit
        if chosen_circ.is_none() {
            let need_stable = LONG_LIVED_PORTS.contains(&request.port);
            let need_fast = true;

            let guard = self.guards.get_guard_for_circuit(&request.time, circgen);

            let circuit = circgen
                .build_circuit_with_flags_and_guard(
                    3,
                    request.port,
                    Some(&guard),
                    need_fast,
                    need_stable,
                )
                .map_err(|e| anyhow::anyhow!(format!("{:?}", e)))?;
            observer.notify_new_circuit(
                request.time,
                &circuit,
                request.port,
                format!("to fulfil stream request {:?}", &request),
            );
            self.circuits.push(ShallowCircuit::from_generated_circuit(
                circuit,
                need_stable,
                need_fast,
                request.time.clone(),
                Some(request.time.clone()), // circuit is dirty
                None,                       // this is not to cover a port need
            ));
            chosen_circ = self.circuits.last();
        }

        // We now have a ready-to-use circuit to handle the request
        let chosen_circ = chosen_circ.unwrap(); // cannot fail as the if block adds an element if there was none

        // Handle the generated packet trace.
        // Note that the user model makes sure that it waits with further requests
        // until all the response packets are over.

        // We "move" the packet trace out of the request object as it is not needed
        // again later on and we want to avoid cloning it.
        let packet_timestamps = std::mem::take(&mut request.packet_timestamps);
        observer.notify_circuit_used(
            chosen_circ,
            &request,
            packet_timestamps,
            csv_writer,
            exit_ids,
        )?;

        let guard_fingerprint = chosen_circ.guard.clone();
        self.guards
            .mark_as_confirmed(&guard_fingerprint, &request.time);

        // Now that we used a circuit to meet a stream request, remember the need for this port
        // so we build appropriate circuits in advance to future requests to the same port.
        // (In TorPS, this is `stream_update_port_needs()`.)
        {
            let port = request.port;
            let fast = true;
            let stable = LONG_LIVED_PORTS.contains(&request.port);

            // add need or update expiration
            let need_string = self.port_needs.add_need(port, &request.time, fast, stable);
            observer.notify_new_need(&request.time, need_string);

            // check if this need can be covered by an existing, clean circuit
            let need_handle = self.port_needs.cover_need_if_necessary(port);

            if let Some(need_handle) = need_handle {
                // the need is now covered because we have created a handle, but
                // if we cannot actually cover the need, the handle will be dropped
                // and the "covered" counter is reset

                'circuit_loop: for circuit in
                    self.circuits.iter_mut().filter(|c| c.dirty_time.is_none())
                {
                    // ignore this circuit if it covers the port already
                    for existing_cover in circuit.covered_needs.iter() {
                        if let Some(existing_port) = existing_cover.get_port() {
                            if existing_port == port {
                                continue 'circuit_loop;
                            }
                        }
                    }

                    if need_handle.can_be_covered_by_circuit(circuit, circgen) {
                        // cover this need
                        circuit.covered_needs.push(need_handle);
                        break;
                    }
                }
            }
        }

        Ok(())
    }

    /// Select an existing **dirty** circuit that is suitable for handling a given stream request
    fn get_suitable_dirty_circuit(
        &mut self,
        request: &Request,
        circgen: &CircuitGenerator,
    ) -> Option<&ShallowCircuit> {
        for circ in self.circuits.iter_mut() {
            if let Some(dirty_time) = circ.dirty_time {
                if request.time < dirty_time + *MAX_CIRCUIT_DIRTINESS
                    && circ.supports_stream(&request, circgen)
                {
                    return Some(circ);
                }
            }
        }
        None
    }

    /// Select an existing **clean** circuit that is suitable for handling a given stream request
    fn get_suitable_clean_circuit(
        &mut self,
        request: &Request,
        circgen: &CircuitGenerator,
    ) -> Option<&ShallowCircuit> {
        for circ in self.circuits.iter_mut() {
            if circ.dirty_time.is_none() {
                if circ.supports_stream(&request, circgen) {
                    // TODO make sure we check somewhere else circuit_idle_timeout
                    // TODO Do we maybe have to reorder the circuits? TorPS uses .appendleft()

                    // make this circuit dirty
                    circ.dirty_time = Some(request.time.clone());

                    // As this circuit is now in use, it doesn't cover the port needs
                    // it may have covered before (not spare anymore). We thus
                    // need to remove its covered `NeedHandle`s, which will
                    // pick up the neccessity for a new need cover.
                    circ.covered_needs.clear();

                    return Some(circ);
                }
            }
        }

        None
    }
}
