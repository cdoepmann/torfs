//! Helpers to manage the relationship between port needs and circuits.
//!
//! Port needs are collected from past user behavior to predict likely upcoming
//! user requests. For predicted ports, suitable circuits are built proactively,
//! before they are actually needed.
//!
//! - interior mut
//! - weak
//! - handle

use crate::client::ShallowCircuit;
use crate::utils::*;

use std::collections::hash_map::Entry::{Occupied, Vacant};
use std::fmt::Display;
use std::sync::RwLock;
use std::sync::{Arc, Weak};

use chrono::prelude::*;
use chrono::Duration;
use lazy_static::lazy_static;
use seeded_rand::RHashMap;
use tor_circuit_generator::CircuitGenerator;

lazy_static! {
    // min coverage given with "#define MIN_CIRCUITS_HANDLING_STREAM 2" in or.h
    static ref PORT_NEED_COVER_NUM: usize = 2;
    // #define PREDICTED_CIRCS_RELEVANCE_TIME 60*60" in rephist.c
    // need expires after an hour
    static ref PORT_NEED_LIFETIME: Duration = Duration::minutes(60);
}

/// A container for all the current port needs of the client
///
/// This container _owns_ the port needs.
pub(crate) struct NeedsContainer {
    needs: RHashMap<u16, Arc<Need>>,
}

impl NeedsContainer {
    /// TODO
    pub fn new() -> NeedsContainer {
        NeedsContainer {
            needs: RHashMap::default(),
        }
    }

    /// If a need for port `port` exists **and needs cover**, return a [NeedHandle] for it.
    ///
    /// This triggers the book-keeping, so the Need knows that it is covered.
    /// If, however, the handle is dropped at some point, the need will be
    /// "uncovered" automatically.
    pub fn cover_need_if_necessary(&mut self, port: u16) -> Option<NeedHandle> {
        self.needs
            .get(&port)
            .filter(|need| need.needs_cover())
            .map(|need| NeedHandle::from_need(need))
    }

    /// If there is a need that isn't sufficiently covered, return a handle for it.
    ///
    /// The need will be counted as covered as soon as this function returns the
    /// handle. If, however, the handle is dropped at some point, it will be
    /// "uncovered" automatically.
    ///
    /// If this function returns `Some(x)`, then `x` is guaranteed to be a need
    /// that exists at the moment ([NeedHandle::exists] returns true).
    pub fn get_uncovered_need(&self) -> Option<NeedHandle> {
        for (_port, need) in self.needs.iter() {
            if need.needs_cover() {
                return Some(NeedHandle::from_need(need));
            }
        }
        None
    }

    /// Remove all the needs that have expired by `now`, and call `handler`
    /// with a string representation of each of them.
    pub fn remove_expired(&mut self, now: &DateTime<Utc>, handler: impl FnMut(String)) {
        let mut handler = handler;

        self.needs.retain_or_else(
            |_port, need| !need.has_expired(now),
            |_port, need| {
                handler(need.to_string());
            },
        );
    }

    /// Add a need to be covered by circuits and return a `String` representation of it.
    ///
    /// There can only be one need per port. If one already exists for `port`,
    /// then it isn't re-inserted. In particular, the `fast` and `stable` flags aren't
    /// updated. If the need has expired, though, the expiration date is updated.
    /// This is in line with TorPS's `stream_update_port_needs` behavior.
    pub fn add_need(&mut self, port: u16, now: &DateTime<Utc>, fast: bool, stable: bool) -> String {
        match self.needs.entry(port) {
            Occupied(mut entry) => {
                let need = entry.get_mut();
                if need.has_expired(now) {
                    need.reset_expiration(now);
                }
                need.to_string()
            }
            Vacant(entry) => {
                let need = Arc::new(Need::new(port, now, fast, stable));
                entry.insert(need).to_string()
            }
        }
    }
}

/// TODO
///
/// TODO Drop
/// **WARNING**: Be sure we do not implement [Clone] for this! Otherwise,
/// cloning the handle and dropping them later could lead to negative "covered"
/// counts! Cloning would be the equivalent of having the need covered by yet
/// another circuit, which is a semantic we do not want here.
#[derive(Debug)]
pub(crate) struct NeedHandle {
    need: Weak<Need>,
}

impl NeedHandle {
    /// TODO
    fn from_need(need: &Arc<Need>) -> NeedHandle {
        // register with the need that we now create a handle to it
        need.increment_cover_count();

        NeedHandle {
            need: Arc::downgrade(need),
        }
    }

    /// Returns `true` if the handle points to a need that still exists.
    #[allow(unused)]
    pub fn exists(&self) -> bool {
        self.need.upgrade().is_some()
    }

    /// Reset the need's expiration time to count from `now` on, if the need still exists.
    ///
    /// Does nothing if the need has already gone.
    #[allow(unused)]
    pub fn reset_expiration(&self, now: &DateTime<Utc>) {
        if let Some(need) = self.need.upgrade() {
            need.reset_expiration(now)
        }
    }

    /// Get the needed port, if the need still exists.
    pub fn get_port(&self) -> Option<u16> {
        self.need.upgrade().map(|need| need.port)
    }

    /// Get the needed fast flag, if the need still exists.
    pub fn get_fast(&self) -> Option<bool> {
        self.need.upgrade().map(|need| need.fast)
    }

    /// Get the needed stable flag, if the need still exists.
    pub fn get_stable(&self) -> Option<bool> {
        self.need.upgrade().map(|need| need.stable)
    }

    /// Return `true` if this need still exists and can be covered by a given circuit.
    pub fn can_be_covered_by_circuit(
        &self,
        circuit: &ShallowCircuit,
        circgen: &CircuitGenerator,
    ) -> bool {
        match self.need.upgrade() {
            None => {
                return false;
            }
            Some(need) => {
                if need.fast && !circuit.is_fast {
                    return false;
                }

                if need.stable && !circuit.is_stable {
                    return false;
                }

                let exit = circgen.lookup_relay(&circuit.exit).unwrap();
                if !(*exit).exit_policy.allows_port(need.port) {
                    return false;
                }

                return true;
            }
        }
    }
}

impl Drop for NeedHandle {
    fn drop(&mut self) {
        // When this handle is dropped (the circuit doesn't cover the need anymore),
        // reduce the "covered" counter.
        if let Some(need) = self.need.upgrade() {
            need.decrement_cover_count();
        }
    }
}

impl Display for NeedHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self.need.upgrade() {
            Some(need) => {
                write!(f, "{:?}", need)
            }
            None => {
                write!(f, "(need doesn't exist anymore)")
            }
        }
    }
}

#[derive(Debug)]
struct Need {
    port: u16,
    expires: RwLock<DateTime<Utc>>,
    fast: bool,
    stable: bool,
    /// Number of circuits that have a handle to this need, "covering" it
    covered: RwLock<usize>,
}

impl Display for Need {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl Need {
    fn new(port: u16, now: &DateTime<Utc>, fast: bool, stable: bool) -> Need {
        Need {
            port,
            expires: RwLock::new(*now + *PORT_NEED_LIFETIME),
            fast,
            stable,
            covered: RwLock::new(0),
        }
    }

    /// TODO
    ///
    /// TODO Panics
    fn decrement_cover_count(&self) {
        let mut covered = self.covered.write().unwrap();
        let old_count = *covered;
        assert!(old_count > 0);
        *covered = old_count - 1;
    }

    /// TODO
    ///
    /// TODO Panics
    fn increment_cover_count(&self) {
        let mut covered = self.covered.write().unwrap();
        let old_count = *covered;
        *covered = old_count + 1;
    }

    /// Returns `true` if the need is _not_ sufficiently covered by circuits at the moment
    fn needs_cover(&self) -> bool {
        *(self.covered.read().unwrap()) < *PORT_NEED_COVER_NUM
    }

    /// TODO
    ///
    /// TODO Panics
    fn reset_expiration(&self, now: &DateTime<Utc>) {
        *self.expires.write().unwrap() = *now + *PORT_NEED_LIFETIME;
    }

    /// TODO
    ///
    /// TODO Panics
    fn has_expired(&self, now: &DateTime<Utc>) -> bool {
        *self.expires.write().unwrap() <= *now
    }
}
