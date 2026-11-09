/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

pub(crate) mod partition_core_frames;
mod partition_core_one_frame;
mod partition_core_reads;
mod partition_core_remote;
mod partition_core_versions;

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::future::poll_fn;
use std::rc::Rc;
use std::sync::mpsc::{Receiver, Sender};
use std::task::Poll;
use std::time::Instant;

use futures::StreamExt;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};
use ultragraphdb_cluster_partition::{
    BoundKey, MAILBOX_PER_TURN_MAX, Partition, PartitionHost, PartitionMessage, RemoteMessage,
};
use ultragraphdb_cluster_placement::PartitionId;
use ultragraphdb_common_role::{Answer, ParkedBytes, RoleClient, RoleCluster};
use ultragraphdb_common_service::InboxTurn;
use ultragraphdb_core_catalog::CatalogContents;
use ultragraphdb_core_ids::Epoch;
use ultragraphdb_error_role::RoleError;
use ultragraphdb_protocol_transport::Address;
use ultragraphdb_protocol_wire::{
    BodyBuilder, Command, DropCommand, ErrorCode, ErrorReply, FLAG_LAST, HEADER_SIZE, Header,
    Reference, Report, TermTable,
};
use ultragraphdb_store_apply::ChangesetError;
use ultragraphdb_store_kv::{FjallStore, MokaListCache, ThreadExecutor};
use ultragraphdb_store_posting::CachedList;
use ultragraphdb_traits_catalog::DefaultEntry;
use ultragraphdb_traits_store::Store;
use ultragraphdb_transactions_log::{CompioStorage, Wal};

use crate::types::commit_mark::CommitMark;
use crate::types::commit_phase::CommitPhase;
use crate::types::commit_phases::CommitPhases;
use crate::types::core_message::{CoreMessage, Pushes};
use crate::types::log_io::LogIo;
use crate::types::open_scan::OpenScan;
use crate::types::parked_frame::ParkedFrame;
use crate::types::refill_blocks::RefillBlocks;
use crate::types::retention::Retention;
use crate::types::scan_board::ScanBoard;
use crate::types::timed_storage::TimedStorage;
use crate::types::turn_budget::TURN_BYTES;
use crate::utils::commit_body::commit_body;
use crate::utils::error_reply::error_reply;
use crate::utils::micros_since::micros_since;

/// A partition of the storage role, over fjall.
pub type StoragePartition =
    Partition<FjallStore, MokaListCache<CachedList>, ThreadExecutor, RefillBlocks>;

/// Its host, over the write-ahead log, its I/O timed.
pub type StorageHost<'a> = PartitionHost<
    'a,
    Wal<TimedStorage<CompioStorage>>,
    FjallStore,
    MokaListCache<CachedList>,
    ThreadExecutor,
    RefillBlocks,
>;

/// How long the core waits for a message before it looks at its timers and at what other partitions of the process sent.
const TICK: std::time::Duration = std::time::Duration::from_millis(1);

/// The waiting messages one turn takes of its inbox after the one it waited for, the rest waiting for the next turn: 1,024, the
/// most requests of the smallest size, a header alone, that [`TURN_BYTES`] holds. A turn of the smallest requests so takes in
/// what it may build of Scans' frames, and the commits and Scans waiting on the turn wait for that much of its connections'
/// work; a larger request counts as one, its bytes bounded by its connection's `max_frame` and held bytes. One sync a turn, at
/// the 2,188 a second the Shard-per-Core note measures, so admits two million requests a second, past what a core serves.
const MESSAGES_PER_TURN_MAX: usize = TURN_BYTES / HEADER_SIZE;

/// The epoch closes one turn takes of those delivered to it, the rest waiting for the next turn: 64. A close arrives at most
/// once an epoch, and an epoch lasts at least `epoch_ms_min`, never under 1 ms (`PROTOCOL.md` §11.9), while a turn waits at
/// most [`TICK`] and syncs once, about 0.46 ms at the 2,188 syncs a second the Shard-per-Core note measures: a turn finds one
/// or two. The bound is for a core that stalled, on a slow sync or a starved thread: a turn takes the closes of 64 ms of the
/// shortest epochs, each a send to its own mailbox and the commit step's close of an epoch, and a longer backlog shrinks by 64
/// a turn less the closes arriving during it.
pub const CLOSES_PER_TURN_MAX: usize = 64;

// A turn of the core hands its host's mailbox at most one message per request it took, the one it waited for among them,
// and one per close; a host's turn has room for all of them beside what the other partitions send, which share its one
// mailbox in the order they came.
const _: () = assert!(MESSAGES_PER_TURN_MAX + 1 + CLOSES_PER_TURN_MAX < MAILBOX_PER_TURN_MAX);

/// What one turn takes of what other partitions of the process sent beyond it, the rest waiting for the next turn: as many as
/// of its inbox, [`MESSAGES_PER_TURN_MAX`]. Each is a write's registrations for a home in another process, sent once a write
/// parks here and again a batch a second until its limit, or a home's answer to a registration this partition asked: each
/// stands for a request a connection sent, as an inbox message does, and costs a spawned task or a parked write's resumption.
pub const REMOTE_PER_TURN_MAX: usize = MESSAGES_PER_TURN_MAX;

/// How long a partition keeps a Scan with no frame able to leave, by default (`PROTOCOL.md` §11.9), in milliseconds.
pub const SCAN_STALL_MS_DEFAULT: u64 = 30_000;

/// What the core asks of the frontend, on a connection of its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrontendAsk {
    /// The close of an epoch.
    Close(Epoch),
    /// The record of a predicate's default entry, after a first write of it or a replay of one (`PROTOCOL.md` §11.12).
    DefaultEntry(DefaultEntry),
    /// A catalog version's outcome here, durable: sent until answered (`PROTOCOL.md` §13.19).
    Report(Report),
}

/// What a parked request does once its transaction's references are answered.
#[derive(Debug, Clone, Copy)]
enum ParkedWork {
    /// A Write's references are answered: its changes run from the frame it held.
    Write,
    /// An Operation's references are answered, and the host applied it.
    Operation,
    /// A request that waited behind a read's references or its stream's parked request takes its turn from the start.
    Turn,
    /// A one-frame write's references are answered, and it goes on from `at`.
    OneFrame { at: OneFrameAt },
}

/// What a request came to.
#[derive(Debug)]
enum RequestStep {
    /// Its answer.
    Answered(Answer),
    /// It waits on a home, going on as `work`, in `tenant`, from the frame it holds.
    Parks { tenant: u64, work: ParkedWork },
    /// A one-frame write in the commit step, under this ticket.
    InCommit(u64),
}

/// Where a one-frame write goes on from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OneFrameAt {
    /// Its changes, from the start.
    Changes,
    /// Its changes, their references answered.
    ChangesHeard,
    /// The operation of this index, those before it held.
    Operation(usize),
}

/// What runs a one-frame write again on a conflict over what it tested.
#[derive(Debug)]
struct OneFrameRetry {
    connection: u64,
    frame: Vec<u8>,
    attempts: u32,
}

/// A request waiting for its transaction's references' answers, a copy of its frame held, which a write's changes borrow, and the
/// frames behind it on its stream in their order. Its answer goes out through `pushes`.
#[derive(Debug)]
struct Parked {
    frame: ParkedFrame,
    tenant: u64,
    work: ParkedWork,
    pushes: Pushes,
    max_frame: u32,
    behind: VecDeque<ParkedFrame>,
    /// Where the core stood when it took the request.
    taken: CommitMark,
}

/// A Reference a home is answering, one registration at a time.
#[derive(Debug)]
struct HomeAsking {
    stream: u64,
    reply: Reference,
    unanswered: usize,
    pushes: Pushes,
}

/// A Commit waiting for its outcome.
#[derive(Debug)]
struct Committing {
    stream: u64,
    session: u128,
    request: u64,
    pushes: Pushes,
    /// A one-frame write's next run.
    retry: Option<OneFrameRetry>,
    /// A Drop's request, which its reply is built from.
    dropped: Option<DropCommand>,
    /// Where the core stood when it took the request, and when the request entered the commit step.
    taken: CommitMark,
    stepped: CommitMark,
}

/// What the core's connections to others share: the address of each partition, and the connections it has open.
#[derive(Debug, Clone)]
pub struct Peers {
    cluster: RoleCluster,
    frontend: Address,
    name: String,
    own: UnboundedSender<PartitionMessage>,
    // Shared by this core's tasks, all on its one thread, so `Rc` and `RefCell` hold them and no borrow crosses a wait.
    addresses: Rc<RefCell<BTreeMap<u32, String>>>,
    open: Rc<RefCell<HashMap<String, RoleClient>>>,
}

impl Peers {
    /// What a core of `cluster` shares, dialling others as `name`, the frontend at `frontend`, a peer's messages reaching it
    /// through `own`. It knows no partition's address and holds no connection yet.
    #[must_use]
    pub fn new(
        cluster: RoleCluster,
        frontend: Address,
        name: String,
        own: UnboundedSender<PartitionMessage>,
    ) -> Self {
        Self {
            cluster,
            frontend,
            name,
            own,
            addresses: Rc::new(RefCell::new(BTreeMap::new())),
            open: Rc::new(RefCell::new(HashMap::new())),
        }
    }

    /// The cluster this role belongs to.
    #[must_use]
    pub fn cluster(&self) -> &RoleCluster {
        &self.cluster
    }

    /// The frontend's role address.
    #[must_use]
    pub fn frontend(&self) -> &Address {
        &self.frontend
    }

    /// The name this role dials others under.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// One partition's core: the one task that owns its host. Connections hand it requests by message and it answers them,
/// so nothing it holds is shared.
pub struct PartitionCore<'a> {
    host: StorageHost<'a>,
    id: PartitionId,
    cluster: u64,
    clock: Instant,
    inbox: UnboundedReceiver<CoreMessage>,
    remote: Receiver<RemoteMessage>,
    own: Sender<PartitionMessage>,
    closes: Receiver<PartitionMessage>,
    asks: UnboundedSender<FrontendAsk>,
    peers: Peers,
    bound_tenant: BTreeMap<BoundKey, (u64, Pushes)>,
    /// The streams a Bind of kind 2 opened, each waiting for its Drop, with the tenant and the principal it named.
    drop_streams: BTreeMap<BoundKey, (u64, u64)>,
    /// The tenants the placement map lists a run for, the only ones a Bind may name.
    tenants: BTreeSet<u64>,
    /// Where each open connection counts what it holds parked, read only as a frame parks.
    connections: BTreeMap<u64, ParkedBytes>,
    parked: BTreeMap<BoundKey, Parked>,
    home_askings: BTreeMap<u64, HomeAsking>,
    next_asking: u64,
    committing: BTreeMap<u64, Committing>,
    /// The catalog version in the commit step, by ticket, with what the data refused of it, run again when a commit aborted it.
    installing: BTreeMap<u64, (CatalogContents, Vec<ChangesetError>)>,
    /// The catalog versions waiting for it, or for a build's end, in the order the closes announced them: one for each close
    /// announcing a catalog change, which administration makes, so its length is the changes not yet installed.
    versions: VecDeque<CatalogContents>,
    /// The Scans it answers, and those with a frame to build, in turn.
    scans: ScanBoard<OpenScan<'a>>,
    scan_stall_ms: u64,
    retention: Retention,
    /// Where a higher discard version goes: the store's synced write of it runs off the core, so no read waits for it.
    discards: Sender<u64>,
    /// What its log spends in its I/O, which its log's storage counts.
    log_io: LogIo,
    /// How long the core has waited on its inbox since it began, in microseconds.
    waited_us: u64,
    /// Where the core stood when it took the request it runs.
    taking: CommitMark,
    /// The phases of its commits since its last report.
    phases: CommitPhases,
}

impl std::fmt::Debug for PartitionCore<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PartitionCore")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

/// The channels a core is reached by and reaches others through. Its connections bound `inbox`, as
/// [`crate::StorageSession`] says: one request each at a time and a note per stream.
#[derive(Debug)]
pub struct CoreChannels {
    inbox: UnboundedReceiver<CoreMessage>,
    remote: Receiver<RemoteMessage>,
    own: Sender<PartitionMessage>,
    closes: Receiver<PartitionMessage>,
    asks: UnboundedSender<FrontendAsk>,
    discards: Sender<u64>,
}

impl CoreChannels {
    /// What its connections send it, `inbox`; what other partitions send it, `remote`; where messages reach it, `own`; the epoch
    /// closes delivered to it, `closes`; where it asks the frontend, `asks`; and where each higher discard version goes,
    /// `discards`, read off the core.
    #[must_use]
    pub fn new(
        inbox: UnboundedReceiver<CoreMessage>,
        remote: Receiver<RemoteMessage>,
        own: Sender<PartitionMessage>,
        closes: Receiver<PartitionMessage>,
        asks: UnboundedSender<FrontendAsk>,
        discards: Sender<u64>,
    ) -> Self {
        Self {
            inbox,
            remote,
            own,
            closes,
            asks,
            discards,
        }
    }
}

/// What a core is made of.
pub struct CoreParts<'a> {
    host: StorageHost<'a>,
    id: PartitionId,
    cluster: u64,
    clock: Instant,
    channels: CoreChannels,
    peers: Peers,
    scan_stall_ms: u64,
    retention: Retention,
    tenants: BTreeSet<u64>,
    log_io: LogIo,
}

impl<'a> CoreParts<'a> {
    /// The core of partition `id`, hosted over `host`, every frame naming `cluster`, its time counted from `clock`, retaining what
    /// `retention` says.
    #[must_use]
    pub fn new(
        host: StorageHost<'a>,
        id: PartitionId,
        cluster: u64,
        clock: Instant,
        channels: CoreChannels,
        peers: Peers,
        retention: Retention,
    ) -> Self {
        Self {
            host,
            id,
            cluster,
            clock,
            channels,
            peers,
            scan_stall_ms: SCAN_STALL_MS_DEFAULT,
            retention,
            tenants: BTreeSet::new(),
            log_io: LogIo::default(),
        }
    }

    /// The core reading what its log spends in its I/O from `io`, which its log's [`TimedStorage`] counts into; without it the
    /// core reports its commits' phases with no log I/O.
    #[must_use]
    pub fn with_log_io(mut self, io: LogIo) -> Self {
        self.log_io = io;
        self
    }

    /// The core taking Binds of `tenants`, those the placement map's runs list.
    #[must_use]
    pub fn with_tenants(mut self, tenants: impl IntoIterator<Item = u64>) -> Self {
        self.tenants = tenants.into_iter().collect();
        self
    }

    /// The core keeping a Scan with no frame able to leave for `ms` milliseconds.
    #[must_use]
    pub fn with_scan_stall_ms(mut self, ms: u64) -> Self {
        self.scan_stall_ms = ms;
        self
    }
}

impl<'a> PartitionCore<'a> {
    /// The core of `parts`.
    #[must_use]
    pub fn new(parts: CoreParts<'a>) -> Self {
        Self {
            host: parts.host,
            id: parts.id,
            cluster: parts.cluster,
            clock: parts.clock,
            inbox: parts.channels.inbox,
            remote: parts.channels.remote,
            own: parts.channels.own,
            closes: parts.channels.closes,
            asks: parts.channels.asks,
            peers: parts.peers,
            bound_tenant: BTreeMap::new(),
            drop_streams: BTreeMap::new(),
            tenants: parts.tenants,
            connections: BTreeMap::new(),
            parked: BTreeMap::new(),
            home_askings: BTreeMap::new(),
            next_asking: 1,
            committing: BTreeMap::new(),
            installing: BTreeMap::new(),
            versions: VecDeque::new(),
            scans: ScanBoard::default(),
            scan_stall_ms: parts.scan_stall_ms,
            retention: parts.retention,
            discards: parts.channels.discards,
            log_io: parts.log_io,
            waited_us: 0,
            taking: CommitMark::default(),
            phases: CommitPhases::new(),
        }
    }

    /// The microseconds since the core began.
    fn now_us(&self) -> u64 {
        micros_since(self.clock)
    }

    /// Where the core stands now.
    fn mark(&self) -> CommitMark {
        CommitMark::new(self.now_us(), self.log_io.totals(), self.waited_us)
    }

    /// Serves until `stopping` says otherwise, then ends what is open and syncs what its log holds through its store.
    ///
    /// A dropped future drops the core without its stop: a request waiting for its first answer is refused by its session
    /// as the partition stopped, a stream already open hears nothing more until its connection closes, a round under sync
    /// is neither applied nor handed out, as [`PartitionHost::turn`] says, and the store is not synced, so the next start
    /// recovers from the log.
    ///
    /// # Errors
    ///
    /// [`RoleError::io`] when the pipeline stops on its log or store.
    pub async fn run(
        mut self,
        stopping: &ultragraphdb_common_service::Stopping,
    ) -> Result<(), RoleError> {
        while !stopping.is_stopping() {
            let waiting = self.now_us();
            let first = if self.scan_ready() {
                // A Scan's next frame is due, so no time is waited away: only its connection gets to run.
                yield_once().await;
                None
            } else {
                compio::time::timeout(TICK, self.inbox.next())
                    .await
                    .ok()
                    .flatten()
            };
            self.waited_us = self
                .waited_us
                .saturating_add(self.now_us().saturating_sub(waiting));
            if let Some(first) = first {
                self.take(first);
            }
            // At most MESSAGES_PER_TURN_MAX, the rest waiting for the next turn (Ultra rule 2).
            let mut taking = InboxTurn::new(MESSAGES_PER_TURN_MAX);
            while let Some(next) = taking.receive(|| self.inbox.try_recv()) {
                self.take(next);
            }
            self.turn().await?;
        }
        self.tell_parked_stopping();
        let keys: Vec<BoundKey> = self.bound_tenant.keys().copied().collect();
        for key in keys {
            self.host.abort_bound(key);
        }
        self.turn().await?;
        // What was told above reaches a connection that gets to run; the rest learn it from the connection's close.
        yield_once().await;
        self.host
            .partition()
            .applier()
            .quads()
            .store()
            .sync()
            .map_err(|error| RoleError::io(format!("the store: {error}")))
    }

    /// One turn of the host, and what that made durable handed on. A future dropped at the host's turn leaves the core fit
    /// only to be dropped, as [`PartitionCore::run`] says.
    async fn turn(&mut self) -> Result<(), RoleError> {
        // At most CLOSES_PER_TURN_MAX, the rest waiting for the next turn (Ultra rule 2).
        let mut taking = InboxTurn::new(CLOSES_PER_TURN_MAX);
        while let Some(message) = taking.receive(|| self.closes.try_recv()) {
            // A close the subscription brought, to the partition's own mailbox.
            drop(self.own.send(message));
        }
        self.take_remote();
        let now = self.now_us();
        for key in self.host.sweep_bound(now) {
            self.past_limit(key);
        }
        self.host
            .turn(now)
            .await
            .map_err(|error| RoleError::io(format!("the partition {}: {error}", self.id.get())))?;
        if let Some(epoch) = self.host.take_close_request() {
            // A frontend link gone is reported by the subscription.
            drop(self.asks.unbounded_send(FrontendAsk::Close(epoch)));
        }
        for entry in self.host.partition().applier().take_default_entries() {
            // A frontend link gone is reported by the subscription, and the recovery after it asks again.
            drop(self.asks.unbounded_send(FrontendAsk::DefaultEntry(entry)));
        }
        self.resume_ready();
        self.hand_out();
        self.next_version();
        self.serve_scans();
        self.raise_discard(now);
        self.report_phases();
        Ok(())
    }

    /// Logs the phases of the commits since the last report once they are due ([`CommitPhases::is_due`]), and starts counting
    /// again: one event a second after a run of commits, or a minute into one, so the cost to the log is bounded whatever the load.
    fn report_phases(&mut self) {
        if !self.phases.is_due(self.now_us()) {
            return;
        }
        let phases = &self.phases;
        let figures = |phase: CommitPhase| phases.figures(phase);
        let (total, before, step) = (
            figures(CommitPhase::Total),
            figures(CommitPhase::BeforeStep),
            figures(CommitPhase::InStep),
        );
        let (write, sync, waited) = (
            figures(CommitPhase::LogWrite),
            figures(CommitPhase::LogSync),
            figures(CommitPhase::Waited),
        );
        let log = phases.log();
        tracing::info!(
            partition = self.id.get(),
            commits = phases.commits(),
            window_us = phases.window_us(),
            total_p50_us = total.0,
            total_p99_us = total.1,
            total_mean_us = total.2,
            before_step_p50_us = before.0,
            before_step_p99_us = before.1,
            before_step_mean_us = before.2,
            in_step_p50_us = step.0,
            in_step_p99_us = step.1,
            in_step_mean_us = step.2,
            log_write_p50_us = write.0,
            log_write_p99_us = write.1,
            log_write_mean_us = write.2,
            log_sync_p50_us = sync.0,
            log_sync_p99_us = sync.1,
            log_sync_mean_us = sync.2,
            waited_p50_us = waited.0,
            waited_p99_us = waited.1,
            waited_mean_us = waited.2,
            log_writes = log.writes(),
            log_written_bytes = log.written_bytes(),
            log_write_us = log.write_us(),
            log_syncs = log.syncs(),
            log_sync_us = log.sync_us(),
            "commit phases"
        );
        self.phases.clear();
    }

    /// Raises the discard version to what the retention allows at `now`, at most the visibility watermark: a read at the watermark
    /// is always answered, though the version bound of a closed epoch lie above it on an idle partition.
    fn raise_discard(&mut self, now: u64) {
        self.retention.closed(self.host.closed(), now);
        let Some(bound) = self.retention.due(now) else {
            return;
        };
        let target = bound.min(self.host.watermark());
        // An ask an open read held back asks again; one the store holds writes nothing.
        if target > self.host.discard_ts() {
            // A reader gone is a partition stopping, which discards nothing more.
            let _gone: Result<(), _> = self.discards.send(target);
        }
    }

    /// Tells the holder of `key` its transaction passed its limit.
    fn past_limit(&mut self, key: BoundKey) {
        let error = ultragraphdb_transactions_txn::TxnError::past_limit(self.host.bound_limit_ms());
        self.tell_ended(key, &error_reply(&error.into()));
    }

    /// Answers each parked request that the partition stops: nothing parked has reached the commit step (`PROTOCOL.md` §11.12).
    fn tell_parked_stopping(&mut self) {
        let reply = ErrorReply::new(ErrorCode::Unavailable, "the partition stopped");
        for ((_, stream), parked) in std::mem::take(&mut self.parked) {
            self.push_error(stream, &reply, &parked.pushes);
        }
    }

    /// Tells the holder of `key` its transaction ended with `reply`: on the stream of the request it waits on, dropping what is
    /// behind that, or else on its own.
    fn tell_ended(&mut self, key: BoundKey, reply: &ErrorReply) {
        if let Some(parked) = self.parked.remove(&key) {
            self.push_error(key.1, reply, &parked.pushes);
            let _ended = self.bound_tenant.remove(&key);
            return;
        }
        if let Some((_, pushes)) = self.bound_tenant.remove(&key) {
            self.push_error(key.1, reply, &pushes);
        }
    }

    /// Pushes `error` on `stream`, which it ends.
    fn push_error(&self, stream: u64, error: &ErrorReply, pushes: &Pushes) {
        let header = Header::new(self.cluster, Command::Error)
            .with_stream(stream)
            .with_flags(FLAG_LAST);
        if let Ok(frame) = error.frame(header) {
            // A connection gone has no one to tell.
            drop(pushes.unbounded_send((stream, frame)));
        }
    }

    /// Each commit's reply, durable now, to who asked. A one-frame write's conflict over what it tested runs it again instead.
    fn hand_out(&mut self) {
        let no_terms = TermTable::new();
        let answered = self.mark();
        for outcome in self.host.take_outcomes() {
            if let Some((contents, refusals)) = self.installing.remove(&outcome.ticket()) {
                self.version_decided(contents, &refusals, outcome.outcome().is_ok());
                continue;
            }
            let Some(mut committing) = self.committing.remove(&outcome.ticket()) else {
                continue;
            };
            if let Err(refusal) = outcome.outcome()
                && self.retry_one_frame(&mut committing, refusal)
            {
                continue;
            }
            let stream = committing.stream;
            let header = Header::new(self.cluster, Command::Commit)
                .with_stream(stream)
                .with_flags(FLAG_LAST);
            let frame = match outcome.outcome() {
                Ok(version) => {
                    self.phases
                        .record(&committing.taken, &committing.stepped, &answered);
                    let (token, results, terms) =
                        outcome.reply().map_or((None, &[][..], &no_terms), |reply| {
                            (reply.token(), reply.results(), reply.terms())
                        });
                    match committing.dropped.take() {
                        Some(body) => self.dropped_frame(stream, body, *version, token),
                        None => commit_body(
                            (committing.session, committing.request),
                            *version,
                            token,
                            results,
                            terms,
                        )
                        .frame(header),
                    }
                }
                Err(refusal) => error_reply(refusal).frame(
                    Header::new(self.cluster, Command::Error)
                        .with_stream(stream)
                        .with_flags(FLAG_LAST),
                ),
            };
            match frame {
                // A connection gone has no one to tell; what committed stays committed.
                Ok(frame) => drop(committing.pushes.unbounded_send((stream, frame))),
                Err(error) => tracing::error!(%error, "a commit's reply could not be framed"),
            }
        }
    }
}

/// Lets every other task of the core's runtime run once, waiting on nothing. A dropped future leaves nothing.
async fn yield_once() {
    let mut yielded = false;
    poll_fn(|cx| {
        if yielded {
            return Poll::Ready(());
        }
        yielded = true;
        cx.waker().wake_by_ref();
        Poll::Pending
    })
    .await;
}
