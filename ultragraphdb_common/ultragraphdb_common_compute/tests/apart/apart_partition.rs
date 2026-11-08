/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A partition the test plays over real connections: it answers each Scan from a table of keys as storage's read kernel does
//! (`PROTOCOL.md` §13.13, §13.14), or as a test scripts it, and records what it was asked.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;
use std::time::Duration;

use futures::StreamExt;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use ultragraphdb_common_role::{Answer, Session};
use ultragraphdb_protocol_transport::ReceivedFrame;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Closed, ClosedPartition, ClosedView, Command, Entries, EntriesEnd,
    ErrorCode, ErrorReply, FLAG_LAST, Header, ObjectRecords, ObjectRecordsBuilder, ScanView,
};
use ultragraphdb_tool_bdd::{TestError, TestResult};

/// The version bound of epoch 9, which every frame answers at.
pub const VERSION: u64 = (9 << 22) | ((1 << 22) - 1);
const EPOCH: u64 = 9;

/// A key's first bytes naming its keyspace and tenant (`PROTOCOL.md` §13.13).
const TENANT_PREFIX: usize = 9;

/// How long a test waits for a Scan to be asked, and how often it looks.
const ASKED_WAIT: Duration = Duration::from_secs(5);
const LOOK: Duration = Duration::from_millis(2);

/// What the partition does with a Scan whose first key starts with a scripted prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Script {
    /// Takes the Scan and sends nothing, its connection open: a partition that hung.
    Silent,
    /// Refuses at once with this code.
    Refuse(ErrorCode),
    /// Sends its first range's first key whole, more of the range to follow, then refuses as stalled.
    StallPastKey,
    /// Sends the first object record of its first range's first key, the list to continue, then refuses as stalled.
    StallInList,
    /// Answers with a frame naming a range past the Scan's last, which no answer may hold.
    BreakOrder,
}

/// One Scan the partition was asked, a pin's left out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeenScan {
    pub ranges: usize,
    pub limit: u32,
    pub first: Vec<u8>,
}

/// A range's entries, each key and its value.
type Held = Vec<(Vec<u8>, Vec<u8>)>;

/// One range of a Scan, as asked.
#[derive(Debug)]
struct Asked {
    partition: u32,
    first: Vec<u8>,
    past: Vec<u8>,
    after: Vec<u8>,
}

/// A played partition's table and what it was asked, shared with the test.
#[derive(Debug)]
pub struct PlayedPartition {
    partition: u32,
    table: RefCell<BTreeMap<Vec<u8>, Vec<u8>>>,
    /// A key prefix, what a Scan starting there gets, and how many more times: 0 for always.
    scripts: RefCell<Vec<(Vec<u8>, Script, u32)>>,
    per_key: Cell<bool>,
    scans: RefCell<Vec<SeenScan>>,
    connections: Cell<u32>,
}

impl PlayedPartition {
    /// Partition `partition`, holding nothing.
    pub fn new(partition: u32) -> Rc<Self> {
        Rc::new(Self {
            partition,
            table: RefCell::default(),
            scripts: RefCell::default(),
            per_key: Cell::new(false),
            scans: RefCell::default(),
            connections: Cell::new(0),
        })
    }

    /// Holds `value` under `key`.
    pub fn put(&self, key: Vec<u8>, value: Vec<u8>) {
        let _old = self.table.borrow_mut().insert(key, value);
    }

    /// A Scan whose first key starts with `prefix` gets `script`, `times` times, or always for 0.
    pub fn script(&self, prefix: Vec<u8>, script: Script, times: u32) {
        self.scripts.borrow_mut().push((prefix, script, times));
    }

    /// Answers each key in a frame of its own, more of its range to follow, and each range's end in a frame holding no entry.
    pub fn frame_per_key(&self) {
        self.per_key.set(true);
    }

    /// The Scans asked so far.
    pub fn scans(&self) -> Vec<SeenScan> {
        self.scans.borrow().clone()
    }

    /// How many connections it took.
    pub fn connections(&self) -> u32 {
        self.connections.get()
    }

    /// Waits until it has been asked a Scan first naming a key that starts with `prefix`, failing past [`ASKED_WAIT`].
    pub async fn until_asked(&self, prefix: &[u8]) -> TestResult {
        let asked = async {
            while !self
                .scans
                .borrow()
                .iter()
                .any(|scan| scan.first.starts_with(prefix))
            {
                compio::time::sleep(LOOK).await;
            }
        };
        compio::time::timeout(ASKED_WAIT, asked)
            .await
            .map_err(|_| TestError::new(format!("no Scan was asked within {ASKED_WAIT:?}")))
    }

    /// A new connection's session.
    pub fn connection(self: &Rc<Self>) -> PartitionConnection {
        self.connections
            .set(self.connections.get().saturating_add(1));
        let (pushes, pushed) = unbounded();
        PartitionConnection {
            played: Rc::clone(self),
            pushes,
            pushed,
        }
    }

    /// The script of a Scan first naming `key`, counted as fired.
    fn script_of(&self, key: &[u8]) -> Option<Script> {
        let mut scripts = self.scripts.borrow_mut();
        let found = scripts
            .iter()
            .position(|(prefix, _, _)| key.starts_with(prefix))?;
        let (_, script, times) = scripts.get_mut(found)?;
        let fired = *script;
        match *times {
            0 => {}
            1 => drop(scripts.remove(found)),
            more => *times = more - 1,
        }
        Some(fired)
    }

    /// The entries of `range`, at most `limit` of them for a nonzero `limit`, and whether a key of it remains.
    fn entries_in(&self, range: &Asked, limit: u32) -> (Held, bool) {
        let past = if range.past.is_empty() {
            past_tenant(&range.first)
        } else {
            range.past.clone()
        };
        let table = self.table.borrow();
        let mut held: Held = table
            .range(range.first.clone()..)
            .take_while(|(key, _)| past.is_empty() || **key < past)
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
        let wanted = limit as usize;
        let more = limit != 0 && held.len() > wanted;
        if more {
            held.truncate(wanted);
        }
        (held, more)
    }
}

/// The key past every key of `first`'s keyspace and tenant.
fn past_tenant(first: &[u8]) -> Vec<u8> {
    let mut past: Vec<u8> = first.iter().take(TENANT_PREFIX).copied().collect();
    while let Some(last) = past.pop() {
        if let Some(raised) = last.checked_add(1) {
            past.push(raised);
            break;
        }
    }
    past
}

/// The object records of `value` from the object `after` on, in the list's order, as a list resumed there reads.
fn records_from(value: &[u8], after: &[u8]) -> Vec<u8> {
    let mut kept = ObjectRecordsBuilder::new();
    if let Ok(records) = ObjectRecords::read(value) {
        records
            .iter()
            .filter(|record| record.encoding() >= after)
            .for_each(|record| kept.push(record.encoding(), &[]));
    }
    kept.value().to_vec()
}

/// The first object record of the list `value`.
fn first_record(value: &[u8]) -> Vec<u8> {
    let mut first = ObjectRecordsBuilder::new();
    if let Some(record) = ObjectRecords::read(value)
        .ok()
        .and_then(|records| records.iter().next())
    {
        first.push(record.encoding(), &[]);
    }
    first.value().to_vec()
}

/// A frame of `body` on `stream`.
fn frame_of<B: BodyBuilder>(body: &B, stream: u64, last: bool) -> Result<Vec<u8>, ErrorReply> {
    let header = Header::new(super::apart_cluster::CLUSTER, B::COMMAND).with_stream(stream);
    let header = if last {
        header.with_flags(FLAG_LAST)
    } else {
        header
    };
    body.frame(header)
        .map_err(|rule| ErrorReply::protocol(&rule))
}

/// Error 1 naming what the partition refuses before any frame.
fn protocol(why: String) -> ErrorReply {
    ErrorReply::new(ErrorCode::Protocol, why)
}

/// One connection at a played partition.
#[derive(Debug)]
pub struct PartitionConnection {
    played: Rc<PlayedPartition>,
    pushes: UnboundedSender<(u64, Vec<u8>)>,
    pushed: UnboundedReceiver<(u64, Vec<u8>)>,
}

impl PartitionConnection {
    fn push(&self, stream: u64, frame: Vec<u8>) {
        // The session holds the receiver as long as the sender.
        drop(self.pushes.unbounded_send((stream, frame)));
    }

    /// The ranges of `scan`, refused as a partition refuses them before any frame.
    fn ranges_of(&self, scan: &ScanView<'_>) -> Result<Vec<Asked>, ErrorReply> {
        let ranges: Vec<Asked> = scan
            .ranges()
            .map(|range| Asked {
                partition: range.partition(),
                first: range.first().to_vec(),
                past: range.past().to_vec(),
                after: range.after().to_vec(),
            })
            .collect();
        if ranges.is_empty() {
            return Err(protocol("a Scan of no range".to_string()));
        }
        for (index, range) in ranges.iter().enumerate() {
            if range.partition != self.played.partition {
                return Err(protocol(format!(
                    "range {index} reads partition {}, which this address does not serve",
                    range.partition
                )));
            }
            if !range.past.is_empty() && range.past <= range.first {
                return Err(protocol(format!(
                    "range {index}'s past key is not above its first"
                )));
            }
        }
        Ok(ranges)
    }

    /// Queues the answer to `ranges` from the table.
    fn answer_scan(&self, stream: u64, ranges: &[Asked], limit: u32) -> Result<(), ErrorReply> {
        let count = ranges.len();
        for (index, range) in ranges.iter().enumerate() {
            let last = index + 1 == count;
            let at = u32::try_from(index).unwrap_or(u32::MAX);
            let (held, more) = self.played.entries_in(range, limit);
            let end = if more {
                EntriesEnd::Limit
            } else {
                EntriesEnd::Ended
            };
            if self.played.per_key.get() {
                for (key, value) in &held {
                    let mut one = Entries::new(at, EntriesEnd::More, EPOCH).with_version(VERSION);
                    one.push(key, value);
                    self.push(stream, frame_of(&one, stream, false)?);
                }
                let ending = Entries::new(at, end, EPOCH).with_version(VERSION);
                self.push(stream, frame_of(&ending, stream, last)?);
            } else {
                let mut entries = Entries::new(at, end, EPOCH).with_version(VERSION);
                held.iter()
                    .for_each(|(key, value)| entries.push(key, value));
                self.push(stream, frame_of(&entries, stream, last)?);
            }
        }
        Ok(())
    }

    /// Queues its first range's first key, whole or by its first object record, then a refusal as stalled.
    fn stall(&self, stream: u64, ranges: &[Asked], in_list: bool) -> Result<(), ErrorReply> {
        let mut entries = Entries::new(0, EntriesEnd::More, EPOCH).with_version(VERSION);
        let held = ranges
            .first()
            .and_then(|range| self.played.entries_in(range, 0).0.into_iter().next());
        if let Some((key, value)) = held {
            if in_list {
                entries.push(&key, &first_record(&value));
                entries = entries.continued();
            } else {
                entries.push(&key, &value);
            }
        }
        self.push(stream, frame_of(&entries, stream, false)?);
        let stalled = ErrorReply::new(ErrorCode::TimedOut, "a played Scan stalled")
            .with_detail(u64::from(self.played.partition));
        self.push(stream, frame_of(&stalled, stream, true)?);
        Ok(())
    }

    /// The answer to a Scan.
    fn scan(&self, stream: u64, scan: &ScanView<'_>) -> Result<(), ErrorReply> {
        let ranges = self.ranges_of(scan)?;
        let first = ranges
            .first()
            .map(|range| range.first.clone())
            .unwrap_or_default();
        // A pin holds a version by several ranges of one key, which is none of what the read asks.
        let pin = ranges.len() > 1 && ranges.iter().all(|range| range.first == first);
        if !pin {
            self.played.scans.borrow_mut().push(SeenScan {
                ranges: ranges.len(),
                limit: scan.limit(),
                first: first.clone(),
            });
        }
        match self.played.script_of(&first) {
            None => self.answer_scan(stream, &ranges, scan.limit()),
            Some(Script::Silent) => Ok(()),
            Some(Script::Refuse(code)) => Err(ErrorReply::new(code, "a played refusal")
                .with_detail(u64::from(self.played.partition))),
            Some(Script::StallPastKey) => self.stall(stream, &ranges, false),
            Some(Script::StallInList) => self.stall(stream, &ranges, true),
            Some(Script::BreakOrder) => {
                let past_last = u32::try_from(ranges.len()).unwrap_or(u32::MAX);
                let entries =
                    Entries::new(past_last, EntriesEnd::Ended, EPOCH).with_version(VERSION);
                self.push(stream, frame_of(&entries, stream, true)?);
                Ok(())
            }
        }
    }
}

impl Session for PartitionConnection {
    async fn answer(&mut self, stream: u64, frame: ReceivedFrame) -> Answer {
        let view = match frame.view() {
            Ok(view) => view,
            Err(rule) => return Answer::Refuse(ErrorReply::protocol(&rule)),
        };
        if view.command() == Command::Closed {
            let asked = ClosedView::from_frame(&view)
                .ok()
                .and_then(|closed| closed.partition(0))
                .map_or(self.played.partition, |held| held.partition());
            let reply = Closed::new(vec![ClosedPartition::new(asked, EPOCH)]);
            return match frame_of(&reply, stream, true) {
                Ok(replied) => Answer::Reply(replied),
                Err(refusal) => Answer::Refuse(refusal),
            };
        }
        let answered = ScanView::from_frame(&view)
            .map_err(|rule| ErrorReply::protocol(&rule))
            .and_then(|scan| self.scan(stream, &scan));
        match answered {
            Ok(()) => Answer::Open,
            Err(refusal) => Answer::Refuse(refusal),
        }
    }

    async fn next_push(&mut self) -> (u64, Vec<u8>) {
        match self.pushed.next().await {
            Some(pushed) => pushed,
            // The session holds a sender, so its channel never ends while it waits here.
            None => std::future::pending().await,
        }
    }
}
