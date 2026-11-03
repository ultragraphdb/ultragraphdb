/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::future::Future;
use std::task::Poll;

use ultragraphdb_error_log::LogError;

use crate::types::durability_ticket::DurabilityTicket;
use crate::types::log_event::LogEvent;
use crate::types::position::Position;
use crate::types::round::Round;
use crate::types::round_ref::RoundRef;

/// The ordered log the commit step, the sequencer and the metadata group write rounds to: the write-ahead log on one
/// node, VSR from stage 8. Its shape is Delos' Loglet (append, check the tail, read, trim, seal) with TigerBeetle's
/// parent checksums chaining the rounds.
///
/// **The async boundary.** The methods that decide are synchronous and do no I/O: [`append`](Self::append) checks a
/// round, stages it and hands back its [`DurabilityTicket`]; [`poll_ticket`](Self::poll_ticket),
/// [`committed`](Self::committed), [`tail`](Self::tail), [`checkpoint`](Self::checkpoint), [`seal`](Self::seal) and
/// [`next_event`](Self::next_event) read the log's state. The methods that wait on storage or on replicas return
/// futures: [`sync`](Self::sync) makes the staged rounds durable, [`read`](Self::read) and
/// [`trim_through`](Self::trim_through) reach the disk. Durability is reported through the ticket and `sync`, never
/// awaited inside `append`, so a refusal is known before anything is written and the commit step resolves round k + 1
/// while round k's sync is pending: neither the resolver nor a ticket borrows the log. A command is applied once its
/// ticket completes. On compio the futures complete on the runtime's completions; over the simulated storage they
/// complete when first polled, so the simulator drives the log without a runtime.
pub trait OrderedLog {
    /// Stages `round` and returns its ticket, which completes once the round is durable. Accepted only on the
    /// primary, at the position after the tail, with the tail's checksum as its parent, and when the round fits;
    /// otherwise refused before anything is written.
    fn append(&mut self, round: RoundRef<'_>) -> Result<DurabilityTicket, LogError>;

    /// Where `ticket`'s round stands: `Ready(Ok(()))` once it is durable, which is when
    /// [`committed`](Self::committed) reaches its position; `Ready(Err(_))` once it never will be, because the log
    /// was poisoned before it was durable ([`LogError::poisoned`]), its round was replaced
    /// ([`LogError::truncated`]), or it names a position past the tail, which this log never handed out
    /// ([`LogError::position_mismatch`]); `Pending` until then. Polling changes nothing.
    fn poll_ticket(&self, ticket: DurabilityTicket) -> Poll<Result<(), LogError>>;

    /// Makes every staged round durable, in order (on one node one write and one sync per round; under VSR a
    /// quorum's journals), completing their tickets, and returns the committed position. A failed write or sync
    /// poisons the log: this and every later operation fails until it is opened again.
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

    /// Records `position` as the checkpoint and removes what holds only the rounds through it. The caller has synced
    /// the store first, since its journal is never synced on its own: a trim never removes a round the store has not
    /// durably applied. Never removes a round past the committed position.
    fn trim_through(&mut self, position: Position) -> impl Future<Output = Result<(), LogError>>;

    /// The last checkpoint's position, [`Position::ZERO`] before the first: recovery refuses a store whose applied
    /// position lies below it, since the rounds between are trimmed.
    fn checkpoint(&self) -> Position;

    /// Refuses every later append and returns the final tail.
    fn seal(&mut self) -> Result<Position, LogError>;

    /// The next view change or replaced suffix, for the sequencer; `None` when there is none.
    fn next_event(&mut self) -> Option<LogEvent>;
}
