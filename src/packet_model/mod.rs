//! Packet model implementation

mod markov;
mod parse;

use markov::Emission;

use std::fs;
use std::path::Path;
use std::rc::Rc;

use anyhow;
use chrono::{DateTime, Utc};

/// A model to generate a sequence of packets that are exchanged once a client
/// starts a request through the network. This currently only generates traffic
/// from the server to the client because this is the setting we analyze in ppcalc.
pub struct PacketStream {
    chain: markov::MarkovChain,
}

impl PacketStream {
    pub fn generate_timestamps(&mut self) -> anyhow::Result<Vec<DateTime<Utc>>> {
        // TODO maybe iterator

        let mut res = Vec::new();

        loop {
            let (time, emission) = self.chain.get_next();
            // println!("{}:{}", time, emission);

            match emission {
                Emission::GeneratePacketFromClientToServer => {
                    // we ignore this direction for now
                }
                Emission::GeneratePacketFromServerToClient => {
                    res.push(time);
                }
                Emission::NewStream => {
                    // This shouldn't happen.
                    anyhow::bail!("The packet model received an unexpected event (new stream). Did you maybe provide the wrong file?")
                }
                Emission::StopGenerating => {
                    break;
                }
            }
        }

        Ok(res)
    }
}

/// The parsed model parameters (the Markov chain) for the packet model
#[derive(Clone)]
pub struct PacketModelParameters {
    model: Rc<parse::StreamPacketModel>,
}

impl PacketModelParameters {
    pub fn new(path: impl AsRef<Path>) -> anyhow::Result<PacketModelParameters> {
        let path = path.as_ref();
        let data = fs::read_to_string(path)?;

        Ok(PacketModelParameters {
            model: Rc::new(parse::parse_stream_or_packet_model(data)?),
        })
    }

    pub fn make_packetstream(&self, time: DateTime<Utc>) -> PacketStream {
        PacketStream {
            chain: markov::MarkovChain::new((*self.model).clone(), time),
        }
    }
}

/// A flow that generates new streams
pub struct FlowOfStreams {
    chain: markov::MarkovChain,
}

impl FlowOfStreams {
    /// Move to a new time, but change no other internal state
    pub fn advance_to(&mut self, new_time: DateTime<Utc>) {
        self.chain.advance_to(new_time)
    }
}

impl Iterator for FlowOfStreams {
    type Item = DateTime<Utc>;

    fn next(&mut self) -> Option<Self::Item> {
        let (time, emission) = self.chain.get_next();

        match emission {
            Emission::GeneratePacketFromClientToServer => {
                panic!("The stream model received an unexpected event (generate packet). Did you maybe provide the wrong file?")
            }
            Emission::GeneratePacketFromServerToClient => {
                panic!("The stream model received an unexpected event (generate packet). Did you maybe provide the wrong file?")
            }
            Emission::NewStream => {
                return Some(time);
            }
            Emission::StopGenerating => {
                return None;
            }
        }
    }
}

/// The parsed model parameters (the Markov chain) for the stream model
#[derive(Clone)]
pub struct StreamModelParameters {
    model: Rc<parse::StreamPacketModel>,
}

impl StreamModelParameters {
    pub fn new(path: impl AsRef<Path>) -> anyhow::Result<StreamModelParameters> {
        let path = path.as_ref();
        let data = fs::read_to_string(path)?;

        Ok(StreamModelParameters {
            model: Rc::new(parse::parse_stream_or_packet_model(data)?),
        })
    }

    pub fn make_flow(&self, time: DateTime<Utc>) -> FlowOfStreams {
        FlowOfStreams {
            chain: markov::MarkovChain::new((*self.model).clone(), time),
        }
    }
}
