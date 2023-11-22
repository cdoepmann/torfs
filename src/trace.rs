//! Generation of network traces for use in ppcalc

use std::fs;
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;

use anyhow;
use chrono::{DateTime, Utc};
use indicatif::ProgressIterator;
use lazy_static::lazy_static;
#[allow(unused_imports)]
use log::{debug, info, trace, warn};

use ppcalc_metric;

lazy_static! {
    static ref NEXT_SENDER: GlobalCounter = GlobalCounter::new(0);
    static ref NEXT_MESSAGE: GlobalCounter = GlobalCounter::new(0);
}

/// The trace of a single client during simulation
pub struct ClientTrace {
    client_id: u64,
    /// collected messages (time message ID + exit/sender ID)
    messages: Vec<(DateTime<Utc>, u64, u64)>,
}

impl ClientTrace {
    pub fn new(client_id: u64) -> ClientTrace {
        ClientTrace {
            client_id,
            messages: Vec::new(),
        }
    }

    pub fn push_stream(&mut self, timestamps: Vec<DateTime<Utc>>) {
        if timestamps.len() == 0 {
            return;
        }

        let sender = NEXT_SENDER.get_next();
        let message_ids = NEXT_MESSAGE.get_next_n(timestamps.len() as u64);

        for (timestamp, message_id) in timestamps.into_iter().zip(message_ids.into_iter()) {
            self.messages.push((timestamp, message_id, sender));
        }
    }
}

pub fn write_traces_to_file(
    client_traces: Vec<ClientTrace>,
    fpath: impl AsRef<Path>,
) -> anyhow::Result<()> {
    let mut builder = ppcalc_metric::TraceBuilder::new();

    // Iterate over the client traces' messages in a sorted order.
    // (Note that the client traces themselves are sorted as there is no
    // packet reordering or overlapping streams)

    let total_messages: usize = client_traces.iter().map(|x| x.messages.len()).sum();

    use itertools::Itertools;
    let merged_iterator = client_traces
        .into_iter()
        .map(|client_trace| {
            // client_trace.messages.sort_unstable();
            client_trace
                .messages
                .into_iter()
                // remember the client ID
                .zip(std::iter::repeat(client_trace.client_id))
        })
        .kmerge_by(|((x, _, _), _), ((y, _, _), _)| x < y);

    info!("Writing sorted trace to file");

    let fpath = fpath.as_ref();
    let file_writer: Box<dyn Write> = {
        let file = fs::File::create(fpath)?;

        if fpath
            .file_name()
            .unwrap()
            .to_string_lossy()
            .ends_with(".zst")
        {
            Box::new(zstd::Encoder::new(file, 16)?.auto_finish())
        } else {
            Box::new(file)
        }
    };

    let print_every = total_messages / 1000;

    use ppcalc_metric::{DestinationId, MessageId, SourceId};
    ppcalc_metric::write_trace_to_writer_from_iter(
        merged_iterator
            .enumerate()
            .map(|(message_id, ((timestamp, _, sender_id), client_id))| {
                if message_id % print_every == 0 {
                    info!(
                        "completed {:.1} % ",
                        (message_id as f64 / print_every as f64) * 0.1
                    )
                }

                let received = timestamp + chrono::Duration::milliseconds(210);
                ppcalc_metric::TraceEntry {
                    m_id: MessageId::new(message_id as u64),
                    source_id: SourceId::new(sender_id),
                    source_timestamp: convert_time(timestamp),
                    destination_id: DestinationId::new(client_id),
                    destination_timestamp: convert_time(received),
                }
            }), // .progress_count(total_messages as u64)
        file_writer,
    )
    .map_err(|e| anyhow::anyhow!(e))?;

    // info!("Compiling full trace from client traces");

    // for mut client_trace in client_traces.into_iter().progress() {
    //     let client_id = client_trace.client_id;

    //     // empty this client trace's messages to save memory
    //     let trace_messages = std::mem::take(&mut client_trace.messages);

    //     for (timestamp, message_id, sender_id) in trace_messages {
    //         use ppcalc_metric::{DestinationId, MessageId, SourceId};

    //         // TODO: network model
    //         let received = timestamp + chrono::Duration::milliseconds(210);

    //         let entry = ppcalc_metric::TraceEntry {
    //             m_id: MessageId::new(message_id),
    //             source_id: SourceId::new(sender_id),
    //             source_timestamp: convert_time(timestamp),
    //             destination_id: DestinationId::new(client_id),
    //             destination_timestamp: convert_time(received),
    //         };

    //         builder.add_entry(entry);
    //     }
    // }

    // info!("Fix trace entries");
    // builder.fix();

    // // let trace = builder.build()?;

    // let fpath = fpath.as_ref();
    // let file_writer: Box<dyn Write> = {
    //     let file = fs::File::create(fpath)?;

    //     if fpath
    //         .file_name()
    //         .unwrap()
    //         .to_string_lossy()
    //         .ends_with(".zst")
    //     {
    //         Box::new(zstd::Encoder::new(file, 16)?.auto_finish())
    //     } else {
    //         Box::new(file)
    //     }
    // };
    // builder
    //     .write_to_writer_unchecked(file_writer)
    //     .map_err(|e| anyhow::anyhow!(e))?;

    Ok(())
}

fn convert_time(timestamp: DateTime<Utc>) -> time::PrimitiveDateTime {
    let unix = timestamp.timestamp_nanos(); // can only represent a few hundred ýears!_
    let time_offset = time::OffsetDateTime::from_unix_timestamp_nanos(unix as i128).unwrap();

    let date_part = time_offset.clone().date();
    let time_part = time_offset.time();

    time::PrimitiveDateTime::new(date_part, time_part)
}

/// A global counter to assign unique values
struct GlobalCounter {
    inner: Mutex<GlobalCounterInner>,
}
struct GlobalCounterInner {
    next_value: u64,
}

impl GlobalCounter {
    fn new(start: u64) -> GlobalCounter {
        GlobalCounter {
            inner: Mutex::new(GlobalCounterInner { next_value: start }),
        }
    }

    fn get_next(&self) -> u64 {
        let mut inner = self.inner.lock().unwrap();

        let res = inner.next_value;
        inner.next_value += 1;

        return res;
    }

    fn get_next_n(&self, n: u64) -> Vec<u64> {
        let first_value = {
            let mut inner = self.inner.lock().unwrap();
            let first_value = inner.next_value;
            inner.next_value += n;
            first_value
        };

        return (first_value..(first_value + n)).collect();
    }
}
