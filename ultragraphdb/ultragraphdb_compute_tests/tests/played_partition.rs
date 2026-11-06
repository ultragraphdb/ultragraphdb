/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Partitions played by the test: real frames over a real connection, for what a real storage process cannot be made to hold or
//! answer. Each answers a Scan from a table of keys, or as a scenario scripts it.

use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::rc::Rc;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use compio::runtime::{Runtime, spawn};
use ultragraphdb_common_role::{
    Answer, ROLE_ITERATIONS, RoleCredentials, RoleSecret, Server, Session, listen, serve_connection,
};
use ultragraphdb_protocol_transport::{Endpoint, ReceivedFrame};
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Closed, ClosedPartition, ClosedView, Command, Entries, EntriesEnd,
    ErrorCode, ErrorReply, FLAG_LAST, Header, ObjectRecords, ObjectRecordsBuilder, ScanView,
};
use ultragraphdb_tests::{CLUSTER, SECRET, scratch};
use ultragraphdb_tool_bdd::TestResult;

/// A version every played partition answers at.
const VERSION: u64 = (9 << 22) | ((1 << 22) - 1);
const EPOCH: u64 = 9;

/// What a played partition does with the next Scan naming a key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Script {
    /// Answers from its table.
    Table,
    /// Refuses with this code and detail.
    Refuse(ErrorCode, u64),
    /// Answers a frame breaking the Scan's order, which closes the connection.
    BreakOrder,
    /// Answers its first range's first key but its last object record, to be continued, then refuses with this code and detail; a
    /// Scan going on in that list is refused so at once, as by a partition that stopped answering.
    ThenRefuse(ErrorCode, u64),
    /// Answers its first range's first key, the Scan not ended, then refuses as stalled, once.
    StallPastKey,
    /// Answers the first of its first key's object records, to be continued, then refuses as stalled, once.
    StallInList,
    /// Answers from its table after this long, holding up this connection.
    After(Duration),
    /// Takes the Scan and sends nothing, the connection open: a partition that hung.
    Silent,
}

/// A range a played partition reads.
#[derive(Debug)]
struct PlayedRange {
    first: Vec<u8>,
    past: Vec<u8>,
    after: Vec<u8>,
}

/// The tables of one played partition, shared with the test.
#[derive(Debug, Default)]
pub struct PlayedTables {
    table: Mutex<BTreeMap<Vec<u8>, Vec<u8>>>,
    script: Mutex<Vec<(Vec<u8>, Script)>>,
    asked: Mutex<u32>,
    connections: Mutex<u32>,
    credits: Mutex<u32>,
}

impl PlayedTables {
    /// Holds `value` under `key`.
    pub fn put(&self, key: Vec<u8>, value: Vec<u8>) {
        let _old = self
            .table
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(key, value);
    }

    /// A Scan whose first key is `key` does as `script` says.
    pub fn script(&self, key: Vec<u8>, script: Script) {
        self.script
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((key, script));
    }

    /// Scans of `key` answer from the table again.
    pub fn unscript(&self, key: &[u8]) {
        self.script
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|(held, _)| held != key);
    }

    /// How many Scans it answered.
    pub fn asked(&self) -> u32 {
        *self.asked.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// How many connections it took.
    pub fn connections(&self) -> u32 {
        *self
            .connections
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// How many frames of credit it was granted.
    pub fn credits(&self) -> u32 {
        *self.credits.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn entries_in(&self, range: &PlayedRange) -> Vec<(Vec<u8>, Vec<u8>)> {
        self.table
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .range(range.first.clone()..)
            .take_while(|(key, _)| range.past.is_empty() || **key < range.past)
            .map(|(key, value)| {
                let resumed = !range.after.is_empty() && *key == range.first;
                let value = if resumed {
                    records_from(value, &range.after)
                } else {
                    value.clone()
                };
                (key.clone(), value)
            })
            .collect()
    }

    /// What a Scan first naming `key` does: the script of the longest key it starts with, a stall's only the once.
    fn script_of(&self, key: &[u8]) -> Script {
        let mut scripts = self.script.lock().unwrap_or_else(PoisonError::into_inner);
        let found = scripts.iter().position(|(held, _)| key.starts_with(held));
        let Some(found) = found else {
            return Script::Table;
        };
        let script = scripts
            .get(found)
            .map_or(Script::Table, |(_, script)| script.clone());
        if matches!(script, Script::StallPastKey | Script::StallInList) {
            let _fired = scripts.remove(found);
        }
        script
    }
}

/// The object records of `value` from the object `after` on, as a list resumed there reads.
fn records_from(value: &[u8], after: &[u8]) -> Vec<u8> {
    let mut kept = ObjectRecordsBuilder::new();
    if let Ok(records) = ObjectRecords::read(value) {
        records
            .iter()
            .skip_while(|record| record.encoding() != after)
            .for_each(|record| kept.push(record.encoding(), &[]));
    }
    kept.value().to_vec()
}

/// A frame of `body` on `stream`.
fn frame_of<B: BodyBuilder>(body: &B, stream: u64, last: bool) -> Result<Vec<u8>, ErrorReply> {
    let header = Header::new(CLUSTER, B::COMMAND).with_stream(stream);
    let header = if last {
        header.with_flags(FLAG_LAST)
    } else {
        header
    };
    body.frame(header)
        .map_err(|rule| ErrorReply::protocol(&rule))
}

/// The frames a played connection has yet to send.
type Pushes = Rc<RefCell<VecDeque<(u64, Vec<u8>)>>>;

/// One connection at a played partition.
#[derive(Debug)]
struct PartitionConnection {
    tables: Arc<PlayedTables>,
    pushes: Pushes,
}

impl PartitionConnection {
    /// Queues the answer to a Scan of `ranges`.
    fn answer_scan(&self, stream: u64, ranges: &[PlayedRange]) -> Result<(), ErrorReply> {
        let count = ranges.len();
        for (index, range) in ranges.iter().enumerate() {
            let last = index + 1 == count;
            let mut entries = Entries::new(
                u32::try_from(index).unwrap_or(u32::MAX),
                EntriesEnd::Ended,
                EPOCH,
            )
            .with_version(VERSION);
            for (key, value) in self.tables.entries_in(range) {
                entries.push(&key, &value);
            }
            let frame = frame_of(&entries, stream, last)?;
            self.pushes.borrow_mut().push_back((stream, frame));
        }
        Ok(())
    }

    /// Queues `entries`, more of the first range to follow, then a refusal of `code` and `detail`.
    fn entries_then(
        &self,
        stream: u64,
        entries: &Entries,
        code: ErrorCode,
        detail: u64,
    ) -> Result<(), ErrorReply> {
        let frame = frame_of(entries, stream, false)?;
        self.pushes.borrow_mut().push_back((stream, frame));
        let refusal = ErrorReply::new(code, "a played refusal").with_detail(detail);
        let refused = frame_of(&refusal, stream, true)?;
        self.pushes.borrow_mut().push_back((stream, refused));
        Ok(())
    }

    /// Queues a frame of the first key of the first of `ranges` but its last object record, to be continued, then a refusal.
    fn then_refuse(
        &self,
        stream: u64,
        ranges: &[PlayedRange],
        code: ErrorCode,
        detail: u64,
    ) -> Result<(), ErrorReply> {
        if ranges.first().is_some_and(|range| !range.after.is_empty()) {
            return Err(ErrorReply::new(code, "a played refusal").with_detail(detail));
        }
        let mut entries = Entries::new(0, EntriesEnd::More, EPOCH).with_version(VERSION);
        let held = ranges
            .first()
            .and_then(|range| self.tables.entries_in(range).into_iter().next());
        if let Some((key, value)) = held {
            let mut kept = ObjectRecordsBuilder::new();
            if let Ok(records) = ObjectRecords::read(&value) {
                let all: Vec<_> = records.iter().collect();
                all.iter()
                    .take(all.len().saturating_sub(1))
                    .for_each(|record| kept.push(record.encoding(), &[]));
            }
            entries.push(&key, kept.value());
            entries = entries.continued();
        }
        self.entries_then(stream, &entries, code, detail)
    }

    /// Queues a frame of the first of `ranges`' first key, whole or by its first object record, more to follow, then a stall.
    fn stall(&self, stream: u64, ranges: &[PlayedRange], in_list: bool) -> Result<(), ErrorReply> {
        let mut entries = Entries::new(0, EntriesEnd::More, EPOCH).with_version(VERSION);
        let held = ranges
            .first()
            .and_then(|range| self.tables.entries_in(range).into_iter().next());
        if let Some((key, value)) = held {
            if in_list {
                let mut first = ObjectRecordsBuilder::new();
                if let Some(record) = ObjectRecords::read(&value)
                    .ok()
                    .and_then(|records| records.iter().next())
                {
                    first.push(record.encoding(), &[]);
                }
                entries.push(&key, first.value());
                entries = entries.continued();
            } else {
                entries.push(&key, &value);
            }
        }
        self.entries_then(stream, &entries, ErrorCode::TimedOut, 1)
    }

    /// Queues a frame no answer to `ranges` may hold: one naming a range past the last.
    fn break_order(&self, stream: u64, ranges: usize) -> Result<(), ErrorReply> {
        let entries = Entries::new(
            u32::try_from(ranges).unwrap_or(u32::MAX),
            EntriesEnd::Ended,
            EPOCH,
        )
        .with_version(VERSION);
        let frame = frame_of(&entries, stream, true)?;
        self.pushes.borrow_mut().push_back((stream, frame));
        Ok(())
    }
}

impl Session for PartitionConnection {
    fn credited(&mut self, _stream: u64, frames: u32) {
        *self
            .tables
            .credits
            .lock()
            .unwrap_or_else(PoisonError::into_inner) += frames;
    }

    async fn answer(&mut self, stream: u64, frame: ReceivedFrame) -> Answer {
        let view = match frame.view() {
            Ok(view) => view,
            Err(rule) => return Answer::Refuse(ErrorReply::protocol(&rule)),
        };
        let command = view.command();
        if command == Command::Closed {
            let Ok(asked) = ClosedView::from_frame(&view) else {
                return Answer::Refuse(ErrorReply::new(ErrorCode::Protocol, "a Closed"));
            };
            let partition = asked.partition(0).map_or(1, |held| held.partition());
            let reply = Closed::new(vec![ClosedPartition::new(partition, EPOCH)]);
            return match frame_of(&reply, stream, true) {
                Ok(replied) => Answer::Reply(replied),
                Err(refusal) => Answer::Refuse(refusal),
            };
        }
        let scan = match ScanView::from_frame(&view) {
            Ok(scan) => scan,
            Err(rule) => {
                return Answer::Refuse(ErrorReply::new(
                    ErrorCode::Protocol,
                    format!("command {} is not played: {rule}", command.number()),
                ));
            }
        };
        *self
            .tables
            .asked
            .lock()
            .unwrap_or_else(PoisonError::into_inner) += 1;
        let ranges: Vec<PlayedRange> = scan
            .ranges()
            .map(|range| PlayedRange {
                first: range.first().to_vec(),
                past: range.past().to_vec(),
                after: range.after().to_vec(),
            })
            .collect();
        let script = ranges
            .first()
            .map_or(Script::Table, |range| self.tables.script_of(&range.first));
        let answered = match script {
            Script::Table => self.answer_scan(stream, &ranges),
            Script::Refuse(code, detail) => {
                return Answer::Refuse(
                    ErrorReply::new(code, "a played refusal").with_detail(detail),
                );
            }
            Script::BreakOrder => self.break_order(stream, ranges.len()),
            Script::ThenRefuse(code, detail) => self.then_refuse(stream, &ranges, code, detail),
            Script::StallPastKey => self.stall(stream, &ranges, false),
            Script::StallInList => self.stall(stream, &ranges, true),
            Script::Silent => Ok(()),
            Script::After(wait) => {
                compio::time::sleep(wait).await;
                self.answer_scan(stream, &ranges)
            }
        };
        match answered {
            Ok(()) => Answer::Open,
            Err(refusal) => Answer::Refuse(refusal),
        }
    }

    async fn next_push(&mut self) -> (u64, Vec<u8>) {
        loop {
            let next = self.pushes.borrow_mut().pop_front();
            if let Some(pushed) = next {
                return pushed;
            }
            compio::time::sleep(Duration::from_millis(1)).await;
        }
    }
}

/// Serves a played partition at the TCP port `port`, on a thread of the test's own for the life of the test process: its tables.
#[expect(
    clippy::disallowed_methods,
    reason = "a played partition answers beside the scenarios, on a thread of the test's own"
)]
pub fn play_partition(port: u32) -> TestResult<Arc<PlayedTables>> {
    let file = scratch(&format!("played-partition-secret-{port}"));
    std::fs::write(&file, SECRET)?;
    let held: RoleCredentials = RoleSecret::read(&file.display().to_string())?.credentials()?;
    let server =
        Server::new(CLUSTER, "storage", Endpoint::Partition)?.with_iterations(ROLE_ITERATIONS);
    let tables = Arc::new(PlayedTables::default());
    let serving = Arc::clone(&tables);
    drop(std::thread::spawn(move || -> TestResult {
        Runtime::new()?.block_on(async move {
            let listener = listen(&format!("tcp://127.0.0.1:{port}")).await?;
            while let Ok(carrier) = listener.accept().await {
                *serving
                    .connections
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner) += 1;
                let (server, held) = (server.clone(), held.clone());
                let connection = PartitionConnection {
                    tables: Arc::clone(&serving),
                    pushes: Rc::default(),
                };
                spawn(async move {
                    serve_connection(carrier, &server, held, connection).await;
                })
                .detach();
            }
            Ok(())
        })
    }));
    Ok(tables)
}
