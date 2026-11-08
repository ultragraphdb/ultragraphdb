/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A partition played by the test on the test's own runtime: real frames over a real connection, each Scan answered from a table
//! of keys at one version, as `PROTOCOL.md` §13.13 and §13.14 say a partition answers it, or refused or left unanswered as a test
//! scripts it.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, VecDeque};
use std::rc::Rc;
use std::time::Duration;

use ultragraphdb_common_role::{Answer, Session};
use ultragraphdb_core_keys::{Keyspace, QuadKey};
use ultragraphdb_protocol_transport::ReceivedFrame;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Closed, ClosedPartition, ClosedView, Command, Entries, EntriesEnd,
    ErrorCode, ErrorReply, FLAG_LAST, Header, MAX_FRAME_SIZE, ObjectRecords, ObjectRecordsBuilder,
    ScanAt, ScanView,
};

/// The epoch every played partition answers its Scans at, and its version: that epoch's last.
pub const PLAYED_EPOCH: u64 = 9;

/// The version every played partition answers its Scans at: the last of [`PLAYED_EPOCH`].
pub const PLAYED_VERSION: u64 = (PLAYED_EPOCH << 22) | ((1 << 22) - 1);

/// The bytes an Entries frame of a played partition holds at most: the protocol's largest frame, less room for its tables.
const FRAME_ROOM: usize = MAX_FRAME_SIZE - (64 << 10);

/// How often a played connection looks for a frame to send.
const LOOK: Duration = Duration::from_millis(1);

/// A key and its value.
type KeyValue = (Vec<u8>, Vec<u8>);

/// A range of a Scan, as the partition took it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RangeSeen {
    /// Its first key.
    pub first: Vec<u8>,
    /// The key past its last; empty for the end of its tenant's keyspace.
    pub past: Vec<u8>,
    /// The object its first list resumes at; empty for none.
    pub after: Vec<u8>,
}

/// A Scan the partition took.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanSeen {
    /// How it chooses its version.
    pub at: ScanAt,
    /// The version it names.
    pub read_version: u64,
    /// The epoch it waits for.
    pub epoch: u64,
    /// The most entries per range.
    pub limit: u32,
    /// How long it may wait for its epoch.
    pub deadline_ms: u32,
    /// How many IRIs its term table names.
    pub terms: usize,
    /// Its ranges.
    pub ranges: Vec<RangeSeen>,
}

impl ScanSeen {
    /// Whether the Scan only holds a version: several ranges of one key, as a read's pin sends.
    pub fn holds_a_version(&self) -> bool {
        self.ranges.len() > 1
            && self
                .ranges
                .first()
                .is_some_and(|first| self.ranges.iter().all(|range| range.first == first.first))
    }
}

/// Whether `key` is a key of the dictionary.
pub fn is_dictionary(key: &[u8]) -> bool {
    QuadKey::parse(key).is_ok_and(|(parsed, _)| parsed.keyspace() == Keyspace::Dictionary)
}

/// What a played partition does with a Scan whose first key starts with a scripted prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Script {
    /// Refuses it with this code and detail, sending no frame.
    Refuse(ErrorCode, u64),
    /// Takes it and sends nothing, the connection open: a partition that hung.
    Silent,
}

/// How a played partition answers a Closed other than with its closed epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClosedAnswer {
    /// Takes it and sends nothing, the connection open.
    Silent,
    /// Refuses it with this code and detail.
    Refuse(ErrorCode, u64),
    /// Answers with a frame of another command.
    NotClosed,
    /// Answers with a Closed whose body breaks its checksum.
    Corrupt,
}

/// One played partition: its table, its scripts and what it took, shared by its connections and the test.
#[derive(Debug)]
pub struct PlayedPartition {
    partition: u32,
    table: RefCell<BTreeMap<Vec<u8>, Vec<u8>>>,
    scripts: RefCell<Vec<(Vec<u8>, Script)>>,
    scans: RefCell<Vec<ScanSeen>>,
    closed_epoch: Cell<u64>,
    closed_answer: Cell<Option<ClosedAnswer>>,
    closed_asked: Cell<u32>,
    cancels: Cell<u32>,
    connections: Cell<u32>,
}

impl PlayedPartition {
    /// Partition `partition`, holding nothing, its epochs closed through [`PLAYED_EPOCH`].
    pub fn new(partition: u32) -> Rc<Self> {
        Rc::new(Self {
            partition,
            table: RefCell::default(),
            scripts: RefCell::default(),
            scans: RefCell::default(),
            closed_epoch: Cell::new(PLAYED_EPOCH),
            closed_answer: Cell::new(None),
            closed_asked: Cell::new(0),
            cancels: Cell::new(0),
            connections: Cell::new(0),
        })
    }

    /// Holds `value` under `key`.
    pub fn put(&self, key: Vec<u8>, value: Vec<u8>) {
        let _old = self.table.borrow_mut().insert(key, value);
    }

    /// A Scan whose first key starts with `prefix` does as `script` says.
    pub fn script(&self, prefix: Vec<u8>, script: Script) {
        self.scripts.borrow_mut().push((prefix, script));
    }

    /// The epoch a Closed is answered with.
    pub fn close_through(&self, epoch: u64) {
        self.closed_epoch.set(epoch);
    }

    /// Every Scan it took, in order.
    pub fn scans(&self) -> Vec<ScanSeen> {
        self.scans.borrow().clone()
    }

    /// The Scans it took that read for the read itself: those holding a version left out.
    pub fn read_scans(&self) -> Vec<ScanSeen> {
        self.scans
            .borrow()
            .iter()
            .filter(|scan| !scan.holds_a_version())
            .cloned()
            .collect()
    }

    /// Every Closed is answered as `answer` says.
    pub fn answer_closed_with(&self, answer: ClosedAnswer) {
        self.closed_answer.set(Some(answer));
    }

    /// How many Closed it answered.
    pub fn closed_asked(&self) -> u32 {
        self.closed_asked.get()
    }

    /// How many streams were cancelled.
    pub fn cancels(&self) -> u32 {
        self.cancels.get()
    }

    /// How many connections it took.
    pub fn connections(&self) -> u32 {
        self.connections.get()
    }

    /// Notes a connection taken.
    pub fn connected(&self) {
        self.connections.set(self.connections.get() + 1);
    }

    fn script_of(&self, key: &[u8]) -> Option<Script> {
        self.scripts
            .borrow()
            .iter()
            .find(|(prefix, _)| key.starts_with(prefix))
            .map(|(_, script)| script.clone())
    }

    /// The entries `range` reads, at most `limit` (0 for all), and whether a key of it remains past them.
    fn entries_in(&self, range: &RangeSeen, limit: u32) -> (Vec<KeyValue>, bool) {
        let tenant_end = range.first.get(..9).map(<[u8]>::to_vec).unwrap_or_default();
        let within = |key: &[u8]| {
            if range.past.is_empty() {
                key.starts_with(&tenant_end)
            } else {
                key < range.past.as_slice()
            }
        };
        let table = self.table.borrow();
        let mut found: Vec<KeyValue> = table
            .range(range.first.clone()..)
            .take_while(|(key, _)| within(key))
            .map(|(key, value)| {
                let resumed = !range.after.is_empty() && *key == range.first;
                let value = if resumed {
                    records_from(value, &range.after)
                } else {
                    value.clone()
                };
                (key.clone(), value)
            })
            .collect();
        let wanted = if limit == 0 {
            usize::MAX
        } else {
            limit as usize
        };
        let remains = found.len() > wanted;
        found.truncate(wanted);
        (found, remains)
    }
}

/// The object records of `value` from the object `after` on, itself included, as a list resumed there reads (`PROTOCOL.md` §13.13).
fn records_from(value: &[u8], after: &[u8]) -> Vec<u8> {
    let mut kept = ObjectRecordsBuilder::new();
    if let Ok(records) = ObjectRecords::read(value) {
        let mut statements: Vec<[u8; 16]> = Vec::new();
        records
            .iter()
            .skip_while(|record| record.encoding() != after)
            .for_each(|record| {
                statements.clear();
                statements.extend(record.statement_ids());
                kept.push(record.encoding(), &statements);
            });
    }
    kept.value().to_vec()
}

/// A frame of `body` on `stream`, flagged `LAST` when `last`.
pub fn frame_of<B: BodyBuilder>(
    cluster: u64,
    body: &B,
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
pub struct PartitionConnection {
    /// The partition it serves.
    pub played: Rc<PlayedPartition>,
    /// The cluster.
    pub cluster: u64,
    /// The frames it has yet to send, per stream, and whether the stream has room for its next: a stream's first frame goes at
    /// once, each next once the connection reports room for it, as a partition builds a Scan's frames.
    pub pushes: BTreeMap<u64, (VecDeque<Vec<u8>>, bool)>,
}

impl PartitionConnection {
    /// Queues the answer to `scan` on `stream`: a run of one-key dictionary ranges packed into frames as far as each holds them, every
    /// other range in a frame of its own.
    fn answer_scan(&mut self, stream: u64, scan: &ScanSeen) -> Result<(), ErrorReply> {
        // Each frame, and whether it answers one-key ranges of the dictionary alone, which a next such range may join.
        let mut frames: Vec<(Entries, bool)> = Vec::new();
        for (index, range) in scan.ranges.iter().enumerate() {
            let at = u32::try_from(index).unwrap_or(u32::MAX);
            let (found, remains) = self.played.entries_in(range, scan.limit);
            let packs = is_dictionary(&range.first) && !remains;
            if packs && let Some((open, true)) = frames.last_mut() {
                let size: usize = found
                    .iter()
                    .map(|(key, value)| key.len() + value.len() + 16)
                    .sum();
                if open.through() + 1 == at && open.frame_size() + size < FRAME_ROOM {
                    let held = std::mem::replace(open, Entries::new(0, EntriesEnd::Ended, 0));
                    let mut joined = held.with_through(at);
                    found
                        .iter()
                        .for_each(|(key, value)| joined.push(key, value));
                    *open = joined;
                    continue;
                }
            }
            let end = if remains {
                EntriesEnd::Limit
            } else {
                EntriesEnd::Ended
            };
            let mut entries = Entries::new(at, end, PLAYED_EPOCH).with_version(PLAYED_VERSION);
            found
                .iter()
                .for_each(|(key, value)| entries.push(key, value));
            frames.push((entries, packs));
        }
        let last = frames.len();
        let mut queued = VecDeque::with_capacity(last);
        for (index, (entries, _)) in frames.iter().enumerate() {
            queued.push_back(frame_of(self.cluster, entries, stream, index + 1 == last)?);
        }
        let _none = self.pushes.insert(stream, (queued, true));
        Ok(())
    }

    /// The answer to a Closed: the partition's closed epoch.
    fn answer_closed(
        &self,
        stream: u64,
        view: &ultragraphdb_protocol_wire::FrameView<'_>,
    ) -> Answer {
        self.played
            .closed_asked
            .set(self.played.closed_asked.get() + 1);
        let Ok(asked) = ClosedView::from_frame(view) else {
            return Answer::Refuse(ErrorReply::new(ErrorCode::Protocol, "a Closed"));
        };
        let partition = asked
            .partition(0)
            .map_or(self.played.partition, |held| held.partition());
        let reply = Closed::new(vec![ClosedPartition::new(
            partition,
            self.played.closed_epoch.get(),
        )]);
        let built = match self.played.closed_answer.get() {
            None => frame_of(self.cluster, &reply, stream, true),
            Some(ClosedAnswer::Silent) => return Answer::Open,
            Some(ClosedAnswer::Refuse(code, detail)) => {
                return Answer::Refuse(
                    ErrorReply::new(code, "a played refusal of a Closed").with_detail(detail),
                );
            }
            Some(ClosedAnswer::NotClosed) => {
                let other = Entries::new(0, EntriesEnd::Ended, PLAYED_EPOCH);
                frame_of(self.cluster, &other, stream, true)
            }
            Some(ClosedAnswer::Corrupt) => {
                frame_of(self.cluster, &reply, stream, true).map(|mut frame| {
                    if let Some(last) = frame.last_mut() {
                        *last ^= 0xFF;
                    }
                    frame
                })
            }
        };
        match built {
            Ok(replied) => Answer::Reply(replied),
            Err(refusal) => Answer::Refuse(refusal),
        }
    }
}

/// The Scan `view` holds, as the partition notes it.
fn seen_of(view: &ScanView<'_>) -> ScanSeen {
    ScanSeen {
        at: view.at(),
        read_version: view.read_version(),
        epoch: view.epoch(),
        limit: view.limit(),
        deadline_ms: view.deadline_ms(),
        terms: view.terms().len(),
        ranges: view
            .ranges()
            .map(|range| RangeSeen {
                first: range.first().to_vec(),
                past: range.past().to_vec(),
                after: range.after().to_vec(),
            })
            .collect(),
    }
}

impl Session for PartitionConnection {
    async fn answer(&mut self, stream: u64, frame: ReceivedFrame) -> Answer {
        let view = match frame.view() {
            Ok(view) => view,
            Err(rule) => return Answer::Refuse(ErrorReply::protocol(&rule)),
        };
        if view.command() == Command::Closed {
            return self.answer_closed(stream, &view);
        }
        let scan = match ScanView::from_frame(&view) {
            Ok(scan) => seen_of(&scan),
            Err(rule) => return Answer::Refuse(ErrorReply::protocol(&rule)),
        };
        self.played.scans.borrow_mut().push(scan.clone());
        let script = scan
            .ranges
            .first()
            .and_then(|range| self.played.script_of(&range.first));
        match script {
            Some(Script::Refuse(code, detail)) => {
                Answer::Refuse(ErrorReply::new(code, "a played refusal").with_detail(detail))
            }
            Some(Script::Silent) => Answer::Open,
            None => match self.answer_scan(stream, &scan) {
                Ok(()) => Answer::Open,
                Err(refusal) => Answer::Refuse(refusal),
            },
        }
    }

    async fn next_push(&mut self) -> (u64, Vec<u8>) {
        loop {
            let free = self
                .pushes
                .iter_mut()
                .find(|(_, (queued, room))| *room && !queued.is_empty());
            if let Some((stream, (queued, room))) = free
                && let Some(frame) = queued.pop_front()
            {
                *room = false;
                let stream = *stream;
                if queued.is_empty() {
                    drop(self.pushes.remove(&stream));
                }
                return (stream, frame);
            }
            compio::time::sleep(LOOK).await;
        }
    }

    fn ready(&mut self, stream: u64) {
        if let Some((_, room)) = self.pushes.get_mut(&stream) {
            *room = true;
        }
    }

    fn cancelled(&mut self, stream: u64) {
        self.played.cancels.set(self.played.cancels.get() + 1);
        drop(self.pushes.remove(&stream));
    }
}
