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

    pub fn make_stream(&self, time: DateTime<Utc>) -> PacketStream {
        PacketStream {
            chain: markov::MarkovChain::new((*self.model).clone(), time),
        }
    }
}
