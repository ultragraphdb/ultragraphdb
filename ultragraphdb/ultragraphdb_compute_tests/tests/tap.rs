/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What compute dials in place of a partition: every byte goes on to the real partition, and what compute asks and the first
//! version each Scan answered is written down, so a scenario reads the Scans a real read sent. A Scan that only holds a version is
//! written down apart, with its frames and its Cancel: what a scenario asserts of a read is what the read itself asks.

use std::collections::HashSet;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use ultragraphdb_core_keys::{Keyspace, key_prefix};
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Command, Credit, EntriesView, FrameView, HEADER_SIZE, Header,
    HeaderView, MAX_FRAME_SIZE, ScanAt, ScanView,
};
use ultragraphdb_tests::CLUSTER;
use ultragraphdb_tool_bdd::TestResult;

/// The longest a tap holds a paused Scan, in milliseconds.
const PAUSE_MAX_MS: u32 = 5_000;

/// A Scan compute sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanSeen {
    pub partition: u32,
    pub connection: usize,
    pub stream: u64,
    pub at: ScanAt,
    pub read_version: u64,
    pub epoch: u64,
    pub limit: u32,
    pub ranges: Vec<RangeSeen>,
}

/// A range of a Scan compute sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RangeSeen {
    pub first: Vec<u8>,
    pub past: Vec<u8>,
    pub after: Vec<u8>,
}

/// What the taps of one cluster saw.
#[derive(Debug, Default)]
pub struct TapLog {
    scans: Mutex<Vec<ScanSeen>>,
    versions: Mutex<Vec<(u32, u64)>>,
    pause_after: AtomicUsize,
    until_next_frame: AtomicBool,
    connections: AtomicUsize,
    cancels: AtomicUsize,
    pins: Mutex<Vec<u64>>,
    pin_streams: Mutex<HashSet<(usize, u64)>>,
    overgrant_lists: AtomicUsize,
    ended: Mutex<HashSet<usize>>,
}

impl TapLog {
    /// Forgets all it saw and stops pausing.
    pub fn clear(&self) {
        self.scans_held().clear();
        self.versions_held().clear();
        self.go_on();
        self.cancels.store(0, Ordering::Release);
        self.pins_held().clear();
        self.overgrant_lists.store(0, Ordering::Release);
    }

    fn pins_held(&self) -> std::sync::MutexGuard<'_, Vec<u64>> {
        self.pins.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn pin_streams_held(&self) -> std::sync::MutexGuard<'_, HashSet<(usize, u64)>> {
        self.pin_streams
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Whether compute's side of `connection` has closed.
    pub fn has_ended(&self, connection: usize) -> bool {
        self.ended
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains(&connection)
    }

    /// The versions of the Scans that only hold one, in the order they went.
    pub fn pins(&self) -> Vec<u64> {
        self.pins_held().clone()
    }

    /// Grants the partition `frames` frames compute never granted on the stream of each Scan of a list, past the Scans now seen.
    pub fn overgrant_lists(&self, frames: usize) {
        self.overgrant_lists.store(frames, Ordering::Release);
    }

    fn scans_held(&self) -> std::sync::MutexGuard<'_, Vec<ScanSeen>> {
        self.scans.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn versions_held(&self) -> std::sync::MutexGuard<'_, Vec<(u32, u64)>> {
        self.versions.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The Scans seen, in the order they went.
    pub fn scans(&self) -> Vec<ScanSeen> {
        self.scans_held().clone()
    }

    /// The Scans seen of `partition`.
    pub fn scans_of(&self, partition: u32) -> Vec<ScanSeen> {
        self.scans()
            .into_iter()
            .filter(|scan| scan.partition == partition)
            .collect()
    }

    /// The partitions asked, ascending.
    pub fn partitions(&self) -> Vec<u32> {
        let mut asked: Vec<u32> = self.scans().iter().map(|scan| scan.partition).collect();
        asked.sort_unstable();
        asked.dedup();
        asked
    }

    /// The version each partition's frames named, in the order they came.
    pub fn versions(&self) -> Vec<(u32, u64)> {
        self.versions_held().clone()
    }

    /// Holds back each Scan past the first `scans`.
    pub fn pause_after(&self, scans: usize) {
        self.pause_after.store(scans, Ordering::Release);
    }

    /// The Cancels compute sent, each written down as it goes on to its partition, after every frame before it.
    pub fn cancels(&self) -> usize {
        self.cancels.load(Ordering::Acquire)
    }

    /// Holds back each Scan past the first `scans` until compute has sent its next frame on the connection, which then goes on
    /// right behind the Scan: compute's Cancel of a Scan it holds reaches the partition after that Scan, whatever the two take.
    pub fn pause_until_next_frame(&self, scans: usize) {
        self.until_next_frame.store(true, Ordering::Release);
        self.pause_after(scans);
    }

    /// Lets the Scans held back go.
    pub fn go_on(&self) {
        self.pause_after.store(0, Ordering::Release);
        self.until_next_frame.store(false, Ordering::Release);
    }
}

/// Reads one whole frame of `from`; `None` once it closed.
fn read_frame(from: &mut TcpStream) -> Option<Vec<u8>> {
    let mut frame = vec![0u8; HEADER_SIZE];
    from.read_exact(&mut frame).ok()?;
    let size = HeaderView::new(&frame).ok()?.size() as usize;
    frame.resize(size.max(HEADER_SIZE), 0);
    from.read_exact(frame.get_mut(HEADER_SIZE..)?).ok()?;
    Some(frame)
}

/// Whether `ranges` are a Scan holding a version: several of one key of the referrer sets' tenant prefix, which no set stands at.
fn holds_a_version(ranges: &[RangeSeen]) -> bool {
    let Some(first) = ranges.first() else {
        return false;
    };
    ranges.len() > 1
        && first.first.first().copied() == Some(Keyspace::Referrers.byte())
        && first.first.len() == key_prefix(Keyspace::Referrers, false, 0, None, None).len()
        && ranges.iter().all(|range| range == first)
}

/// Whether compute has sent `compute` anything past the frame just read; a millisecond's wait when it has not.
fn frame_behind(compute: &TcpStream) -> bool {
    drop(compute.set_read_timeout(Some(Duration::from_millis(1))));
    let behind = compute.peek(&mut [0u8]).is_ok_and(|read| read > 0);
    drop(compute.set_read_timeout(None));
    behind
}

/// Writes down what `frame`, from compute, asks: what the tap asks of the partition on its own, when anything.
fn saw_request(
    log: &TapLog,
    partition: u32,
    (connection, compute): (usize, &TcpStream),
    frame: &[u8],
) -> Option<Vec<u8>> {
    let view = FrameView::validate_client(frame, CLUSTER, MAX_FRAME_SIZE).ok()?;
    let stream = view.header().stream();
    if view.command() == Command::Cancel && !log.pin_streams_held().contains(&(connection, stream))
    {
        let _before = log.cancels.fetch_add(1, Ordering::AcqRel);
    }
    if view.command() != Command::Scan {
        return None;
    }
    let scan = ScanView::from_frame(&view).ok()?;
    let ranges: Vec<RangeSeen> = scan
        .ranges()
        .map(|range| RangeSeen {
            first: range.first().to_vec(),
            past: range.past().to_vec(),
            after: range.after().to_vec(),
        })
        .collect();
    if holds_a_version(&ranges) {
        let _new = log.pin_streams_held().insert((connection, stream));
        log.pins_held().push(scan.read_version());
        return None;
    }
    let grant = overgrant(log, stream, &ranges);
    let seen = ScanSeen {
        partition,
        connection,
        stream,
        at: scan.at(),
        read_version: scan.read_version(),
        epoch: scan.epoch(),
        limit: scan.limit(),
        ranges,
    };
    let scans = {
        let mut held = log.scans_held();
        held.push(seen);
        held.len()
    };
    let pause = log.pause_after.load(Ordering::Acquire);
    if pause != 0 && scans > pause {
        // A scenario that failed before letting go holds its Scans no longer than a role would wait on one.
        for _ in 0..PAUSE_MAX_MS {
            if log.pause_after.load(Ordering::Acquire) == 0 {
                break;
            }
            if !log.until_next_frame.load(Ordering::Acquire) {
                std::thread::sleep(Duration::from_millis(1));
            } else if frame_behind(compute) {
                break;
            }
        }
    }
    grant
}

/// The Credit compute never sent, for a Scan of a list while a scenario asks for one.
fn overgrant(log: &TapLog, stream: u64, ranges: &[RangeSeen]) -> Option<Vec<u8>> {
    let frames = log.overgrant_lists.load(Ordering::Acquire);
    let list = ranges
        .first()?
        .first
        .first()
        .is_some_and(|keyspace| *keyspace == Keyspace::Forward.byte());
    if frames == 0 || !list {
        return None;
    }
    Credit::new(u32::try_from(frames).ok()?)
        .frame(Header::new(CLUSTER, Command::Credit).with_stream(stream))
        .ok()
}

/// Writes down the version `frame`, from the partition, names, unless a Scan holding a version sent it.
fn saw_reply(log: &TapLog, partition: u32, connection: usize, frame: &[u8]) {
    let Ok(view) = FrameView::validate(frame, CLUSTER, MAX_FRAME_SIZE) else {
        return;
    };
    let stream = view.header().stream();
    if view.command() == Command::Entries
        && !log.pin_streams_held().contains(&(connection, stream))
        && let Ok(entries) = EntriesView::from_frame(&view)
    {
        log.versions_held().push((partition, entries.version()));
    }
}

/// Goes on with the frames of one direction until either side closes.
fn relay(
    mut from: TcpStream,
    mut to: TcpStream,
    log: &TapLog,
    mut saw: impl FnMut(&TapLog, &[u8]) -> Option<Vec<u8>>,
) {
    // The frames the tap added on its own: each frame after them takes its place in the sender's order one further on.
    let mut added = 0u128;
    while let Some(mut frame) = read_frame(&mut from) {
        let also = saw(log, &frame);
        let Ok(sequence) = HeaderView::new(&frame).map(|header| header.sequence()) else {
            break;
        };
        if Header::stamp_sequence(&mut frame, sequence + added).is_err()
            || to.write_all(&frame).is_err()
        {
            break;
        }
        if let Some(mut also) = also {
            added += 1;
            if Header::stamp_sequence(&mut also, sequence + added).is_err()
                || to.write_all(&also).is_err()
            {
                break;
            }
        }
    }
    drop(from.shutdown(Shutdown::Both));
    drop(to.shutdown(Shutdown::Both));
}

/// Listens at `port` as `partition`, which really listens at `target`, on threads of the test's own for the life of the test
/// process.
#[expect(
    clippy::disallowed_methods,
    reason = "a tap relays beside the scenarios, on threads of the test's own"
)]
pub fn tap(port: u32, partition: u32, target: u32, log: &Arc<TapLog>) -> TestResult {
    let listener = TcpListener::bind(format!("127.0.0.1:{port}"))?;
    let log = Arc::clone(log);
    drop(std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let Ok(partition_side) = TcpStream::connect(format!("127.0.0.1:{target}")) else {
                continue;
            };
            let connection = log.connections.fetch_add(1, Ordering::AcqRel);
            let (Ok(compute_back), Ok(compute_seen), Ok(partition_back)) = (
                stream.try_clone(),
                stream.try_clone(),
                partition_side.try_clone(),
            ) else {
                continue;
            };
            drop(stream.set_nodelay(true));
            drop(partition_side.set_nodelay(true));
            let up = Arc::clone(&log);
            drop(std::thread::spawn(move || {
                relay(stream, partition_side, &up, |seen, frame| {
                    saw_request(seen, partition, (connection, &compute_seen), frame)
                });
                let _new = up
                    .ended
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .insert(connection);
            }));
            let down = Arc::clone(&log);
            drop(std::thread::spawn(move || {
                relay(partition_back, compute_back, &down, |seen, frame| {
                    saw_reply(seen, partition, connection, frame);
                    None
                });
            }));
        }
    }));
    Ok(())
}
