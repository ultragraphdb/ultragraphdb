/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroUsize;

use ultragraphdb_cluster_partition::{BoundKey, BoundStep};
use ultragraphdb_common_role::{Answer, Push};
use ultragraphdb_core_keys::Keyspace;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Closed, ClosedPartition, ClosedView, Command, Entries, EntriesEnd,
    ErrorCode, ErrorReply, FLAG_LAST, FrameView, Header, MAX_FRAME_SIZE, RecallView, ScanView,
};
use ultragraphdb_store_quads::{Page, PageEnd};

use crate::types::core_message::Pushes;
use crate::types::entries_fill::EntriesFill;
use crate::types::open_scan::OpenScan;
use crate::types::partition_core::PartitionCore;
use crate::types::partition_core::partition_core_frames::{built, last};
use crate::types::scan_bound::ScanBound;
use crate::types::scan_key::ScanKey;
use crate::types::scan_read::ScanRead;
use crate::types::scan_state::ScanState;
use crate::types::turn_budget::TurnBudget;
use crate::utils::commit_body::commit_body;
use crate::utils::error_reply::{error_reply, scan_refusal};
use crate::utils::scan_check::{check_bound, scan_reads, scan_terms};
use crate::utils::scan_version::{scan_named, scan_version};
use ultragraphdb_transactions_log::DurableStorage;

/// Why a frame of a Scan was not sent.
enum FrameRefusal {
    /// The Scan alone ends.
    Scan(ErrorReply),
    /// The Scan's transaction ended with it, so its Bind stream says so too.
    Transaction(BoundKey, ErrorReply),
}

fn as_unheld(identity: [u8; 16]) -> ErrorReply {
    ErrorReply::transaction_not_held(format!(
        "a Scan of transaction {:032x}, as one the partition does not hold",
        u128::from_be_bytes(identity)
    ))
}

impl<'a, S: DurableStorage> PartitionCore<'a, S> {
    /// A Scan: everything in it checked at arrival, before any frame, and the core answers it from then on, each frame once the
    /// one before has left.
    pub(super) fn scan(
        &mut self,
        key: ScanKey,
        view: &FrameView<'_>,
        pushes: &Pushes,
        max_frame: u32,
    ) -> Result<Answer, ErrorReply> {
        let scan = ScanView::from_frame(view).map_err(|rule| ErrorReply::protocol(&rule))?;
        let reads = scan_reads(&scan, self.id.get(), |keyspace, after| {
            self.host.resume_at(keyspace, after)
        })?;
        let terms = scan_terms(&scan)?;
        let identity = scan.transaction();
        let bound = if identity == [0; 16] {
            None
        } else {
            let read = self
                .host
                .bound_read(&identity)
                .ok_or_else(|| as_unheld(identity))?;
            if self.parked.contains_key(&read.key()) {
                return Err(ErrorReply::new(
                    ErrorCode::Protocol,
                    "a Scan of a transaction whose write waits on its home: a read follows the writes before it",
                ));
            }
            check_bound(&scan, &read, &reads, &terms)?;
            Some(ScanBound {
                identity,
                key: read.key(),
            })
        };
        let now = self.now_us();
        let closed = self.host.closed().get();
        let waits = bound.is_none() && closed < scan.epoch();
        if waits && scan.deadline_ms() == 0 {
            return Err(self.not_closed(scan.epoch()));
        }
        let reads_after = reads.first().and_then(|read| read.after.clone());
        let open = OpenScan {
            pushes: pushes.clone(),
            max_frame: (max_frame as usize).min(MAX_FRAME_SIZE), // lossless: a `usize` is at least 32 bits
            at: scan.at(),
            read_version: scan.read_version(),
            epoch: scan.epoch(),
            limit: NonZeroUsize::new(scan.limit() as usize), // a `u32`, lossless
            bound,
            from: reads
                .first()
                .map_or_else(Vec::new, |read| read.first.clone()),
            terms,
            ranges: reads,
            state: ScanState::Epoch {
                deadline_us: now.saturating_add(u64::from(scan.deadline_ms()) * 1_000),
            },
            version: 0,
            named: 0,
            read_epoch: 0,
            pin: None,
            range: 0,
            after: reads_after,
            held: None,
            rows: 0,
        };
        self.opened(key, open, waits, now)
    }

    /// Takes `open` on, at once unless it `waits` for an epoch or its transaction for its home's answers.
    fn opened(
        &mut self,
        key: ScanKey,
        mut open: OpenScan<'a>,
        waits: bool,
        now: u64,
    ) -> Result<Answer, ErrorReply> {
        if let Some(bound) = open.bound {
            self.scans.join_transaction(bound.key, key);
            match self.host.read_terms_bound(bound.key, &open.terms, now) {
                Ok(BoundStep::Done) => {}
                Ok(BoundStep::Waiting) => {
                    open.state = ScanState::References;
                    self.scans.open(key, open);
                    return Ok(Answer::Open);
                }
                Err(refusal) => {
                    let reply = error_reply(&refusal);
                    self.tell_ended(bound.key, &reply);
                    return Err(reply);
                }
            }
        }
        if waits {
            let (epoch, state) = (open.epoch, open.state);
            self.scans.open(key, open);
            self.scans.wait_for_epoch(key, epoch);
            if let ScanState::Epoch { deadline_us } = state {
                self.scans.set_timer(key, deadline_us);
            }
            return Ok(Answer::Open);
        }
        self.choose_version(&mut open)?;
        open.state = ScanState::Queued;
        self.scans.open(key, open);
        self.scans.give_turn(key);
        Ok(Answer::Open)
    }

    /// Chooses the one version `open` reads, and keeps it readable.
    fn choose_version(&self, open: &mut OpenScan<'a>) -> Result<(), ErrorReply> {
        let closed = self.host.closed();
        let (version, epoch) =
            scan_version(open.at, open.read_version, closed, self.host.watermark())?;
        open.version = version;
        open.named = scan_named(open.at, open.read_version, closed, version);
        open.read_epoch = epoch;
        if open.bound.is_none() {
            let pin = self
                .host
                .pin(version)
                .map_err(|refused| error_reply(&refused))?;
            open.pin = Some(pin);
        }
        Ok(())
    }

    /// Whether a Scan has a frame to build now.
    pub(super) fn scan_ready(&self) -> bool {
        self.scans.has_turns()
    }

    /// One turn of the Scans: those of a transaction that ended refused, those whose wait ended or that stalled dealt with, each found
    /// by what it waited on, then at most one frame per Scan in turn, within the turn's budget (`PROTOCOL.md` §13.14). A Scan its budget
    /// leaves keeps its place at the front for the next turn.
    pub(super) fn serve_scans(&mut self) {
        for bound in self.host.take_bound_ended() {
            for key in self.scans.end_transaction(bound) {
                let identity = self.scans.get(key).and_then(|scan| scan.bound);
                if let Some(identity) = identity {
                    self.refuse_scan(key, &as_unheld(identity.identity));
                }
            }
        }
        if self.scans.is_empty() {
            return;
        }
        let now = self.now_us();
        let closed = self.host.closed().get();
        while let Some(key) = self.scans.next_closed(closed) {
            self.start_scan(key);
        }
        while let Some(key) = self.scans.next_due(now) {
            self.past_its_time(key);
        }
        let mut budget = TurnBudget::default();
        for _ in 0..self.scans.turns_waiting() {
            if budget.is_spent() {
                return;
            }
            self.next_frame(now, &mut budget);
        }
    }

    /// The Scan `key`, whose time has passed, is refused: as waiting on its epoch, or as stalled.
    fn past_its_time(&mut self, key: ScanKey) {
        let Some(scan) = self.scans.get(key) else {
            return;
        };
        let reply = match scan.state {
            ScanState::Epoch { .. } => self.not_closed(scan.epoch),
            ScanState::Parked { .. } => self.stalled(),
            // Set only in the states above, and let go of on leaving them.
            ScanState::References | ScanState::Queued => return,
        };
        self.refuse_scan(key, &reply);
    }

    fn not_closed(&self, epoch: u64) -> ErrorReply {
        ErrorReply::new(
            ErrorCode::TimedOut,
            format!("partition {} has not closed epoch {epoch}", self.id.get()),
        )
        .with_detail(u64::from(self.id.get()))
    }

    fn stalled(&self) -> ErrorReply {
        ErrorReply::new(
            ErrorCode::TimedOut,
            format!(
                "the Scan of partition {} stalled: no frame could leave for {} ms",
                self.id.get(),
                self.scan_stall_ms
            ),
        )
        .with_detail(u64::from(self.id.get()))
    }

    /// Ends the Scan `key` with `reply` on its stream, and lets go of what it held.
    fn refuse_scan(&mut self, key: ScanKey, reply: &ErrorReply) {
        if let Some(scan) = self.scans.remove(key) {
            self.push_error(key.1, reply, &scan.pushes);
        }
    }

    /// The Scan `key`, its wait over: its version chosen and its first frame queued.
    fn start_scan(&mut self, key: ScanKey) {
        let Some(mut scan) = self.scans.remove(key) else {
            return;
        };
        match self.choose_version(&mut scan) {
            Ok(()) => {
                scan.state = ScanState::Queued;
                self.scans.open(key, scan);
                self.scans.give_turn(key);
            }
            Err(reply) => self.push_error(key.1, &reply, &scan.pushes),
        }
    }

    /// Wakes each Scan of the transaction `bound` waiting on its references, now answered: it starts, or asks for terms of its
    /// own, for which those after it wait in turn.
    pub(super) fn wake_references(&mut self, bound: BoundKey) {
        let now = self.now_us();
        for key in self.waiting_on(bound) {
            let Some(scan) = self.scans.get(key) else {
                continue;
            };
            match self.host.read_terms_bound(bound, &scan.terms, now) {
                Ok(BoundStep::Done) => self.start_scan(key),
                Ok(BoundStep::Waiting) => {}
                Err(refusal) => {
                    let reply = error_reply(&refusal);
                    self.tell_ended(bound, &reply);
                    self.refuse_references(bound, &reply);
                    return;
                }
            }
        }
    }

    /// Ends each Scan of the transaction `bound` waiting on its references with `reply`, the home's refusal.
    pub(super) fn refuse_references(&mut self, bound: BoundKey, reply: &ErrorReply) {
        for key in self.waiting_on(bound) {
            self.refuse_scan(key, reply);
        }
    }

    fn waiting_on(&self, bound: BoundKey) -> Vec<ScanKey> {
        self.scans
            .of_transaction(bound)
            .into_iter()
            .filter(|key| {
                self.scans
                    .get(*key)
                    .is_some_and(|scan| scan.state == ScanState::References)
            })
            .collect()
    }

    /// The Scan `connection` and `stream` name has room for its next frame: its last one left.
    pub(super) fn scan_room(&mut self, key: ScanKey) {
        if let Some(scan) = self.scans.get_mut(key)
            && matches!(scan.state, ScanState::Parked { .. })
        {
            scan.state = ScanState::Queued;
            self.scans.clear_timer(key);
            self.scans.give_turn(key);
        }
    }

    /// The last frame of the Scan `key` left, with no credit for its next: its stall counts from now.
    pub(super) fn scan_left(&mut self, key: ScanKey) {
        let now = self.now_us();
        if let Some(scan) = self.scans.get_mut(key)
            && let ScanState::Parked { since_us } = &mut scan.state
        {
            *since_us = now;
            self.scans
                .set_timer(key, now.saturating_add(self.stall_us()));
        }
    }

    /// How long a Scan with no frame able to leave is kept.
    fn stall_us(&self) -> u64 {
        self.scan_stall_ms.saturating_mul(1_000)
    }

    /// Lets go of the Scan `key`, which its reader cancelled.
    pub(super) fn cancel_scan(&mut self, key: ScanKey) {
        let _gone = self.scans.remove(key);
    }

    /// Lets go of every Scan of `connection`, which closed.
    pub(super) fn close_scans(&mut self, connection: u64) {
        self.scans.close(connection);
    }

    /// Builds the next queued Scan's next frame within what `budget` leaves, and hands it to its connection.
    fn next_frame(&mut self, now: u64, budget: &mut TurnBudget) {
        let Some((key, mut scan)) = self.scans.take_turn() else {
            return;
        };
        match self.fill_frame(&mut scan, now, budget.fill_to()) {
            // Nothing to send yet: the Scan goes on at its next turn.
            Ok(None) => {
                budget.spend_idle_page();
                self.scans.open(key, scan);
                self.scans.give_turn(key);
            }
            Ok(Some(entries)) => {
                let done = scan.is_done();
                let header = Header::new(self.cluster, Command::Entries).with_stream(key.1);
                let header = if done {
                    header.with_flags(FLAG_LAST)
                } else {
                    header
                };
                match entries.frame(header) {
                    Ok(frame) => {
                        budget.spend_frame(frame.len());
                        // A connection gone says so by its Closed.
                        drop(scan.pushes.unbounded_send((key.1, Push::Frame(frame))));
                    }
                    Err(rule) => {
                        self.push_error(key.1, &ErrorReply::protocol(&rule), &scan.pushes);
                        return;
                    }
                }
                if !done {
                    scan.state = ScanState::Parked { since_us: now };
                    self.scans.open(key, scan);
                    self.scans
                        .set_timer(key, now.saturating_add(self.stall_us()));
                }
            }
            Err(FrameRefusal::Scan(reply)) => self.push_error(key.1, &reply, &scan.pushes),
            Err(FrameRefusal::Transaction(bound, reply)) => {
                self.tell_ended(bound, &reply);
                self.push_error(key.1, &reply, &scan.pushes);
            }
        }
    }

    /// The next frame of `scan`'s range under way, when its turn found anything to send: what it covers is in the transaction's
    /// reads before it returns.
    fn fill_frame(
        &mut self,
        scan: &mut OpenScan<'a>,
        now: u64,
        fill_to: usize,
    ) -> Result<Option<Entries>, FrameRefusal> {
        let Some(read) = scan.ranges.get(scan.range).cloned() else {
            return Err(FrameRefusal::Scan(ErrorReply::new(
                ErrorCode::Protocol,
                "a Scan holds no range",
            )));
        };
        let range = u32::try_from(scan.range).unwrap_or(u32::MAX);
        let mut fill = EntriesFill::new(range, (scan.read_epoch, scan.named), scan.max_frame)
            .filling_to(fill_to);
        if read.keyspace == Keyspace::Dictionary {
            return self.fill_run(scan, fill, now).map(Some);
        }
        let request = scan.kernel_scan(&read);
        let values = matches!(read.keyspace, Keyspace::Referrers | Keyspace::Statements);
        let page = match scan.bound {
            Some(bound) if values => {
                self.host
                    .values_page_bound(bound.key, &request, now, |key, value| {
                        fill.offer_value(key, value)
                    })
            }
            Some(bound) => self
                .host
                .page_bound(bound.key, &request, now, |key, object, ids| {
                    fill.offer_object(key, object, ids)
                }),
            None if values => self
                .host
                .values_page(&request, |key, value| fill.offer_value(key, value)),
            None => self.host.page(&request, |key, object, ids| {
                fill.offer_object(key, object, ids)
            }),
        }
        .map_err(|refusal| frame_refusal(scan, &scan.from, &refusal))?;
        page_ended(scan, fill, page)
    }

    /// The frame of the run of one-key dictionary ranges `scan` stands at: every one that fits, so resolving a batch of terms costs
    /// one frame and not one per term (`PROTOCOL.md` §13.14).
    fn fill_run(
        &mut self,
        scan: &mut OpenScan<'a>,
        mut fill: EntriesFill,
        now: u64,
    ) -> Result<Entries, FrameRefusal> {
        let first = scan.range;
        let mut next = first;
        while let Some(read) = scan
            .ranges
            .get(next)
            .filter(|read| read.keyspace == Keyspace::Dictionary)
        {
            let value = self
                .dictionary_value(scan, next, read, now)
                .map_err(|refusal| frame_refusal(scan, &read.first, &refusal))?;
            if let Some(value) = value
                && fill.offer_value(&read.first, &value).is_break()
            {
                if next == first {
                    return Err(FrameRefusal::Scan(too_large(scan.max_frame)));
                }
                break;
            }
            next += 1;
        }
        scan.move_to(next);
        Ok(fill.finish_run(u32::try_from(next - 1).unwrap_or(u32::MAX)))
    }

    /// What the dictionary answers range `read`, at `index`, of `scan`: a term's ID as a read of its transaction resolves it, the
    /// entry as the transaction sees it, or as the store holds it.
    fn dictionary_value(
        &mut self,
        scan: &OpenScan<'a>,
        index: usize,
        read: &ScanRead,
        now: u64,
    ) -> Result<Option<Vec<u8>>, ultragraphdb_cluster_partition::PartitionError> {
        let Some(bound) = scan.bound else {
            return self.host.dictionary(&read.first);
        };
        match scan.terms.get(index) {
            Some(iri) => Ok(self
                .host
                .resolve_bound(bound.key, iri.as_ref(), now)?
                .map(|id| id.to_be_bytes().to_vec())),
            None => self.host.dictionary_bound(bound.key, &read.first, now),
        }
    }

    /// A Closed: the partition's latest closed epoch.
    pub(super) fn closed_epochs(
        &self,
        stream: u64,
        view: &FrameView<'_>,
    ) -> Result<Answer, ErrorReply> {
        let asked = ClosedView::from_frame(view).map_err(|rule| ErrorReply::protocol(&rule))?;
        asked
            .check_request()
            .map_err(|rule| ErrorReply::protocol(&rule))?;
        let epoch = self.host.closed().get();
        let own = self.id.get();
        let partitions = asked
            .partitions()
            .map(|partition| {
                if partition.partition() == own {
                    Ok(ClosedPartition::new(own, epoch))
                } else {
                    Err(self.not_served(partition.partition()))
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        built(
            &Closed::new(partitions),
            last(self.cluster, stream, Command::Closed),
        )
    }

    /// A Recall: the reply the session table holds, as the Commit answered it, or why it holds none.
    pub(super) fn recall(&self, stream: u64, view: &FrameView<'_>) -> Result<Answer, ErrorReply> {
        let recall = RecallView::from_frame(view).map_err(|rule| ErrorReply::protocol(&rule))?;
        if recall.partition() != self.id.get() {
            return Err(self.not_served(recall.partition()));
        }
        let session = u128::from_be_bytes(recall.session());
        let stored = self
            .host
            .recall(session, recall.request())
            .map_err(|refusal| error_reply(&refusal.into()))?;
        let body = commit_body(
            (session, recall.request()),
            stored.version(),
            Some(stored.token()),
            stored.results(),
            stored.terms(),
        );
        built(&body, last(self.cluster, stream, Command::Commit))
    }

    fn not_served(&self, partition: u32) -> ErrorReply {
        ErrorReply::new(
            ErrorCode::Protocol,
            format!(
                "this address hosts partition {}, not {partition}",
                self.id.get()
            ),
        )
    }
}

/// Why a read of `key` under `scan` sends no frame: a Scan of a transaction ended it, which its Bind stream says too.
fn frame_refusal(
    scan: &OpenScan<'_>,
    key: &[u8],
    refusal: &ultragraphdb_cluster_partition::PartitionError,
) -> FrameRefusal {
    let reply = scan_refusal(key, refusal);
    match scan.bound {
        Some(bound) => FrameRefusal::Transaction(bound.key, reply),
        None => FrameRefusal::Scan(reply),
    }
}

/// The frame `fill` holds, as `page` ended, when it holds one to send, and `scan` moved on to what follows it.
fn page_ended(
    scan: &mut OpenScan<'_>,
    fill: EntriesFill,
    page: Page,
) -> Result<Option<Entries>, FrameRefusal> {
    scan.rows += page.rows();
    let (end, held) = page.into_parts();
    scan.held = held;
    match end {
        PageEnd::Exhausted => {
            scan.next_range();
            Ok(Some(fill.finish(EntriesEnd::Ended)))
        }
        PageEnd::Stopped { .. } => {
            scan.next_range();
            Ok(Some(fill.finish(EntriesEnd::Limit)))
        }
        PageEnd::Paused { last: None, .. } => Err(FrameRefusal::Scan(too_large(scan.max_frame))),
        PageEnd::Paused { from, after, .. } => {
            scan.from = from;
            let inside = after.is_some();
            scan.after = after;
            Ok(Some(if inside {
                fill.finish_continuing()
            } else {
                fill.finish(EntriesEnd::More)
            }))
        }
        PageEnd::Yielded { from } => {
            scan.from = from;
            scan.after = None;
            Ok((!fill.is_empty()).then(|| fill.finish(EntriesEnd::More)))
        }
    }
}

/// An object record or value too large for an otherwise empty frame of `max_frame` bytes.
fn too_large(max_frame: usize) -> ErrorReply {
    ErrorReply::new(
        ErrorCode::TooLarge,
        format!("an entry is too large for a frame of {max_frame} bytes"),
    )
    .with_detail(max_frame as u64) // lossless: a `usize` is at most 64 bits
}
