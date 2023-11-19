//! User models for costumizing client behavior

use chrono::{DateTime, Utc};

use rand::Rng;
use seeded_rand::get_rng;

/// A user behavior model that determines when to initiate which kind of
/// traffic through the Tor network.
///
/// This is essentially just an iterator of `Request`s which will be carried out
/// by the Tor client.
pub(crate) trait UserModel: Iterator<Item = Request> {}

/// A traffic request by the user, to be carried out by the Tor client
#[derive(Clone, Debug)]
pub(crate) struct Request {
    /// Time of this request
    pub time: DateTime<Utc>,
    /// Remote port to connect to
    pub port: u16,
}

/// A dummy client that connects to HTTPS randomly every 0-3 days
pub(crate) struct DummyUser {
    current_time: DateTime<Utc>,
}

impl DummyUser {
    /// Create a new dummy user at a given point in time
    pub fn new(start_time: DateTime<Utc>) -> DummyUser {
        DummyUser {
            current_time: start_time,
        }
    }
}

impl Iterator for DummyUser {
    type Item = Request;

    fn next(&mut self) -> Option<Self::Item> {
        let mut rng = get_rng();

        // three days for now
        let wait_time = chrono::Duration::seconds(rng.gen_range(0..=60 * 60 * 24 * 3));
        // let wait_time = chrono::Duration::seconds(600);
        self.current_time += wait_time;

        Some(Request {
            time: self.current_time.clone(),
            port: 443,
        })
    }
}

impl UserModel for DummyUser {}
