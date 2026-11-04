/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

pub(crate) mod partition_core_frames;
mod partition_core_remote;

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;
use std::sync::mpsc::{Receiver, Sender};
use std::time::Instant;

use futures::StreamExt;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};
use futures::channel::oneshot;
use ultragraphdb_cluster_partition::{
    BoundKey, Partition, PartitionHost, PartitionMessage, RemoteMessage,
};
use ultragraphdb_cluster_placement::PartitionId;
use ultragraphdb_common_role::{Answer, RoleClient, RoleCluster};
use ultragraphdb_core_ids::Epoch;
use ultragraphdb_error_role::RoleError;
use ultragraphdb_protocol_transport::{Address, ReceivedFrame};
use ultragraphdb_protocol_wire::{BodyBuilder, Command, ErrorReply, FLAG_LAST, Header, Reference};
use ultragraphdb_store_kv::{FjallStore, MokaListCache, ThreadExecutor};
use ultragraphdb_store_posting::CachedList;
use ultragraphdb_traits_store::Store;
use ultragraphdb_transactions_log::{CompioStorage, Wal};

use crate::types::core_message::{CoreMessage, Pushes};
use crate::types::refill_blocks::RefillBlocks;
use crate::utils::error_reply::error_reply;

/// A partition of the storage role, over fjall.
pub type StoragePartition =
    Partition<FjallStore, MokaListCache<CachedList>, ThreadExecutor, RefillBlocks>;

/// Its host, over the write-ahead log.
pub type StorageHost<'a> = PartitionHost<
    'a,
    Wal<CompioStorage>,
    FjallStore,
    MokaListCache<CachedList>,
    ThreadExecutor,
    RefillBlocks,
>;

/// How long the core waits for a message before it looks at its timers and at what other partitions of the process sent.
const TICK: std::time::Duration = std::time::Duration::from_millis(1);

/// What the core asks of the frontend, on a connection of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrontendAsk {
    /// The close of an epoch.
    Close(Epoch),
}

/// A request waiting for its references' answers, its frame held, which a write's changes borrow.
#[derive(Debug)]
struct Parked {
    frame: ReceivedFrame,
    done: oneshot::Sender<Answer>,
    namespace: u64,
    is_write: bool,
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
}

/// What the core's connections to others share: the address of each partition, and the connections it has open.
#[derive(Debug, Clone)]
pub struct Peers {
    /// The cluster this role belongs to.
    pub cluster: RoleCluster,
    /// The frontend's role address.
    pub frontend: Address,
    /// The name this role dials others under.
    pub name: String,
    /// Where an answering peer's messages reach this core.
    pub own: UnboundedSender<PartitionMessage>,
    /// The role address of each partition.
    pub addresses: Rc<RefCell<BTreeMap<u32, String>>>,
    /// The connections open, by address.
    pub open: Rc<RefCell<HashMap<String, RoleClient>>>,
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
    bound_namespace: BTreeMap<BoundKey, (u64, Pushes)>,
    parked: BTreeMap<BoundKey, Parked>,
    home_askings: BTreeMap<u64, HomeAsking>,
    next_asking: u64,
    committing: BTreeMap<u64, Committing>,
}

impl std::fmt::Debug for PartitionCore<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PartitionCore")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

/// What a core is made of.
pub struct CoreParts<'a> {
    /// What the partition is hosted over.
    pub host: StorageHost<'a>,
    /// The partition.
    pub id: PartitionId,
    /// The cluster every frame names.
    pub cluster: u64,
    /// When the core began, which its time counts from.
    pub clock: Instant,
    /// What its connections send it.
    pub inbox: UnboundedReceiver<CoreMessage>,
    /// What other partitions send it.
    pub remote: Receiver<RemoteMessage>,
    /// Where messages reach it.
    pub own: Sender<PartitionMessage>,
    /// The epoch closes delivered to it.
    pub closes: Receiver<PartitionMessage>,
    /// Where it asks the frontend.
    pub asks: UnboundedSender<FrontendAsk>,
    /// What its connections to others share.
    pub peers: Peers,
}

impl<'a> PartitionCore<'a> {
    /// The core of `parts`.
    pub fn new(parts: CoreParts<'a>) -> Self {
        Self {
            host: parts.host,
            id: parts.id,
            cluster: parts.cluster,
            clock: parts.clock,
            inbox: parts.inbox,
            remote: parts.remote,
            own: parts.own,
            closes: parts.closes,
            asks: parts.asks,
            peers: parts.peers,
            bound_namespace: BTreeMap::new(),
            parked: BTreeMap::new(),
            home_askings: BTreeMap::new(),
            next_asking: 1,
            committing: BTreeMap::new(),
        }
    }

    /// The microseconds since the core began.
    #[expect(
        clippy::disallowed_methods,
        reason = "the shell reads the clock and tells the partition's host, which reads none"
    )]
    fn now_us(&self) -> u64 {
        u64::try_from(self.clock.elapsed().as_micros()).unwrap_or(u64::MAX)
    }

    /// Serves until `stopping` says otherwise, then ends what is open and syncs what its log holds through its store.
    ///
    /// # Errors
    ///
    /// [`RoleError::io`] when the pipeline stops on its log or store.
    pub async fn run(
        mut self,
        stopping: &ultragraphdb_common_service::Stopping,
    ) -> Result<(), RoleError> {
        while !stopping.is_stopping() {
            if let Ok(Some(first)) = compio::time::timeout(TICK, self.inbox.next()).await {
                self.take(first);
                while let Ok(next) = self.inbox.try_recv() {
                    self.take(next);
                }
            }
            self.turn().await?;
        }
        let keys: Vec<BoundKey> = self.bound_namespace.keys().copied().collect();
        for key in keys {
            self.host.abort_bound(key);
        }
        self.turn().await?;
        self.host
            .partition()
            .applier()
            .quads()
            .store()
            .sync()
            .map_err(|error| RoleError::io(format!("the store: {error}")))
    }

    /// One turn of the host, and what that made durable handed on.
    async fn turn(&mut self) -> Result<(), RoleError> {
        while let Ok(message) = self.closes.try_recv() {
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
        self.resume_ready();
        self.hand_out();
        Ok(())
    }

    /// Tells the holder of `key` its transaction passed its limit.
    fn past_limit(&mut self, key: BoundKey) {
        let error = ultragraphdb_transactions_txn::TxnError::past_limit(self.host.bound_limit_ms());
        let reply = error_reply(&error.into());
        if let Some(parked) = self.parked.remove(&key) {
            drop(parked.done.send(Answer::Refuse(reply)));
            let _ended = self.bound_namespace.remove(&key);
            return;
        }
        if let Some((_, pushes)) = self.bound_namespace.remove(&key) {
            self.push_error(key.1, &reply, &pushes);
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

    /// Each commit's reply, durable now, to who asked.
    fn hand_out(&mut self) {
        for outcome in self.host.take_outcomes() {
            let Some(committing) = self.committing.remove(&outcome.ticket()) else {
                continue;
            };
            let header = Header::new(self.cluster, Command::Commit)
                .with_stream(committing.stream)
                .with_flags(FLAG_LAST);
            let frame = match outcome.outcome() {
                Ok(version) => {
                    let mut body = ultragraphdb_protocol_wire::Commit::new(
                        committing.session.to_be_bytes(),
                        committing.request,
                        *version,
                    );
                    if let Some(reply) = outcome.reply() {
                        if let Some(token) = reply.token() {
                            body = body.with_token(token.clone());
                        }
                        for result in reply.results() {
                            match result.node().and_then(|index| reply.terms().get(index)) {
                                Some(ultragraphdb_protocol_wire::RequestTerm::Iri(node)) => {
                                    body.push_node_result(node, result.created());
                                }
                                _ => body.push_result(*result),
                            }
                        }
                    }
                    body.frame(header)
                }
                Err(refusal) => error_reply(refusal).frame(
                    Header::new(self.cluster, Command::Error)
                        .with_stream(committing.stream)
                        .with_flags(FLAG_LAST),
                ),
            };
            match frame {
                // A connection gone has no one to tell; what committed stays committed.
                Ok(frame) => drop(committing.pushes.unbounded_send((committing.stream, frame))),
                Err(error) => tracing::error!(%error, "a commit's reply could not be framed"),
            }
        }
    }
}
