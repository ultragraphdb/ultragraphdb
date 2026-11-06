/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What compute dials in place of a partition: every byte goes on to the real partition, and what compute asks and the first
//! version each Scan answered is written down, so a scenario reads the Scans a real read sent.

use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use ultragraphdb_protocol_wire::{
    BodyView, Command, EntriesView, FrameView, HEADER_SIZE, HeaderView, MAX_FRAME_SIZE, ScanAt,
    ScanView,
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
    connections: AtomicUsize,
}

impl TapLog {
    /// Forgets all it saw and stops pausing.
    pub fn clear(&self) {
        self.scans_held().clear();
        self.versions_held().clear();
        self.pause_after.store(0, Ordering::Release);
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

    /// Lets the Scans held back go.
    pub fn go_on(&self) {
        self.pause_after.store(0, Ordering::Release);
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

/// Writes down what `frame`, from compute, asks.
fn saw_request(log: &TapLog, partition: u32, connection: usize, frame: &[u8]) {
    let Ok(view) = FrameView::validate_client(frame, CLUSTER, MAX_FRAME_SIZE) else {
        return;
    };
    if view.command() != Command::Scan {
        return;
    }
    let Ok(scan) = ScanView::from_frame(&view) else {
        return;
    };
    let ranges = scan
        .ranges()
        .map(|range| RangeSeen {
            first: range.first().to_vec(),
            past: range.past().to_vec(),
            after: range.after().to_vec(),
        })
        .collect();
    let seen = ScanSeen {
        partition,
        connection,
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
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

/// Writes down the version `frame`, from the partition, names.
fn saw_reply(log: &TapLog, partition: u32, frame: &[u8]) {
    let Ok(view) = FrameView::validate(frame, CLUSTER, MAX_FRAME_SIZE) else {
        return;
    };
    if view.command() == Command::Entries
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
    mut saw: impl FnMut(&TapLog, &[u8]),
) {
    while let Some(frame) = read_frame(&mut from) {
        saw(log, &frame);
        if to.write_all(&frame).is_err() {
            break;
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
            let (Ok(compute_back), Ok(partition_back)) =
                (stream.try_clone(), partition_side.try_clone())
            else {
                continue;
            };
            drop(stream.set_nodelay(true));
            drop(partition_side.set_nodelay(true));
            let up = Arc::clone(&log);
            drop(std::thread::spawn(move || {
                relay(stream, partition_side, &up, |seen, frame| {
                    saw_request(seen, partition, connection, frame);
                });
            }));
            let down = Arc::clone(&log);
            drop(std::thread::spawn(move || {
                relay(partition_back, compute_back, &down, |seen, frame| {
                    saw_reply(seen, partition, frame);
                });
            }));
        }
    }));
    Ok(())
}
