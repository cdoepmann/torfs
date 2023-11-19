//! Guard handling for simulated Tor clients

// Deviations from spec:
//
// We do not model reachability of relays. Relays are assumed to be
// reachable if they are in the consensus and have the Running flag.
// We wouln'd have a way to model reachability otherwise.
// This also means FILTERED_GUARDS and USABLE_FILTERED_GUARDS are the
// same for us. We therefore simply use FILTERED_GUARDS whenever
// USABLE_FILTERED_GUARDS is required.

use std::borrow::Borrow;
use std::cmp::min;

use crate::observer::ClientObserver;

use chrono::prelude::*;
use chrono::Duration;
use lazy_static::lazy_static;
use rand::Rng;
use seeded_rand::{get_rng, RHashSet};
use tor_circuit_generator::CircuitGenerator;
use tordoc::{consensus::Flag, Fingerprint};

lazy_static! {
    static ref GUARD_LIFETIME: Duration = Duration::days(120);
    static ref REMOVE_UNLISTED_GUARDS_AFTER: Duration = Duration::days(20);
    static ref GUARD_CONFIRMED_MIN_LIFETIME: Duration = Duration::days(60);
    // static ref GUARD_LIFETIME: Duration = Duration::days(6);
    // static ref REMOVE_UNLISTED_GUARDS_AFTER: Duration = Duration::days(1);
    // static ref GUARD_CONFIRMED_MIN_LIFETIME: Duration = Duration::days(3);
    static ref MIN_FILTERED_SAMPLE: usize = 20;
    static ref MAX_SAMPLE_SIZE: usize = 60;
    static ref MAX_SAMPLE_THRESHOLD: f64 = 0.2;
    static ref N_PRIMARY_GUARDS: usize = 3;
    static ref N_USABLE_PRIMARY_GUARDS: usize = 1;
}

#[derive(Debug)]
pub(crate) struct GuardHandling {
    sampled_guards: Vec<SampledGuard>,
    confirmed_guards: Vec<ConfirmedGuard>,
    primary_guards: Vec<Fingerprint>,
}

impl GuardHandling {
    pub fn new() -> GuardHandling {
        GuardHandling {
            sampled_guards: Vec::new(),
            confirmed_guards: Vec::new(),
            primary_guards: Vec::new(),
        }
    }

    pub fn timed_updates(
        &mut self,
        now: &DateTime<Utc>,
        circgen: &CircuitGenerator,
        observer: &mut ClientObserver,
    ) {
        // update guard information
        for guard in self.sampled_guards.iter_mut() {
            match circgen.lookup_relay(&guard.fingerprint) {
                Some(relay) if relay.flags.contains(&Flag::Running) => {
                    guard.first_unlisted_at = None;
                }
                _ => {
                    // relay is either missing or not running
                    if guard.is_listed() {
                        guard.set_unlisted(now);
                    }
                }
            }
        }

        // remove old guards
        {
            let mut guards_to_remove = RHashSet::default();
            for guard in self.sampled_guards.iter() {
                if !guard.is_listed()
                    && (*now - guard.first_unlisted_at.unwrap() >= *REMOVE_UNLISTED_GUARDS_AFTER)
                {
                    guards_to_remove.insert(guard.fingerprint.clone());
                    observer.notify_guard_removed_offline(now, &guard.fingerprint);
                }

                if (*now - guard.added_on) >= *GUARD_LIFETIME
                    && match self.get_confirmed_on(&guard.fingerprint) {
                        None => true,
                        Some(confirmed_date) => {
                            *now - confirmed_date >= *GUARD_CONFIRMED_MIN_LIFETIME
                        }
                    }
                {
                    guards_to_remove.insert(guard.fingerprint.clone());
                    observer.notify_guard_removed_too_old(now, &guard.fingerprint);
                }
            }

            self.sampled_guards
                .retain(|guard| !guards_to_remove.contains(&guard.fingerprint));
            self.confirmed_guards
                .retain(|guard| !guards_to_remove.contains(&guard.fingerprint));
        }

        self.recompute_primary_guards();
    }

    fn recompute_primary_guards(&mut self) {
        let mut primary_guards = Vec::new();

        // filtered guards
        let filtered_guards: Vec<_> = self
            .sampled_guards
            .iter()
            .filter(|guard| guard.is_listed())
            .map(|guard| guard.fingerprint.clone())
            .collect();

        for confirmed_guard in self.confirmed_guards.iter() {
            // is this confirmed guard also in filtered_guards?
            if filtered_guards
                .iter()
                .position(|x| x == &confirmed_guard.fingerprint)
                .is_some()
            {
                primary_guards.push(confirmed_guard.fingerprint.clone());
            }

            if primary_guards.len() == *N_PRIMARY_GUARDS {
                break;
            }
        }

        // if primary_guards.len() < *N_PRIMARY_GUARDS {
        //     let usable_guards = self.usable_guards(now, circgen);
        //     for i in primary_guards.len()..*N_PRIMARY_GUARDS {
        //         primary_guards.push(usable_guards[i].clone());
        //     }
        // }

        self.primary_guards = primary_guards;
    }

    /// Get the time when a guard was confirmed, if it is confirmed
    fn get_confirmed_on(&self, guard: &Fingerprint) -> Option<DateTime<Utc>> {
        for confirmed_guard in self.confirmed_guards.iter() {
            if &confirmed_guard.fingerprint == guard {
                return Some(confirmed_guard.confirmed_on);
            }
        }
        None
    }

    fn usable_guards(
        &mut self,
        now: &DateTime<Utc>,
        circgen: &CircuitGenerator,
    ) -> Vec<Fingerprint> {
        loop {
            let usable_guards: Vec<_> = self
                .sampled_guards
                .iter()
                .filter(|guard| guard.is_listed())
                .collect();

            // Do we have enough sampled relays that are usable?
            let (guards_in_consensus, _, _) = circgen.num_relays();
            let max_sampled = min(
                (*MAX_SAMPLE_THRESHOLD as f64 * guards_in_consensus as f64) as usize,
                *MAX_SAMPLE_SIZE,
            );

            if usable_guards.len() < *MIN_FILTERED_SAMPLE && usable_guards.len() < max_sampled {
                // sample a new guard and add it to the sampled_guards list
                self.sampled_guards.push(SampledGuard::new(
                    now,
                    circgen,
                    &self
                        .sampled_guards
                        .iter()
                        .map(|guard| &guard.fingerprint)
                        .collect(),
                ));
                continue;
            }

            break usable_guards
                .into_iter()
                .map(|guard| guard.fingerprint.clone())
                .collect();
        }
    }

    // /// Pick a usable guard, possibly sampling completely new ones
    // fn pick_usable_guard(
    //     &mut self,
    //     now: &DateTime<Utc>,
    //     circgen: &CircuitGenerator,
    // ) -> Fingerprint {
    //     let usable_guards = self.usable_guards(now, circgen);

    //     let mut rng = get_rng();
    //     let i = rng.gen_range(0..usable_guards.len());

    //     usable_guards[i].clone()
    // }

    pub fn mark_as_confirmed(&mut self, guard: &Fingerprint, now: &DateTime<Utc>) {
        // if not already confirmed, confirm it now
        if self
            .confirmed_guards
            .iter()
            .position(|confirmed| &confirmed.fingerprint == guard)
            .is_none()
        {
            self.confirmed_guards
                .push(ConfirmedGuard::new(guard.clone(), now));

            self.recompute_primary_guards();
        }
    }

    pub fn get_guard_for_circuit(
        &mut self,
        now: &DateTime<Utc>,
        circgen: &CircuitGenerator,
    ) -> Fingerprint {
        if self.primary_guards.len() >= *N_USABLE_PRIMARY_GUARDS {
            let mut rng = get_rng();
            let chosen_primary = rng.gen_range(0..*N_USABLE_PRIMARY_GUARDS);

            self.primary_guards[chosen_primary].clone()
        } else {
            self.usable_guards(now, circgen).into_iter().next().unwrap()
        }
    }
}

#[derive(Debug)]
struct ConfirmedGuard {
    fingerprint: Fingerprint,
    confirmed_on: DateTime<Utc>,
}

impl ConfirmedGuard {
    fn new(fingerprint: Fingerprint, now: &DateTime<Utc>) -> ConfirmedGuard {
        ConfirmedGuard {
            fingerprint,
            confirmed_on: random_past(now, *GUARD_LIFETIME / 10),
        }
    }
}

#[derive(Debug)]
struct SampledGuard {
    fingerprint: Fingerprint,
    added_on: DateTime<Utc>,
    first_unlisted_at: Option<DateTime<Utc>>,
}

impl SampledGuard {
    fn new(
        now: &DateTime<Utc>,
        circgen: &CircuitGenerator,
        existing_guards: &Vec<&Fingerprint>,
    ) -> SampledGuard {
        let new_guard = circgen.sample_new_guard(&existing_guards).unwrap();

        SampledGuard {
            fingerprint: new_guard.fingerprint.clone(),
            added_on: random_past(now, *GUARD_LIFETIME / 10),
            first_unlisted_at: None,
        }
    }

    fn is_listed(&self) -> bool {
        self.first_unlisted_at.is_none()
    }

    /// Set this guard to unlisted and randomize the `unlisted` time.
    fn set_unlisted(&mut self, now: &DateTime<Utc>) {
        self.first_unlisted_at = Some(random_past(now, *REMOVE_UNLISTED_GUARDS_AFTER / 5))
    }
}

fn random_past(now: &DateTime<Utc>, range: impl Borrow<Duration>) -> DateTime<Utc> {
    let mut rng = get_rng();
    let offset = Duration::milliseconds(rng.gen_range(0..range.borrow().num_milliseconds()));

    *now - offset
}
