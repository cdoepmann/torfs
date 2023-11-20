use std::fmt::Display;

use serde::{Deserialize, Serialize};
use serde_json::Result;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StreamPacketModel {
    pub directed: bool,
    pub multigraph: bool,
    pub graph: StreamGraph,
    pub nodes: Vec<StreamNode>,
    pub links: Vec<StreamEdge>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StreamGraph {
    node_default: String,
    edge_default: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StreamStandardNode {
    #[serde(rename = "type")]
    pub ttype: String,
    pub id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StreamStartNode {
    pub id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum StreamNode {
    Standard(StreamStandardNode),
    Start(StreamStartNode),
}

impl Display for StreamNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StreamNode::Standard(std) => write!(f, "{}", std.id),
            StreamNode::Start(str) => write!(f, "{}", str.id),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StreamEdgeEmission {
    pub exp_lambda: f64,
    #[serde(rename = "type")]
    pub ttype: String,
    pub lognorm_sigma: f64,
    pub weight: f64,
    pub lognorm_mu: f64,
    pub source: String,
    pub target: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StreamEdgeTransition {
    #[serde(rename = "type")]
    pub ttype: String,
    pub weight: f64,
    pub source: String,
    pub target: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum StreamEdge {
    Emission(StreamEdgeEmission),
    Transition(StreamEdgeTransition),
}

pub fn parse_stream_or_packet_model(data: String) -> Result<StreamPacketModel> {
    let stream_packet_model = serde_json::from_str(&data)?;

    Ok(stream_packet_model)
}
