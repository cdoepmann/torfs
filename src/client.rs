//! Implementation of simulated clients/users

pub(crate) struct Client {
    id: u64,
}

impl Client {
    /// Construct a new Client
    pub(crate) fn new(id: u64) -> Client {
        Client { id }
    }

    /// Called from outside when the simulation enters a new epoch,
    /// with a new consensus being available
    fn trigger_new_epoch(&mut self) {
        // use user model to decide whether to build a circuit and what kind
    }
}
