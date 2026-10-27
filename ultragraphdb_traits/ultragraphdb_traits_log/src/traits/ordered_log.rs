/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::future::Future;

use ultragraphdb_error_log::LogError;

use crate::types::log_event::LogEvent;
use crate::types::position::Position;
use crate::types::round::Round;
use crate::types::round_ref::RoundRef;

/// The ordered log the commit step writes rounds to: the write-ahead log on one node, VSR from stage 10. Its shape is
/// Delos' Loglet (append, check the tail, read, trim, seal) with TigerBeetle's parent checksums chaining the rounds.
///
/// **The async boundary.** The methods that decide are synchronous and do no I/O: [`append`](Self::append) checks a
/// round and stages it, [`committed`](Self::committed), [`tail`](Self::tail), [`seal`](Self::seal) and
/// [`next_event`](Self::next_event) read the log's state. The methods that wait on storage or on replicas return
/// futures: [`sync`](Self::sync) makes the staged rounds durable, [`read`](Self::read) and
/// [`trim_through`](Self::trim_through) reach the disk. Durability is reported by `sync`, never awaited inside
/// `append`, so a refusal is known before anything is written and the commit step resolves round k + 1 while the
/// future of round k's sync is pending: the resolver does not borrow the log. On compio the futures complete on the
/// runtime's completions; over the simulated storage they complete when first polled, so the simulator drives the
/// log without a runtime.
pub trait OrderedLog {
    /// Stages `round` and returns its position. Accepted only on the primary, at the position after the tail, with
    /// the tail's checksum as its parent, and when the round fits; otherwise refused before anything is written. A
    /// staged round is durable once [`sync`](Self::sync) reports a committed position at or past it.
    fn append(&mut self, round: RoundRef<'_>) -> Result<Position, LogError>;

    /// Makes every staged round durable, in order (on one node one write and one sync per round; under VSR a
    /// quorum's journals), and returns the committed position. A failed write or sync poisons the log: this and
    /// every later operation fails until it is opened again.
    fn sync(&mut self) -> impl Future<Output = Result<Position, LogError>>;

    /// The highest durable position: synced on one node, journaled by a quorum under VSR. Never decreases.
    fn committed(&self) -> Position;

    /// The last appended round's position and checksum, which the next round names as its parent; the last
    /// checkpoint's when no round follows it.
    fn tail(&self) -> (Position, u64);

    /// The durable rounds from `from` on, in order, each one's checksums and parent verified, as many as fit in
    /// `max_bytes` of frames and at least one when any is durable. A reader applies only up to
    /// [`committed`](Self::committed); a staged round is not returned.
    fn read(
        &mut self,
        from: Position,
        max_bytes: u32,
    ) -> impl Future<Output = Result<Vec<Round>, LogError>>;

    /// Records that the positions through `position` are covered by a durable checkpoint, and removes what holds
    /// only them. Never removes a round past the committed position.
    fn trim_through(&mut self, position: Position) -> impl Future<Output = Result<(), LogError>>;

    /// Refuses every later append and returns the final tail.
    fn seal(&mut self) -> Result<Position, LogError>;

    /// The next view change or replaced suffix, for the sequencer; `None` when there is none.
    fn next_event(&mut self) -> Option<LogEvent>;
}
