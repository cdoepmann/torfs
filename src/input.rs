//! Wrappers and helpers for loading consensus and descriptor data

use std::fs::{self, File};
use std::io::prelude::*;
use std::path::PathBuf;

use anyhow;
use chrono::prelude::*;
use regex::Regex;
use tordoc;

use crate::cli::MonthYear;

/// Loader for data (consensus or descriptors) from an on-disk Tor data archive
pub(crate) struct TorArchive {
    dir: PathBuf,
}

impl TorArchive {
    /// Construct a new loader
    pub(crate) fn new(dir: impl Into<PathBuf>) -> anyhow::Result<TorArchive> {
        let dir = dir.into();

        if !dir.exists() {
            anyhow::bail!("Data archive path {} does not exist", dir.to_string_lossy())
        }

        if !dir.is_dir() {
            anyhow::bail!(
                "Data archive path {} is not a directory",
                dir.to_string_lossy()
            )
        }

        Ok(TorArchive { dir: dir })
    }

    /// Find all the consensuses in a given date range
    pub(crate) fn find_consensuses(
        &self,
        from: &MonthYear,
        to: &MonthYear,
    ) -> anyhow::Result<Vec<ConsensusHandle>> {
        // helper to get utf-8 file name
        let fname_as_string = |entry: &fs::DirEntry| -> anyhow::Result<String> {
            Ok(entry
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("invalid UTF-8 in path"))?)
        };

        // iterate through available consensuses
        let re_consdir = Regex::new(r"^consensuses-(\d{4})-(\d{2})$").unwrap();
        let re_subdir = Regex::new(r"^\d{2}$").unwrap();
        let re_consfile = Regex::new(r"^(\d{4}-\d{2}-\d{2}-\d{2}-\d{2}-\d{2})-consensus$").unwrap();

        let mut handles = Vec::new();

        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            match re_consdir.captures(fname_as_string(&entry)?.as_str()) {
                None => continue,
                Some(captures) => {
                    let dir_year = captures.get(1).unwrap().as_str().parse::<u16>().unwrap();
                    let dir_month = captures.get(2).unwrap().as_str().parse::<u8>().unwrap();

                    if dir_year < from.year || (dir_year == from.year && dir_month < from.month) {
                        continue;
                    }

                    if dir_year > to.year || (dir_year == to.year && dir_month > to.month) {
                        continue;
                    }
                }
            }

            // find all consensuses in this folder
            for subentry in fs::read_dir(entry.path())? {
                let subentry = subentry?;
                if !re_subdir.is_match(fname_as_string(&subentry)?.as_str()) {
                    continue;
                }

                for file in fs::read_dir(subentry.path())? {
                    let file = file?;
                    match re_consfile.captures(fname_as_string(&file)?.as_str()) {
                        None => continue,
                        Some(captures) => {
                            let raw_date = captures.get(1).unwrap().as_str();

                            handles.push(ConsensusHandle {
                                path: file.path(),
                                time: Utc.from_utc_datetime(&NaiveDateTime::parse_from_str(
                                    raw_date,
                                    "%Y-%m-%d-%H-%M-%S",
                                )?),
                            });
                        }
                    }
                }
            }
        }

        handles.sort_unstable_by_key(|h| h.time);

        Ok(handles)
    }
}

/// A reference to a consensus that is known to exist in the data archive
#[derive(Debug)]
pub(crate) struct ConsensusHandle {
    time: DateTime<Utc>,
    path: PathBuf,
}

impl ConsensusHandle {
    pub fn load(self) -> anyhow::Result<(tordoc::Consensus, Vec<tordoc::Descriptor>)> {
        let consensus = {
            let mut raw = String::new();
            let mut file = File::open(&self.path)?;
            file.read_to_string(&mut raw).unwrap();
            tordoc::Consensus::from_str(&raw).unwrap()
        };

        let descriptors = consensus
            .retrieve_descriptors(&self.path)
            .map_err(|_| anyhow::anyhow!("Error combining docuiments"))?; // TODO

        Ok((consensus, descriptors))
    }
}
