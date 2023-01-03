//! Silent observer that collects the simulated behavior and generates insight from that.

pub(crate) struct SimulationObserver;

impl SimulationObserver {
    /// Construct a new `SimulationObserver` for the simulation.
    pub(crate) fn new() -> SimulationObserver {
        SimulationObserver {}
    }
}
