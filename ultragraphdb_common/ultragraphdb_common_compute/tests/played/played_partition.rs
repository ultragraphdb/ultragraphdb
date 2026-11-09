/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A partition the test plays at an address of its own on the test's runtime: it answers a Scan from a table of keys as
//! `PROTOCOL.md` §13.13 and §13.14 say a partition does, and writes down every Scan and every grant of credit it took.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::time::Duration;

use compio::runtime::spawn;
use futures::StreamExt;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use ultragraphdb_common_role::{
    Answer, Push, ROLE_ITERATIONS, RoleCluster, Server, Session, serve_connection,
};
use ultragraphdb_core_ids::{Epoch, Version};
use ultragraphdb_core_keys::Keyspace;
use ultragraphdb_protocol_transport::{Address, Endpoint, ReceivedFrame};
use ultragraphdb_protocol_transport_compio::Listener;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Closed, ClosedPartition, ClosedView, Command, Entries, EntriesEnd,
    ErrorCode, ErrorReply, FLAG_LAST, Header, ObjectRecords, ObjectRecordsBuilder, ScanAt,
    ScanView,
};
use ultragraphdb_tool_bdd::TestResult;

/// The epoch every played partition has closed and answers at.
pub const EPOCH: u64 = 9;

/// The version every played partition answers at: the bound of [`EPOCH`].
pub fn version() -> u64 {
    Epoch::new(EPOCH).map_or(0, Version::bound_of)
}

/// The length of a key's keyspace byte and tenant, which a range of no past key stays within (`PROTOCOL.md` §13.13).
const TENANT_PREFIX: usize = 9;

/// What a played partition holds: each key's value.
type Table = BTreeMap<Vec<u8>, Vec<u8>>;

/// Entries of a range, each key and its value.
type Found = Vec<(Vec<u8>, Vec<u8>)>;

/// A prefix of keys, and how long a Scan first naming one is answered after.
type Delays = Vec<(Vec<u8>, Duration)>;

/// A range of a Scan a played partition took.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeenRange {
    /// The range's partition.
    pub partition: u32,
    /// Its first key.
    pub first: Vec<u8>,
    /// Its past key.
    pub past: Vec<u8>,
    /// The object its first list resumes at.
    pub after: Vec<u8>,
}

/// A Scan a played partition took.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeenScan {
    /// The connection it came on, counted from 1.
    pub connection: u32,
    /// Its stream.
    pub stream: u64,
    /// How it chose its version.
    pub at: ScanAt,
    /// Its `read_version`.
    pub read_version: u64,
    /// Its `epoch`.
    pub epoch: u64,
    /// Its `limit`.
    pub limit: u32,
    /// Its `deadline_ms`.
    pub deadline_ms: u32,
    /// Its ranges.
    pub ranges: Vec<SeenRange>,
}

impl SeenRange {
    /// Whether the range holds one key of the dictionary alone, which a frame answers in a run with the ranges beside it
    /// (`PROTOCOL.md` §13.14).
    pub fn is_dictionary_key(&self) -> bool {
        self.first.first() == Some(&Keyspace::Dictionary.byte())
            && self.past.split_last() == Some((&0, self.first.as_slice()))
    }
}

impl SeenScan {
    /// Whether the Scan only holds a version: several ranges of one key (`PinTarget`'s).
    pub fn holds_a_version(&self) -> bool {
        self.ranges.len() > 1
            && self
                .ranges
                .first()
                .is_some_and(|first| self.ranges.iter().all(|range| range.first == first.first))
    }
}

/// What one played partition saw.
#[derive(Debug, Default)]
pub struct PartitionLog {
    /// Every Scan, in order.
    pub scans: Vec<SeenScan>,
    /// Every grant of credit: the connection, the stream and the frames.
    pub credits: Vec<(u32, u64, u32)>,
    /// The connections it took.
    pub connections: u32,
}

/// A played partition: its table, its log and the delays it answers some Scans after.
#[derive(Debug, Clone)]
pub struct PlayedPartition {
    partition: u32,
    address: String,
    table: Rc<RefCell<Table>>,
    log: Rc<RefCell<PartitionLog>>,
    delays: Rc<RefCell<Delays>>,
}

impl PlayedPartition {
    /// Partition `partition` of `cluster`, listening at a port of its own on this runtime.
    pub async fn start(cluster: &RoleCluster, partition: u32) -> TestResult<Self> {
        let listener = Listener::bind(&Address::any_port("127.0.0.1")?).await?;
        let address = listener.local_address()?.to_string();
        let credentials = cluster.secret().credentials()?;
        let server = Server::new(cluster.id(), "storage", Endpoint::Partition)?
            .with_iterations(ROLE_ITERATIONS);
        let played = Self {
            partition,
            address,
            table: Rc::default(),
            log: Rc::default(),
            delays: Rc::default(),
        };
        let serving = played.clone();
        spawn(async move {
            while let Ok(carrier) = listener.accept().await {
                let connection = {
                    let mut log = serving.log.borrow_mut();
                    log.connections += 1;
                    log.connections
                };
                let (pushes, pushed) = unbounded();
                let session = PartitionSession {
                    played: serving.clone(),
                    connection,
                    pushes,
                    pushed,
                };
                let (server, credentials) = (server.clone(), credentials.clone());
                spawn(
                    async move { serve_connection(carrier, &server, credentials, session).await },
                )
                .detach();
            }
        })
        .detach();
        Ok(played)
    }

    /// The partition.
    pub fn partition(&self) -> u32 {
        self.partition
    }

    /// Where it listens.
    pub fn address(&self) -> &str {
        &self.address
    }

    /// Holds `value` under `key`.
    pub fn put(&self, key: Vec<u8>, value: Vec<u8>) {
        let _old: Option<Vec<u8>> = self.table.borrow_mut().insert(key, value);
    }

    /// A Scan whose first range's first key starts with `prefix` is answered `delay` after it arrives.
    pub fn delay(&self, prefix: Vec<u8>, delay: Duration) {
        self.delays.borrow_mut().push((prefix, delay));
    }

    /// Every Scan it took, those only holding a version left out.
    pub fn scans(&self) -> Vec<SeenScan> {
        self.log
            .borrow()
            .scans
            .iter()
            .filter(|scan| !scan.holds_a_version())
            .cloned()
            .collect()
    }

    /// Every Scan it took that only holds a version.
    pub fn pins(&self) -> Vec<SeenScan> {
        self.log
            .borrow()
            .scans
            .iter()
            .filter(|scan| scan.holds_a_version())
            .cloned()
            .collect()
    }

    /// The frames of credit granted on the streams of `scans`.
    pub fn credits_on(&self, scans: &[SeenScan]) -> u32 {
        self.log
            .borrow()
            .credits
            .iter()
            .filter(|(connection, stream, _)| {
                scans
                    .iter()
                    .any(|scan| scan.connection == *connection && scan.stream == *stream)
            })
            .map(|(_, _, frames)| *frames)
            .sum()
    }

    /// How many connections it took.
    pub fn connections(&self) -> u32 {
        self.log.borrow().connections
    }

    /// What `range` holds at the played version, at most `limit` entries, 0 for no limit: the entries, and whether the limit
    /// stopped it.
    fn entries_in(&self, range: &SeenRange, limit: u32) -> (Found, bool) {
        let table = self.table.borrow();
        let prefix = range.first.get(..TENANT_PREFIX).unwrap_or(&range.first);
        let mut within = table
            .range(range.first.clone()..)
            .take_while(|(key, _)| {
                if range.past.is_empty() {
                    key.starts_with(prefix)
                } else {
                    **key < range.past
                }
            })
            .map(|(key, value)| {
                let resumed = !range.after.is_empty() && *key == range.first;
                let value = if resumed {
                    records_from(value, &range.after)
                } else {
                    value.clone()
                };
                (key.clone(), value)
            });
        let wanted = if limit == 0 {
            usize::MAX
        } else {
            limit as usize
        };
        let taken: Vec<_> = within.by_ref().take(wanted).collect();
        let stopped = within.next().is_some();
        (taken, stopped)
    }

    /// The delay a Scan first naming `key` is answered after.
    fn delay_of(&self, key: &[u8]) -> Duration {
        self.delays
            .borrow()
            .iter()
            .find(|(prefix, _)| key.starts_with(prefix))
            .map_or(Duration::ZERO, |(_, delay)| *delay)
    }
}

/// The object records of the list `value` from the object `after` on, in the list's order, `after` itself included
/// (`PROTOCOL.md` §13.13).
fn records_from(value: &[u8], after: &[u8]) -> Vec<u8> {
    let mut kept = ObjectRecordsBuilder::new();
    if let Ok(records) = ObjectRecords::read(value) {
        for record in records.iter().filter(|record| record.encoding() >= after) {
            let statements: Vec<[u8; 16]> = record.statement_ids().collect();
            kept.push(record.encoding(), &statements);
        }
    }
    kept.value().to_vec()
}

/// A frame of `body` on `stream`, flagged last when `last`.
fn frame_of<B: BodyBuilder>(
    body: &B,
    cluster: u64,
    stream: u64,
    last: bool,
) -> Result<Vec<u8>, ErrorReply> {
    let header = Header::new(cluster, B::COMMAND).with_stream(stream);
    let header = if last {
        header.with_flags(FLAG_LAST)
    } else {
        header
    };
    body.frame(header)
        .map_err(|rule| ErrorReply::protocol(&rule))
}

/// One connection at a played partition.
#[derive(Debug)]
struct PartitionSession {
    played: PlayedPartition,
    connection: u32,
    pushes: UnboundedSender<(u64, Vec<u8>)>,
    pushed: UnboundedReceiver<(u64, Vec<u8>)>,
}

impl PartitionSession {
    /// The answer to the Scan `scan` on `stream`: one frame per range, the last flagged, each at the played version.
    fn answer_scan(
        &self,
        cluster: u64,
        stream: u64,
        scan: &SeenScan,
    ) -> Result<Vec<Vec<u8>>, ErrorReply> {
        let count = scan.ranges.len();
        let too_many = || ErrorReply::new(ErrorCode::Protocol, "too many ranges");
        if count > 1 && scan.ranges.iter().all(SeenRange::is_dictionary_key) {
            let through = u32::try_from(count - 1).map_err(|_| too_many())?;
            let mut run = Entries::new(0, EntriesEnd::Ended, EPOCH)
                .with_version(version())
                .with_through(through);
            for range in &scan.ranges {
                for (key, value) in self.played.entries_in(range, 0).0 {
                    run.push(&key, &value);
                }
            }
            return Ok(vec![frame_of(&run, cluster, stream, true)?]);
        }
        let mut frames = Vec::with_capacity(count);
        for (index, range) in scan.ranges.iter().enumerate() {
            let (entries, stopped) = self.played.entries_in(range, scan.limit);
            let end = if stopped {
                EntriesEnd::Limit
            } else {
                EntriesEnd::Ended
            };
            let at = u32::try_from(index).map_err(|_| too_many())?;
            let mut answer = Entries::new(at, end, EPOCH).with_version(version());
            for (key, value) in &entries {
                answer.push(key, value);
            }
            frames.push(frame_of(&answer, cluster, stream, index + 1 == count)?);
        }
        Ok(frames)
    }

    /// The Scan `view` holds, as the log keeps it.
    fn seen(&self, stream: u64, view: &ScanView<'_>) -> SeenScan {
        SeenScan {
            connection: self.connection,
            stream,
            at: view.at(),
            read_version: view.read_version(),
            epoch: view.epoch(),
            limit: view.limit(),
            deadline_ms: view.deadline_ms(),
            ranges: view
                .ranges()
                .map(|range| SeenRange {
                    partition: range.partition(),
                    first: range.first().to_vec(),
                    past: range.past().to_vec(),
                    after: range.after().to_vec(),
                })
                .collect(),
        }
    }
}

impl Session for PartitionSession {
    fn credited(&mut self, stream: u64, frames: u32) {
        self.played
            .log
            .borrow_mut()
            .credits
            .push((self.connection, stream, frames));
    }

    async fn answer(&mut self, stream: u64, frame: ReceivedFrame) -> Answer {
        let view = match frame.view() {
            Ok(view) => view,
            Err(rule) => return Answer::Refuse(ErrorReply::protocol(&rule)),
        };
        let cluster = view.header().cluster();
        if view.command() == Command::Closed {
            let asked = ClosedView::from_frame(&view)
                .ok()
                .and_then(|asked| asked.partition(0));
            let partition = asked.map_or(self.played.partition, |held| held.partition());
            let reply = Closed::new(vec![ClosedPartition::new(partition, EPOCH)]);
            return match frame_of(&reply, cluster, stream, true) {
                Ok(replied) => Answer::Reply(replied),
                Err(refusal) => Answer::Refuse(refusal),
            };
        }
        let scan = match ScanView::from_frame(&view) {
            Ok(scan) => self.seen(stream, &scan),
            Err(rule) => return Answer::Refuse(ErrorReply::protocol(&rule)),
        };
        self.played.log.borrow_mut().scans.push(scan.clone());
        let frames = match self.answer_scan(cluster, stream, &scan) {
            Ok(frames) => frames,
            Err(refusal) => return Answer::Refuse(refusal),
        };
        let delay = scan
            .ranges
            .first()
            .map_or(Duration::ZERO, |range| self.played.delay_of(&range.first));
        let pushes = self.pushes.clone();
        spawn(async move {
            compio::time::sleep(delay).await;
            // A connection gone takes no frame.
            frames
                .into_iter()
                .for_each(|built| drop(pushes.unbounded_send((stream, built))));
        })
        .detach();
        Answer::Open
    }

    async fn next_push(&mut self) -> (u64, Push) {
        match self.pushed.next().await {
            Some((stream, frame)) => (stream, Push::Frame(frame)),
            // The session holds a sender, so its channel never ends while it waits here.
            None => std::future::pending().await,
        }
    }
}
