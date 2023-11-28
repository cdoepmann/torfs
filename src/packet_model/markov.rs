use super::parse::StreamEdge;
use super::parse::StreamEdgeEmission;
use super::parse::StreamNode;
use super::parse::StreamPacketModel;

use chrono::{DateTime, Duration, Utc};
use core::panic;
use rand::distributions::WeightedIndex;
use rand_distr::{Distribution, Exp, LogNormal};
use seeded_rand::get_rng;
use seeded_rand::RHashMap as HashMap;
use std::fmt;
use std::fmt::Display;

/* This is my interpretation of the Model described in Privacy-Preserving Dynamic Learning of Tor Network Traffic
 * The data is from: https://github.com/tmodel-ccs2018/tmodel-ccs2018.github.io
 * The documentation of the model is here: https://github.com/shadow/tgen/blob/main/doc/TGen-Markov-Models.md
 *
 * The original model describes three layers:
 * - a traffic model which models when noew traffic should start
 * - a stream model which models when a new stream should start
 * - a packet model which models when a packet is sent from client to server or server to client
 *
 * We implemented the last two the packet and stream model which have a very close syntax and fileformat.
 *
 * The original model used a graph to describe the relation and stored its definition as graphML file.
 * these files can be found in the data directory together with a script to transform them to a JSON file.
 *
 * This graph had two kinds of nodes and two kinds of edges.
 * Nodes of the type "state", which represent the states in the markov chain and nodes of the type
 * "observation" which, signal that an event happens.
 *
 * The edges of type "transition" are used to transfer between "state" nodes and the edges of type
 * "emission" connect "state" with observation nodes.
 *
 * We remodeled this and used edges strictly as transitions between states and stored the
 * "emission" edges as "actions" at each states.
 *
 * Each node can have multiple transitions and multiple actions which are selected based on the defined weight.
 *
 *
*/

pub struct MarkovChain {
    pub start: String,
    pub current_state: String,
    pub current_time: DateTime<Utc>,
    pub states: HashMap<String, MarkovState>,
    pub stopped: bool,
}

impl Display for MarkovChain {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "start:{}\n current_state: {}\n current_time: {}\n stopped: {}\n states:\n{:?}",
            self.start,
            self.current_state,
            self.current_time,
            self.stopped,
            self.states.values()
        )
    }
}

//  first event happens at time 0
/* get_next logic:
 * 1. Transition to the next state#
 * 2. Emission of an event
 * 3. Sampling of the delay (time between this and the next transition)
 * 4. set time of the next event (current time + delay)
 * 5. update the state
 */
impl MarkovChain {
    pub fn get_next(&mut self, not_after: DateTime<Utc>) -> (DateTime<Utc>, Emission) {
        /* No more hops after the generations has stopped */
        if self.stopped {
            return (self.current_time, Emission::StopGenerating);
        }

        let state = self.states.get(&self.current_state).unwrap();
        let next_state = self.states.get(&state.transition()).unwrap();
        let (emission, delay) = next_state.emission();
        assert!(delay >= Duration::microseconds(0));
        let time = self.current_time;

        if let Emission::StopGenerating = emission {
            self.stopped = true;
        }

        // make sure the current time does not leave the valid time range and
        // does not overflow
        if delay >= (not_after - self.current_time) {
            self.current_time = not_after;
            self.stopped = true;
        }

        self.current_state = next_state.id.clone();
        return (time, emission);
    }

    /// Move to a new time, but change no other internal state
    pub fn advance_to(&mut self, new_time: DateTime<Utc>) {
        self.current_time = new_time;
    }

    /* Takes the parsed JSON and transforms it into our more intuitive model */
    pub fn new(model: StreamPacketModel, current_time: DateTime<Utc>) -> Self {
        let mut start: Option<String> = None;
        let mut states: HashMap<String, MarkovState> = HashMap::default();

        for node in model.nodes {
            match node {
                StreamNode::Start(start_node) => {
                    if start_node.id == "start" {
                        if start.is_none() {
                            let new_start = MarkovState::new(start_node.id.clone());
                            states.insert(start_node.id.clone(), new_start);
                            start = Some(start_node.id);
                        } else {
                            panic!("Multiple start nodes! Abort!");
                        }
                    } else {
                        panic!("Start node but id is not \"start\"! Abort!");
                    }
                }
                StreamNode::Standard(standard) => match standard.ttype.as_str() {
                    "state" => {
                        states.insert(standard.id.clone(), MarkovState::new(standard.id));
                    }
                    "observation" => {
                        /* We don't need to store them at this point, since it only defines the name at this point
                         * which is also part of every emission edge
                         * we will for the moment hard-code the semantic
                         */
                    }
                    _ => {
                        println!("Unknown Stream node state: {}", standard.ttype);
                    }
                },
            };
        }

        for link in model.links {
            match link {
                StreamEdge::Emission(em) => {
                    /* First Sanity-check */
                    if em.ttype != "emission" {
                        panic!("Unexpected type for emission: {}", em.ttype);
                    }

                    let delay = MarkovDelay::new(&em);
                    let emission = Emission::new(&em);
                    let action = MarkovAction {
                        weight: em.weight,
                        emission: emission,
                        delay: delay,
                    };
                    let source_state = states.get_mut(&em.source).unwrap();
                    source_state.actions.push(action);
                }
                StreamEdge::Transition(transition) => {
                    /* First Sanity-check */
                    if transition.ttype != "transition" {
                        panic!("Unexpected type for transmission: {}", transition.ttype);
                    }
                    let source_state = states.get_mut(&transition.source).unwrap();

                    let edge = MarkovEdge {
                        weight: transition.weight,
                        target: transition.target,
                    };
                    source_state.transitions.push(edge);
                }
            };
        }

        let start = start.unwrap();

        MarkovChain {
            start: start.clone(),
            current_state: start,
            current_time: current_time,
            states: states,
            stopped: false,
        }
    }
}

#[derive(Debug)]
pub struct MarkovState {
    pub id: String,
    pub actions: Vec<MarkovAction>,
    pub transitions: Vec<MarkovEdge>,
}

impl MarkovState {
    pub fn new(id: String) -> Self {
        MarkovState {
            id: id,
            actions: Vec::new(),
            transitions: Vec::new(),
        }
    }
    pub fn transition(self: &Self) -> String {
        let mut choices = vec![];
        let mut weights = vec![];
        for t in self.transitions.iter() {
            choices.push(&t.target);
            weights.push(t.weight);
        }
        let dist = WeightedIndex::new(&weights).unwrap();
        let mut rng = get_rng();
        choices[dist.sample(&mut rng)].to_string()
    }
    pub fn emission(self: &Self) -> (Emission, Duration) {
        let mut choices = vec![];
        let mut weights = vec![];
        for a in &self.actions {
            choices.push(a);
            weights.push(a.weight);
        }
        let dist = WeightedIndex::new(&weights).unwrap();
        let mut rng = get_rng();
        let action = choices[dist.sample(&mut rng)];
        let emission = action.emission;
        let delay = action.sample_delay();
        (emission, delay)
    }
}

impl Display for MarkovState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Id:{}\n actions:\n {:?}\ntransitions:\n {:?}",
            self.id, self.actions, self.transitions
        )
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Emission {
    GeneratePacketFromClientToServer,
    GeneratePacketFromServerToClient,
    NewStream,
    StopGenerating,
}

impl Emission {
    fn new(em: &StreamEdgeEmission) -> Self {
        match em.target.as_str() {
            "+" => Emission::GeneratePacketFromClientToServer,
            "-" => Emission::GeneratePacketFromServerToClient,
            "F" => Emission::StopGenerating,
            "$" => Emission::NewStream,
            _ => {
                panic!("Unknown emission target: {}", em.target);
            }
        }
    }
}

impl fmt::Display for Emission {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let str = match self {
            Emission::GeneratePacketFromClientToServer => "C -> S",
            Emission::GeneratePacketFromServerToClient => "S -> C",
            Emission::NewStream => "new Stream",
            Emission::StopGenerating => "STOP",
        };
        write!(f, "{}", str)
    }
}
#[derive(Debug)]
pub struct MarkovAction {
    pub weight: f64,
    pub emission: Emission,
    pub delay: MarkovDelay,
    //sample: Box<dyn Fn() -> Duration>,
}
impl MarkovAction {
    fn sample_delay(self: &Self) -> Duration {
        match &self.delay {
            MarkovDelay::Exponential(exp) => sample_exponential(exp),
            MarkovDelay::LogNormal(lnormal) => sample_log_normal(lnormal),
            MarkovDelay::None => Duration::microseconds(0),
        }
    }
}

impl Display for MarkovAction {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} -> {}", self.weight, self.emission)
    }
}
fn sample_exponential(ex: &MarkovExponential) -> Duration {
    let exp = Exp::new(ex.lambda).unwrap();
    let v = exp.sample(&mut get_rng()).round() as i64;
    Duration::microseconds(v)
}

fn sample_log_normal(lnormal: &MarkovLogNormal) -> Duration {
    let log_normal = LogNormal::new(lnormal.mu, lnormal.sigma).unwrap();
    let v = log_normal.sample(&mut get_rng()).round() as i64;
    Duration::microseconds(v)
}

#[derive(Debug)]
pub enum MarkovDelay {
    Exponential(MarkovExponential),
    LogNormal(MarkovLogNormal),
    None,
}

impl MarkovDelay {
    fn new(em: &StreamEdgeEmission) -> Self {
        if em.exp_lambda > 0.0 && em.lognorm_mu == 0.0 && em.lognorm_sigma == 0.0 {
            return MarkovDelay::Exponential(MarkovExponential {
                lambda: em.exp_lambda,
            });
        }
        if em.lognorm_mu > 0.0 && em.lognorm_sigma > 0.0 && em.exp_lambda == 0.0 {
            return MarkovDelay::LogNormal(MarkovLogNormal {
                sigma: em.lognorm_sigma,
                mu: em.lognorm_mu,
            });
        }

        if em.exp_lambda == 0.0 && em.lognorm_mu == 0.0 && em.lognorm_sigma == 0.0 {
            return MarkovDelay::None;
        }
        panic!("Unsupport formatting for Stream Edge emssion: source:{} target: {} exp_lambda: {}, lognorm_mu: {}, lognorm_sigma: {}", em.source, em.target, em.exp_lambda, em.lognorm_mu, em.lognorm_sigma)
    }
}

#[derive(Debug)]
pub struct MarkovExponential {
    pub lambda: f64,
}

#[derive(Debug)]
pub struct MarkovLogNormal {
    pub sigma: f64,
    pub mu: f64,
}

#[derive(Debug)]
pub struct MarkovEdge {
    pub weight: f64,
    pub target: String,
}

impl Display for MarkovEdge {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} -> {}", self.weight, self.target)
    }
}
