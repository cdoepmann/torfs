//! Generation of network traces for use in ppcalc

use num_cpus;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;
use std::sync::Mutex;
use std::thread::JoinHandle;
use zstd;

use anyhow;
use chrono::{DateTime, Utc};
use crossbeam::channel::{Receiver, Sender};
// use indicatif::ProgressIterator;
use lazy_static::lazy_static;
#[allow(unused_imports)]
use log::{debug, info, trace, warn};

use ppcalc_metric;
use ppcalc_metric::{DestinationId, MessageId, SourceId, TraceEntry};

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

pub fn make_trace_entries(
    timestamps: Vec<DateTime<Utc>>,
    client_id: u64,
) -> impl Iterator<Item = TraceEntry> {
    let sender = NEXT_SENDER.get_next();
    let message_ids = NEXT_MESSAGE.get_next_n(timestamps.len() as u64);

    timestamps
        .into_iter()
        .zip(message_ids.into_iter())
        .map(move |(timestamp, message_id)| {
            let source_timestamp = convert_time(timestamp);
            let destination_timestamp = source_timestamp + time::Duration::milliseconds(210);

            TraceEntry {
                m_id: MessageId::new(message_id),
                source_id: SourceId::new(sender),
                source_timestamp,
                destination_id: DestinationId::new(client_id),
                destination_timestamp,
            }
        })
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

pub struct TraceHandle {
    sender: Sender<Option<Vec<u8>>>,
    join_handle: JoinHandle<anyhow::Result<()>>,
}

impl TraceHandle {
    pub fn new(path: impl AsRef<Path>) -> anyhow::Result<TraceHandle> {
        let (sender, receiver) = crossbeam::channel::bounded(1024);

        let worker = TraceWorker::new(path, receiver)?;
        let join_handle = std::thread::spawn(move || worker.run());

        Ok(TraceHandle {
            sender,
            join_handle,
        })
    }

    pub fn get_writer(&self) -> MemoryCsvWriter {
        MemoryCsvWriter::new(self.sender.clone())
    }

    pub fn stop_and_join(self) -> anyhow::Result<()> {
        self.sender.send(None)?;
        self.join_handle.join().unwrap()?;
        Ok(())
    }
}

struct TraceWorker {
    receiver: Receiver<Option<Vec<u8>>>,
    file_writer: Box<dyn std::io::Write + Send>,
}

impl TraceWorker {
    fn new(
        path: impl AsRef<Path>,
        receiver: Receiver<Option<Vec<u8>>>,
    ) -> anyhow::Result<TraceWorker> {
        let file_writer: Box<dyn Write + Send> = {
            let path = path.as_ref();

            let file = File::create(path)?;

            if path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .ends_with(".zst")
            {
                let nproc = num_cpus::get_physical();
                Box::new({
                    let mut encoder = zstd::Encoder::new(file, 5)?;
                    encoder.multithread(nproc as u32)?;
                    encoder.auto_finish()
                })
            } else {
                Box::new(file)
            }
        };

        Ok(TraceWorker {
            receiver,
            file_writer,
        })
    }

    fn run(mut self) -> anyhow::Result<()> {
        self.file_writer
            .write_all(b"m_id,source_id,source_timestamp,destination_id,destination_timestamp\n")?;

        while let Some(data) = self.receiver.recv()? {
            assert!(&data.iter().filter(|x| x == &&b',').count() % 4 == 0);

            // let s = String::from_utf8_lossy(&data[..]);
            // info!("Got: \"{}\"", s);

            self.file_writer.write_all(&data[..])?;
        }

        Ok(())
    }
}

pub struct MemoryCsvWriter {
    sender: Sender<Option<Vec<u8>>>,
    csv_writer: csv::Writer<Vec<u8>>,
}

impl MemoryCsvWriter {
    pub fn new(sender: Sender<Option<Vec<u8>>>) -> MemoryCsvWriter {
        MemoryCsvWriter {
            sender,
            csv_writer: csv::WriterBuilder::new()
                .has_headers(false)
                .from_writer(Vec::with_capacity(65536)),
        }
    }

    pub fn write_entries(
        &mut self,
        entries: impl Iterator<Item = TraceEntry>,
    ) -> anyhow::Result<()> {
        for entry in entries {
            self.csv_writer.serialize(entry)?;
        }

        if self.csv_writer.get_ref().len() > 49152 {
            self.flush()?;
        }

        Ok(())
    }

    pub fn flush(&mut self) -> anyhow::Result<()> {
        let new_writer = csv::WriterBuilder::new()
            .has_headers(false)
            .from_writer(Vec::with_capacity(65536));
        let old_writer = std::mem::replace(&mut self.csv_writer, new_writer);
        self.sender
            .send(Some(old_writer.into_inner()?))
            .map_err(|e| anyhow::anyhow!(e))?;

        Ok(())
    }
}

// impl Clone for MemoryCsvWriter {
//     fn clone(&self) -> Self {
//         Self {
//             sender: self.sender.clone(),
//             csv_writer: csv::Writer::from_writer(Vec::with_capacity(65536)),
//         }
//     }
// }

impl Drop for MemoryCsvWriter {
    fn drop(&mut self) {
        self.flush().unwrap();
    }
}
