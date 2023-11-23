//! User models for costumizing client behavior
//!
//! These models currently also generate the response packet traces so they can
//! delay further requests until the previous one is finished.

use chrono::{DateTime, Duration, Utc};
use rand::Rng;
use rand_distr::{Distribution, Exp};
use seeded_rand::get_rng;

use crate::packet_model::{FlowOfStreams, PacketModelParameters, StreamModelParameters};

/// A user behavior model that determines when to initiate which kind of
/// traffic through the Tor network.
///
/// This is essentially just an iterator of `Request`s which will be carried out
/// by the Tor client.
pub(crate) trait UserModel: Iterator<Item = Request> {}

/// A traffic request by the user, to be carried out by the Tor client
///
/// As a replacement for a full network simulation, this also contains a sequence
/// of timestamps that indicate when the response packets will be sent by the
/// server.
#[derive(Clone, Debug)]
pub(crate) struct Request {
    /// Time of this request
    pub time: DateTime<Utc>,
    /// Remote port to connect to
    pub port: u16,
    /// Response packets the server will send
    pub packet_timestamps: Vec<DateTime<Utc>>,
}

/// A dummy client that connects to HTTPS randomly every 0-3 days
#[allow(unused)]
pub(crate) struct DummyUser {
    current_time: DateTime<Utc>,
    // packet model to generate the response timestamps
    packet_model: PacketModelParameters,
    not_after: DateTime<Utc>,
}

impl DummyUser {
    /// Create a new dummy user at a given point in time
    #[allow(unused)]
    pub fn new(
        start_time: DateTime<Utc>,
        packet_model: PacketModelParameters,
        not_after: DateTime<Utc>,
    ) -> DummyUser {
        DummyUser {
            current_time: start_time,
            packet_model,
            not_after,
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
        let request_time = self.current_time;

        // generate the stream of packets
        let packet_timestamps = self
            .packet_model
            .make_packetstream(request_time)
            .generate_timestamps(self.not_after)
            .unwrap();

        // wait with further requests until this request is over
        // TODO: network latency?
        if let Some(last_timestamp) = packet_timestamps.last() {
            self.current_time = last_timestamp.clone();
        }

        Some(Request {
            time: request_time,
            port: 443,
            packet_timestamps,
        })
    }
}

impl UserModel for DummyUser {}

/// A user model that behaves much like the one modelled by the PrivCount paper
/// and implemented in tornettools.
///
/// However, it differs in that it does not actually create circuits because
/// these are managed by ourselves in the circuit manager. The "generated"
/// circuits are instead interpreted as flows that govern the creation of
/// multiple streams in a row.
pub(crate) struct PrivcountUser {
    flow_model: ExponentialFlowModel,
    current_flow: Option<FlowOfStreams>,
    stream_model_parameters: StreamModelParameters,
    // packet model to generate the response timestamps
    packet_model: PacketModelParameters,
    /// Do not generate packets after this time
    not_after: DateTime<Utc>,
}

impl PrivcountUser {
    /// Create a new PrivCount user at a given point in time, who creates the
    /// specified amount of flows every 10 minutes
    pub fn new(
        start_time: DateTime<Utc>,
        flows_every_10min: f64,
        stream_model: StreamModelParameters,
        packet_model: PacketModelParameters,
        not_after: DateTime<Utc>,
    ) -> PrivcountUser {
        PrivcountUser {
            flow_model: ExponentialFlowModel::new(start_time, flows_every_10min),
            current_flow: None,
            stream_model_parameters: stream_model,
            packet_model,
            not_after,
        }
    }
}

impl Iterator for PrivcountUser {
    type Item = Request;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // Advance the model until we have a stream
            match self.current_flow {
                Some(ref mut current_flow) => {
                    // we are currently inside a flow
                    match current_flow.next() {
                        Some(request_time) => {
                            // this is the next TCP stream the user requests

                            // make sure future flows do not overlap with this one
                            self.flow_model.advance_to(request_time);

                            // generate the stream of packets
                            let packet_timestamps = self
                                .packet_model
                                .make_packetstream(request_time)
                                .generate_timestamps(self.not_after)
                                .unwrap();

                            // wait with further requests until this request is over
                            // TODO: network latency?
                            if let Some(last_timestamp) = packet_timestamps.last() {
                                current_flow.advance_to(last_timestamp.clone());
                                self.flow_model.advance_to(last_timestamp.clone());
                            }

                            return Some(Request {
                                time: request_time,
                                port: 443,
                                packet_timestamps,
                            });
                        }
                        None => {
                            // this flow has finished, no more streams
                            self.current_flow = None;
                        }
                    }
                }
                None => {
                    // there is no active flow, we have to start one
                    let flow_time = self.flow_model.next().unwrap(); // this is an infinite stream, so unwrap is fine
                    self.current_flow = Some(self.stream_model_parameters.make_flow(flow_time));
                }
            }
        }
    }
}

impl UserModel for PrivcountUser {}

/// A flow model that emits new flows based on an expontential distribution
struct ExponentialFlowModel {
    current_time: DateTime<Utc>,
    distr: Exp<f64>,
}

impl ExponentialFlowModel {
    fn new(current_time: DateTime<Utc>, flows_every_10min: f64) -> ExponentialFlowModel {
        let usec_per_flow: f64 = (10.0 * 60.0 * 1000.0 * 1000.0) / flows_every_10min;
        let exponential_rate = 1.0 / usec_per_flow;
        ExponentialFlowModel {
            current_time,
            distr: Exp::new(exponential_rate).unwrap(),
        }
    }

    fn advance_to(&mut self, new_time: DateTime<Utc>) {
        self.current_time = new_time;
    }
}

impl Iterator for ExponentialFlowModel {
    type Item = DateTime<Utc>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut rng = get_rng();

        let micros = self.distr.sample(&mut rng).round() as i64;
        let delay = Duration::microseconds(micros);

        self.current_time += delay;

        Some(self.current_time.clone())
    }
}

/// Get the number of clients to create, based on the PrivCount measurements.
///
/// > Privacy-Preserving Dynamic Learning of Tor Network Traffic
/// > Proceedings of the 25th ACM Conference on Computer and Communication Security (CCS 2018)
/// > by Rob Jansen
pub(crate) fn get_privcount_users() -> u64 {
    // implemented after tornettools' generate_tgen.py > __get_client_counts()

    // data/privcount/measurement1/privcount.tallies.1508707017-1508793717.json
    // EntryActiveClientIPCount
    let raw: u64 = 1436887;

    let privcount_scale: f64 = 0.0126;
    let privcount_periods_per_day = 144;
    let scale_factor = (1.0 / privcount_scale) / privcount_periods_per_day as f64;

    (raw as f64 * scale_factor) as u64
}

/// Get the number of circuits to create every 10 minutes based on the PrivCount measurements.
///
/// > Privacy-Preserving Dynamic Learning of Tor Network Traffic
/// > Proceedings of the 25th ACM Conference on Computer and Communication Security (CCS 2018)
/// > by Rob Jansen
pub(crate) fn get_privcount_circuits_10min() -> f64 {
    // implemented after tornettools' generate_tgen.py > __get_client_counts()

    // data/privcount/measurement3/privcount.tallies.1515796790-1515883190.json
    // ExitActiveCircuitCount
    let raw: u64 = 4575895;

    let privcount_scale: f64 = 0.0213;
    let privcount_periods_per_day = 144;
    let scale_factor = (1.0 / privcount_scale) / privcount_periods_per_day as f64;

    raw as f64 * scale_factor
}
