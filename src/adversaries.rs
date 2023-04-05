//! Creation and handling of adversaries

use std::net::IpAddr;

use anyhow;
use anyhow::Context;
#[allow(unused_imports)]
use log::{debug, info, trace, warn};

use seeded_rand::RHashSet;
use tordoc::{
    consensus::CondensedExitPolicy, consensus::Flag, consensus::Relay, descriptor::OrAddress,
    Consensus, Descriptor, Fingerprint,
};

use crate::cli::Cli;

pub(crate) struct Adversary {
    extra_relays: Vec<(Relay, Descriptor)>,
    adversary_fingerprints: RHashSet<Fingerprint>,
}

impl Adversary {
    /// Construct a new adversary object from the command-line arguments
    pub fn new(cli: &Cli) -> Adversary {
        let mut extra_relays = Vec::new();

        if let Some(adv_guards_num) = cli.adv_guards_num {
            let adv_guards_bw = cli.adv_guards_bw.unwrap(); // ensured by clap

            extra_relays.append(
                &mut (1..=adv_guards_num)
                    .into_iter()
                    .map(|index| make_adversarial_guard(index, adv_guards_bw))
                    .collect(),
            );
        }

        if let Some(adv_exits_num) = cli.adv_exits_num {
            let adv_exits_bw = cli.adv_exits_bw.unwrap(); // ensured by clap

            extra_relays.append(
                &mut (1..=adv_exits_num)
                    .into_iter()
                    .map(|index| {
                        make_adversarial_exit(index, cli.adv_guards_num.unwrap_or(0), adv_exits_bw)
                    })
                    .collect(),
            );
        }

        let adversary_fingerprints = extra_relays
            .iter()
            .map(|(r, _)| r.fingerprint.as_ref().unwrap().clone())
            .collect();

        Adversary {
            extra_relays,
            adversary_fingerprints,
        }
    }

    /// Carry out modifications to the consensus, if necessary for the adversary
    pub fn modify_consensus(&self, consensus: &mut Consensus, descriptors: &mut Vec<Descriptor>) {
        for (consensus_entry, descriptor) in self.extra_relays.iter() {
            consensus.relays.push(consensus_entry.clone());
            descriptors.push(descriptor.clone());
        }

        if self.extra_relays.len() > 0 {
            bwweights::recompute_bw_weights(consensus);
        }
    }

    /// Determine if a given fingerprint belongs to the adversary
    pub fn is_adversarial(&self, fingerprint: &Fingerprint) -> bool {
        self.adversary_fingerprints.contains(fingerprint)
    }
}

/// Generate a new (adversarial) guard relay and its descriptor
fn make_adversarial_guard(index: u64, weight: u64) -> (Relay, Descriptor) {
    let nickname = format!("BadGuyGuard{}", index);
    let fingerprint = Fingerprint::from_str_hex(format!("{:0>40}", index)).unwrap();
    let ip_address: IpAddr = format!("10.{}.0.1", index).parse().unwrap();

    let relay = Relay {
        nickname: Some(nickname.clone()),
        fingerprint: Some(fingerprint.clone()),
        digest: Some(fingerprint.clone()),
        published: None,
        address: None,
        or_port: None,
        dir_port: None,
        flags: Some(vec![
            Flag::Fast,
            Flag::Guard,
            Flag::Running,
            Flag::Stable,
            Flag::Valid,
        ]),
        version_line: None,
        protocols: None,
        exit_policy: Some(CondensedExitPolicy::reject_all()),
        bandwidth_weight: Some(weight),
    };

    let descriptor = Descriptor {
        nickname: Some(nickname.clone()),
        fingerprint: Some(fingerprint.clone()),
        digest: Some(fingerprint.clone()),
        published: None,
        or_addresses: Some(vec![OrAddress {
            ip: ip_address,
            port: 9001,
        }]),
        family_members: None,
        bandwidth_avg: None,
        bandwidth_burst: None,
        bandwidth_observed: None,
        exit_policy: None,
        exit_policies_ipv6: None,
    };

    (relay, descriptor)
}

/// Generate a new (adversarial) exit relay and its descriptor
fn make_adversarial_exit(index: u64, ip_offset: u64, weight: u64) -> (Relay, Descriptor) {
    let nickname = format!("BadGuyExit{}", index);
    let fingerprint = Fingerprint::from_str_hex(format!("{:F>40}", index)).unwrap();
    let ip_address: IpAddr = format!("10.{}.0.1", ip_offset + index).parse().unwrap();

    let relay = Relay {
        nickname: Some(nickname.clone()),
        fingerprint: Some(fingerprint.clone()),
        digest: Some(fingerprint.clone()),
        published: None,
        address: None,
        or_port: None,
        dir_port: None,
        flags: Some(vec![
            Flag::Fast,
            Flag::Exit,
            Flag::Running,
            Flag::Stable,
            Flag::Valid,
        ]),
        version_line: None,
        protocols: None,
        exit_policy: Some(CondensedExitPolicy::accept_all()),
        bandwidth_weight: Some(weight),
    };

    let descriptor = Descriptor {
        nickname: Some(nickname.clone()),
        fingerprint: Some(fingerprint.clone()),
        digest: Some(fingerprint.clone()),
        published: None,
        or_addresses: Some(vec![OrAddress {
            ip: ip_address,
            port: 9001,
        }]),
        family_members: None,
        bandwidth_avg: None,
        bandwidth_burst: None,
        bandwidth_observed: None,
        exit_policy: None,
        exit_policies_ipv6: None,
    };

    (relay, descriptor)
}

mod bwweights {
    use std::cmp::{max, min};
    use std::collections::BTreeMap;

    use tordoc::consensus::Flag;
    use tordoc::Consensus;

    #[allow(non_snake_case)]
    pub fn recompute_bw_weights(consensus: &mut Consensus) {
        let mut Wmd: i64;
        let mut Wed: i64;
        let mut Wgd: i64;
        let mut Wme: i64;
        let mut Wee: i64;
        let mut Wmg: i64;
        let mut Wgg: i64;
        // First, collect the total bandwidth values
        let mut E = 1i64;
        let mut G = 1i64;
        let mut D = 1i64;
        let mut M = 1i64;
        for relay in consensus.relays.iter() {
            let flags = relay.flags.as_ref().expect("relay missing flags");
            let bandwidth_weight = relay
                .bandwidth_weight
                .expect("relay missing bandwidth weight") as i64;

            let is_exit = flags.contains(&Flag::Exit) && !flags.contains(&Flag::BadExit);
            if is_exit && flags.contains(&Flag::Guard) {
                D += bandwidth_weight;
            } else if is_exit {
                E += bandwidth_weight;
            } else if flags.contains(&Flag::Guard) {
                G += bandwidth_weight;
            } else {
                M += bandwidth_weight;
            }
        }
        let T = E + G + D + M;
        let weightscale = 10000;

        if 3 * E >= T && 3 * G >= T {
            // Case 1: Neither are scarce
            // casename = "Case 1 (Wgd=Wmd=Wed)"
            Wmd = weightscale / 3;
            Wed = weightscale / 3;
            Wgd = weightscale / 3;
            Wee = (weightscale * (E + G + M)) / (3 * E);
            Wme = weightscale - Wee;
            Wmg = (weightscale * (2 * G - E - M)) / (3 * G);
            Wgg = weightscale - Wmg
        } else if 3 * E < T && 3 * G < T {
            // Case 2: Both Guards and Exits are scarce
            // Balance D between E and G, depending upon D capacity and
            // scarcity
            let R = min(E, G);
            let S = max(E, G);
            if R + D < S {
                // subcase a
                Wgg = weightscale;
                Wee = weightscale;
                Wmd = 0;
                Wme = 0;
                Wmg = 0;
                if E < G {
                    // casename = "Case 2a (E scarce)"
                    Wed = weightscale;
                    Wgd = 0;
                } else {
                    // casename = "Case 2a (G scarce)"
                    Wed = 0;
                    Wgd = weightscale;
                }
            } else {
                // subcase b R+D >= S
                // casename = "Case 2b1 (Wgg=weightscale, Wmd=Wgd)"
                Wee = (weightscale * (E - G + M)) / E;
                Wed = (weightscale * (D - 2 * E + 4 * G - 2 * M)) / (3 * D);
                Wme = (weightscale * (G - M)) / E;
                Wmg = 0;
                Wgg = weightscale;
                Wgd = (weightscale - Wed) / 2;
                Wmd = (weightscale - Wed) / 2;

                if let Some(_) = check_weights_errors(
                    Wgg,
                    Wgd,
                    Wmg,
                    Wme,
                    Wmd,
                    Wee,
                    Wed,
                    weightscale,
                    G,
                    M,
                    E,
                    D,
                    T,
                    10,
                    true,
                ) {
                    // casename = 'Case 2b2 (Wgg=weightscale, Wee=weightscale)'
                    Wee = weightscale;
                    Wgg = weightscale;
                    Wed = (weightscale * (D - 2 * E + G + M)) / (3 * D);
                    Wmd = (weightscale * (D - 2 * M + G + E)) / (3 * D);
                    Wmg = 0;
                    Wme = 0;
                    if Wmd < 0 {
                        // Too much bandwidth at middle position
                        // casename = 'case 2b3 (Wmd=0)'
                        Wmd = 0;
                    }
                    Wgd = weightscale - Wed - Wmd;
                }

                match check_weights_errors(
                    Wgg,
                    Wgd,
                    Wmg,
                    Wme,
                    Wmd,
                    Wee,
                    Wed,
                    weightscale,
                    G,
                    M,
                    E,
                    D,
                    T,
                    10,
                    true,
                ) {
                    None | Some(BwwError::BalanceMid) => {}
                    _ => {
                        panic!("bw weight error");
                    }
                }
            }
        } else {
            // if (E < T/3 or G < T/3)
            // Case 3: Guard or Exit is scarce
            let S = min(E, G);

            if 3 * (S + D) < T {
                // subcase a: S+D < T/3
                if G < E {
                    // casename = 'Case 3a (G scarce)'
                    Wgd = weightscale;
                    Wgg = weightscale;
                    Wmg = 0;
                    Wed = 0;
                    Wmd = 0;

                    if E < M {
                        Wme = 0;
                    } else {
                        Wme = (weightscale * (E - M)) / (2 * E);
                    }
                    Wee = weightscale - Wme;
                } else {
                    // G >= E
                    // casename = "Case 3a (E scarce)"
                    Wed = weightscale;
                    Wee = weightscale;
                    Wme = 0;
                    Wgd = 0;
                    Wmd = 0;
                    if G < M {
                        Wmg = 0;
                    } else {
                        Wmg = (weightscale * (G - M)) / (2 * G);
                    }
                    Wgg = weightscale - Wmg;
                }
            } else {
                // subcase S+D >= T/3
                if G < E {
                    // casename = 'Case 3bg (G scarce, Wgg=weightscale, Wmd == Wed'
                    Wgg = weightscale;
                    Wgd = (weightscale * (D - 2 * G + E + M)) / (3 * D);
                    Wmg = 0;
                    Wee = (weightscale * (E + M)) / (2 * E);
                    Wme = weightscale - Wee;
                    Wed = (weightscale - Wgd) / 2;
                    Wmd = (weightscale - Wgd) / 2;
                } else {
                    // G >= E
                    // casename = 'Case 3be (E scarce, Wee=weightscale, Wmd == Wgd'
                    Wee = weightscale;
                    Wed = (weightscale * (D - 2 * E + G + M)) / (3 * D);
                    Wme = 0;
                    Wgg = (weightscale * (G + M)) / (2 * G);
                    Wmg = weightscale - Wgg;
                    Wgd = (weightscale - Wed) / 2;
                    Wmd = (weightscale - Wed) / 2;
                }
            }
        }

        consensus.weights = Some(BTreeMap::from_iter(
            [
                ("Wbd", Wmd),
                ("Wbe", Wme),
                ("Wbg", Wmg),
                ("Wbm", weightscale),
                ("Wdb", weightscale),
                ("Web", weightscale),
                ("Wed", Wed),
                ("Wee", Wee),
                ("Weg", Wed),
                ("Wem", Wee),
                ("Wgb", weightscale),
                ("Wgd", Wgd),
                ("Wgg", Wgg),
                ("Wgm", Wgg),
                ("Wmb", weightscale),
                ("Wmd", Wmd),
                ("Wme", Wme),
                ("Wmg", Wmg),
                ("Wmm", weightscale),
            ]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v as u64)),
        ));
        //    /*
        //    * Provide Wgm=Wgg, Wmm=weight_scale, Wem=Wee, Weg=Wed. May later determine
        //    * that middle nodes need different bandwidth weights for dirport traffic,
        //    * or that weird exit policies need special weight, or that bridges
        //    * need special weight.
        //    *
        //    * NOTE: This list is sorted.
        //    */
        //   smartlist_add_asprintf(chunks,
        //     "bandwidth-weights Wbd=%d Wbe=%d Wbg=%d Wbm=%d "
        //     "Wdb=%d "
        //     "Web=%d Wed=%d Wee=%d Weg=%d Wem=%d "
        //     "Wgb=%d Wgd=%d Wgg=%d Wgm=%d "
        //     "Wmb=%d Wmd=%d Wme=%d Wmg=%d Wmm=%d\n",
        //     (int)Wmd, (int)Wme, (int)Wmg, (int)weight_scale,
        //     (int)weight_scale,
        //     (int)weight_scale, (int)Wed, (int)Wee, (int)Wed, (int)Wee,
        //     (int)weight_scale, (int)Wgd, (int)Wgg, (int)Wgg,
        //     (int)weight_scale, (int)Wmd, (int)Wme, (int)Wmg, (int)weight_scale);
    }

    fn check_eq(a: i64, b: i64, margin: i64) -> bool {
        if (a - b) >= 0 {
            (a - b) <= margin
        } else {
            (b - a) <= margin
        }
    }
    fn check_range(a: i64, b: i64, c: i64, d: i64, e: i64, f: i64, g: i64, mx: i64) -> bool {
        a >= 0
            && a <= mx
            && b >= 0
            && b <= mx
            && c >= 0
            && c <= mx
            && d >= 0
            && d <= mx
            && e >= 0
            && e <= mx
            && f >= 0
            && f <= mx
            && g >= 0
            && g <= mx
    }

    #[derive(Debug, PartialEq, Copy, Clone)]
    enum BwwError {
        SumD,
        SumG,
        SumE,
        Range,
        BalanceEg,
        BalanceMid,
    }

    /// Verify that our weights satify the formulas from dir-spec.txt
    #[allow(non_snake_case)]
    fn check_weights_errors(
        Wgg: i64,
        Wgd: i64,
        Wmg: i64,
        Wme: i64,
        Wmd: i64,
        Wee: i64,
        Wed: i64,
        weightscale: i64,
        G: i64,
        M: i64,
        E: i64,
        D: i64,
        T: i64,
        margin: i64,
        do_balance: bool,
    ) -> Option<BwwError> {
        // # Wed + Wmd + Wgd == weightscale
        if !check_eq(Wed + Wmd + Wgd, weightscale, margin) {
            return Some(BwwError::SumD);
        }
        // # Wmg + Wgg == weightscale
        if !check_eq(Wmg + Wgg, weightscale, margin) {
            return Some(BwwError::SumG);
        }
        // # Wme + Wee == weightscale
        if !check_eq(Wme + Wee, weightscale, margin) {
            return Some(BwwError::SumE);
        }
        // # Verify weights within range 0 -> weightscale
        if !check_range(Wgg, Wgd, Wmg, Wme, Wmd, Wed, Wee, weightscale) {
            return Some(BwwError::Range);
        }
        if do_balance {
            // #Wgg*G + Wgd*D == Wee*E + Wed*D
            if !check_eq(Wgg * G + Wgd * D, Wee * E + Wed * D, (margin * T) / 3) {
                return Some(BwwError::BalanceEg);
            }
            // #Wgg*G+Wgd*D == M*weightscale + Wmd*D + Wme * E + Wmg*G
            if !check_eq(
                Wgg * G + Wgd * D,
                M * weightscale + Wmd * D + Wme * E + Wmg * G,
                (margin * T) / 3,
            ) {
                return Some(BwwError::BalanceMid);
            }
        }

        None
    }
}
