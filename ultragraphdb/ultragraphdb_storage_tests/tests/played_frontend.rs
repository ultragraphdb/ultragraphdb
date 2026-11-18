/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A frontend played by the test: a real storage process joins it as it would a real one, and it delivers a close out of its
//! order, breaks a subscription's connection or a catalog version's frames, or holds a Join's reply, which no real frontend can be
//! made to do. It records the leases and Joins it is asked, and the subscriptions' and catalog reads' connections that ended.

use std::collections::{BTreeMap, VecDeque};
use std::process::ExitStatus;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use compio::runtime::spawn;
use service_utils::ServiceUtil;
use ultragraphdb_cluster_placement::{NodeId, PlacementMap};
use ultragraphdb_common_role::{
    Answer, Push as RolePush, ROLE_ITERATIONS, RoleCredentials, RoleSecret, Server, Session,
    listen, serve_connection,
};
use ultragraphdb_core_ids::{Epoch, Tenant};
use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_transport::{Endpoint, ReceivedFrame};
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, BucketRange, Catalog, CatalogView, Command, EpochClose, ErrorCode,
    ErrorReply, FLAG_LAST, FrameView, Header, Join, JoinView, Lease, LeaseView, Placement,
    PreferredLeader, TenantRun,
};
use ultragraphdb_tests::{CLUSTER, SECRET, START_SECONDS};
use ultragraphdb_tool_bdd::{TestError, TestResult, loopback};

/// The offset above the target's port base (`ultragraphdb_tool_bdd::port`) the played frontend listens at.
const PLAYED_FRONTEND_OFFSET: u16 = 15;

/// Where the played frontend listens: a port of its own, so no other scenario's frontend meets it.
///
/// # Errors
///
/// When the target sets no port base.
pub fn played_frontend_address() -> TestResult<String> {
    loopback(PLAYED_FRONTEND_OFFSET)
}

/// The close it delivers first: a subscriber holding none expects close 1.
pub const OUT_OF_ORDER_EPOCH: u64 = 5;

/// What it says ending a subscription.
pub const TRIMMED: &str = "the close after 0 is no longer held";

/// The sequence the next lease starts at, so no two connections' blocks overlap.
static NEXT_SEQUENCE: AtomicU64 = AtomicU64::new(1);

/// How often a wait for the process's exit looks.
const POLL: Duration = Duration::from_millis(20);

/// What a subscription is sent.
#[derive(Debug, Clone, Copy)]
pub enum Push {
    /// A close out of its order.
    OutOfOrder,
    /// The refusal of a close no longer held, in place of a close.
    Ending,
    /// Nothing: the subscription stays open and quiet.
    Nothing,
    /// On the first subscription, a connection error: Error 1 on stream 0, and the connection closes (`PROTOCOL.md` §2.6).
    ConnectionError,
    /// On the first subscription, an Error on a stream the storage process has not opened (`PROTOCOL.md` §2.5, step 3).
    StrayAhead,
    /// On the first connection a catalog is read on, an Error on the stream catalog version 1 ended, sent between the two
    /// Catalog frames of version 2: a late frame (`PROTOCOL.md` §2.5, step 3; §11.2). Subscriptions stay quiet.
    LateInCatalog,
    /// On the first connection a catalog is read on, catalog version 1's second Catalog frame on the stream named, which
    /// answers no request (`PROTOCOL.md` §2.5, step 3). Subscriptions stay quiet.
    StrayInCatalog(u64),
}

/// The stream [`Push::StrayAhead`] sends on, past the subscription's, and the stream past catalog version 1's a
/// [`Push::StrayInCatalog`] may name.
pub const STRAY_AHEAD: u64 = 100;

/// The catalog versions of tenant 0 the played group holds, pending, under [`Push::LateInCatalog`] and [`Push::StrayInCatalog`];
/// a later version is one the tenant never held.
const PENDING_VERSIONS: u64 = 2;

/// The blocks asked for, by type prefix and partition.
pub type Leases = BTreeMap<(u16, u32), usize>;

/// What the played frontend's connections were asked, and whether a Join's reply is held.
#[derive(Debug, Default)]
pub struct Played {
    leases: Mutex<Leases>,
    /// The blocks asked for as the first Join found them.
    leases_at_join: Mutex<Option<Leases>>,
    /// The partitions each Join named, in order.
    joined: Mutex<Vec<u32>>,
    subscribes: AtomicUsize,
    /// The connections whose authentication was refused.
    refusals: AtomicUsize,
    /// The role commands its connections were asked, whatever their command.
    commands: AtomicUsize,
    holding: AtomicBool,
    /// Whether each Join after the first is held.
    holding_rejoins: AtomicBool,
    /// The connections a subscription was opened on that have ended.
    subscriptions_ended: AtomicUsize,
    /// The connections a catalog version was asked on, and of those the ones that have ended.
    catalog_connections: AtomicUsize,
    catalog_connections_ended: AtomicUsize,
    /// The catalog reads' connections that had ended when a catalog version was first asked on a second connection.
    catalog_ended_at_second: AtomicUsize,
}

impl Played {
    /// Holds each Join's reply until called again with false, or lets them go.
    pub fn hold_joins(&self, holding: bool) {
        self.holding.store(holding, Ordering::Release);
    }

    /// Holds each Join's reply after the first until called again with false, or lets them go.
    pub fn hold_rejoins(&self, holding: bool) {
        self.holding_rejoins.store(holding, Ordering::Release);
    }

    /// How many connections a subscription was opened on have ended.
    pub fn subscriptions_ended(&self) -> usize {
        self.subscriptions_ended.load(Ordering::Acquire)
    }

    /// How many connections a catalog version was asked on.
    pub fn catalog_connections(&self) -> usize {
        self.catalog_connections.load(Ordering::Acquire)
    }

    /// How many catalog reads' connections had ended when a catalog version was first asked on a second connection.
    pub fn catalog_ended_at_second(&self) -> usize {
        self.catalog_ended_at_second.load(Ordering::Acquire)
    }

    /// The partitions the Joins named, in order.
    pub fn joined(&self) -> TestResult<Vec<u32>> {
        Ok(self.joined.lock()?.clone())
    }

    /// The blocks asked for before the first Join, or nothing if none came.
    pub fn leases_at_join(&self) -> TestResult<Option<Leases>> {
        Ok(self.leases_at_join.lock()?.clone())
    }

    /// How many subscriptions were opened.
    pub fn subscribes(&self) -> usize {
        self.subscribes.load(Ordering::Acquire)
    }

    /// How many connections' authentication was refused.
    pub fn refusals(&self) -> usize {
        self.refusals.load(Ordering::Acquire)
    }

    /// How many role commands its connections were asked.
    pub fn commands(&self) -> usize {
        self.commands.load(Ordering::Acquire)
    }

    fn lease(&self, type_prefix: u16, partition: u32) {
        if let Ok(mut leases) = self.leases.lock() {
            let asked = leases.entry((type_prefix, partition)).or_default();
            *asked = asked.saturating_add(1);
        }
    }

    /// Records a Join naming `partitions`, then waits while Joins are held, or Joins after the first, at most as long as a start.
    async fn join(&self, partitions: &[u32]) {
        if let (Ok(leases), Ok(mut at_join)) = (self.leases.lock(), self.leases_at_join.lock()) {
            let _first: &mut Leases = at_join.get_or_insert_with(|| leases.clone());
        }
        let mut rejoin = false;
        if let Ok(mut joined) = self.joined.lock() {
            rejoin = !joined.is_empty();
            joined.extend_from_slice(partitions);
        }
        for _ in 0..START_SECONDS * 50 {
            let held = self.holding.load(Ordering::Acquire)
                || (rejoin && self.holding_rejoins.load(Ordering::Acquire));
            if !held {
                return;
            }
            compio::time::sleep(POLL).await;
        }
    }
}

/// One connection at the played frontend.
#[derive(Debug)]
struct PlayedFrontend {
    subscribed: Option<u64>,
    /// Whether a subscription was opened on this connection, and whether it was the first of all.
    subscriber: bool,
    first: bool,
    /// Which connection a catalog version was asked on this one is, counted from 0, once one was.
    catalog_connection: Option<usize>,
    /// What [`Session::next_push`] sends before anything else, in order.
    queued: VecDeque<(u64, RolePush)>,
    push: Push,
    played: Arc<Played>,
}

impl Drop for PlayedFrontend {
    fn drop(&mut self) {
        if self.subscriber {
            let _ended: usize = self
                .played
                .subscriptions_ended
                .fetch_add(1, Ordering::AcqRel);
        }
        if self.catalog_connection.is_some() {
            let _ended: usize = self
                .played
                .catalog_connections_ended
                .fetch_add(1, Ordering::AcqRel);
        }
    }
}

impl PlayedFrontend {
    /// The answer to the Catalog request `view` on `stream`: a version the tenant never held refused with code 7, as the group
    /// refuses one; under [`Push::LateInCatalog`] and [`Push::StrayInCatalog`], versions 1 and 2 pending, each in one frame
    /// but the one broken on the first connection a catalog is read on, whose frames are queued for [`Session::next_push`].
    fn catalog(&mut self, stream: u64, view: &FrameView<'_>) -> Answer {
        let asked = match CatalogView::from_frame(view) {
            Ok(asked) => asked,
            Err(rule) => return Answer::Refuse(ErrorReply::protocol(&rule)),
        };
        let played = &self.played;
        let connection = *self.catalog_connection.get_or_insert_with(|| {
            let connection = played.catalog_connections.fetch_add(1, Ordering::AcqRel);
            if connection == 1 {
                let ended = played.catalog_connections_ended.load(Ordering::Acquire);
                played
                    .catalog_ended_at_second
                    .store(ended, Ordering::Release);
            }
            connection
        });
        let version = asked.version();
        let broken = match self.push {
            Push::LateInCatalog if version <= PENDING_VERSIONS => {
                (connection == 0 && version == 2).then_some((stream.saturating_sub(1), true))
            }
            Push::StrayInCatalog(stray) if version <= PENDING_VERSIONS => {
                (connection == 0 && version == 1).then_some((stray, false))
            }
            _ => {
                // The played frontend holds no catalog version, so it refuses each as the group refuses one a tenant never held.
                return Answer::Refuse(ErrorReply::new(
                    ErrorCode::Refused,
                    "the played frontend holds no catalog version",
                ));
            }
        };
        let pending = Catalog::reply(asked.tenant(), version, 0, 1, Vec::new());
        let frame = |on: u64, flags: u8| {
            pending.frame(
                Header::new(CLUSTER, Command::Catalog)
                    .with_stream(on)
                    .with_flags(flags),
            )
        };
        let Some((other, late)) = broken else {
            return match frame(stream, FLAG_LAST) {
                Ok(whole) => Answer::Reply(whole),
                Err(rule) => Answer::Refuse(ErrorReply::protocol(&rule)),
            };
        };
        let last_on = if late { stream } else { other };
        let (first, last) = match (frame(stream, 0), frame(last_on, FLAG_LAST)) {
            (Ok(first), Ok(last)) => (first, last),
            (Err(rule), _) | (_, Err(rule)) => return Answer::Refuse(ErrorReply::protocol(&rule)),
        };
        // Each frame is queued on the version's stream, whatever stream its header names, so a stray one leaves in its turn.
        self.queued.push_back((stream, RolePush::Frame(first)));
        if late {
            let refusal = ErrorReply::new(ErrorCode::Unavailable, "a late refusal");
            self.queued.push_back((other, RolePush::Refuse(refusal)));
        }
        self.queued.push_back((stream, RolePush::Frame(last)));
        Answer::Open
    }
}

/// `body` as the reply on `stream`.
fn built<B: BodyBuilder>(body: &B, stream: u64, command: Command) -> Answer {
    let header = Header::new(CLUSTER, command)
        .with_stream(stream)
        .with_flags(FLAG_LAST);
    match body.frame(header) {
        Ok(frame) => Answer::Reply(frame),
        Err(rule) => Answer::Refuse(ErrorReply::protocol(&rule)),
    }
}

/// The even placement over one partition, as a real frontend's first start forms it.
fn placement() -> TestResult<Placement> {
    let map = PlacementMap::even(1, NodeId::new(1))?;
    let mut reply = Placement::reply(0, 0, 1, 0);
    reply.push_run(TenantRun::new(0, Tenant::new(0).bucket_bits()));
    for (range, partition) in map.ranges_at(Epoch::ZERO) {
        reply.push_range(BucketRange::new(
            range.start(),
            range.end(),
            partition.get(),
        ));
        if let Some(leader) = map.preferred_leader(partition) {
            reply.push_leader(PreferredLeader::new(partition.get(), leader.get()));
        }
    }
    Ok(reply)
}

impl Session for PlayedFrontend {
    fn authentication_refused(&mut self) {
        let _refused: usize = self.played.refusals.fetch_add(1, Ordering::AcqRel);
    }

    async fn answer(&mut self, stream: u64, frame: ReceivedFrame) -> Answer {
        let _asked: usize = self.played.commands.fetch_add(1, Ordering::AcqRel);
        let view = match frame.view() {
            Ok(view) => view,
            Err(rule) => return Answer::Refuse(ErrorReply::protocol(&rule)),
        };
        let command = view.command();
        if command == Command::Join {
            return match JoinView::from_frame(&view) {
                Ok(join) => {
                    let partitions = (0..join.partition_count())
                        .filter_map(|index| join.partition(index))
                        .collect::<Vec<u32>>();
                    self.played.join(&partitions).await;
                    let held = Join::storage(join.address(), partitions);
                    built(&held, stream, command)
                }
                Err(rule) => Answer::Refuse(ErrorReply::protocol(&rule)),
            };
        }
        if command == Command::Placement {
            return match placement() {
                Ok(reply) => built(&reply, stream, command),
                Err(error) => {
                    Answer::Refuse(ErrorReply::new(ErrorCode::Unavailable, error.to_string()))
                }
            };
        }
        if command == Command::Lease {
            return match LeaseView::from_frame(&view) {
                Ok(asked) => {
                    self.played.lease(asked.type_prefix(), asked.partition());
                    let start = NEXT_SEQUENCE.fetch_add(asked.size(), Ordering::Relaxed);
                    let reply =
                        Lease::request(asked.type_prefix(), asked.partition(), asked.size())
                            .with_block(start);
                    built(&reply, stream, command)
                }
                Err(rule) => Answer::Refuse(ErrorReply::protocol(&rule)),
            };
        }
        if command == Command::Subscribe {
            let opened: usize = self.played.subscribes.fetch_add(1, Ordering::AcqRel);
            self.subscribed = Some(stream);
            self.subscriber = true;
            self.first = opened == 0;
            return Answer::Open;
        }
        if command == Command::Catalog {
            return self.catalog(stream, &view);
        }
        Answer::Refuse(ErrorReply::new(
            ErrorCode::Unavailable,
            format!("command {} is not played", command.number()),
        ))
    }

    async fn next_push(&mut self) -> (u64, RolePush) {
        if let Some(queued) = self.queued.pop_front() {
            return queued;
        }
        let Some(stream) = self.subscribed.take() else {
            return std::future::pending().await;
        };
        let close = match self.push {
            Push::Ending => ErrorReply::new(ErrorCode::Protocol, TRIMMED).frame(
                Header::new(CLUSTER, Command::Error)
                    .with_stream(stream)
                    .with_flags(FLAG_LAST),
            ),
            Push::OutOfOrder => EpochClose::new(OUT_OF_ORDER_EPOCH, 1)
                .frame(Header::new(CLUSTER, Command::EpochClose).with_stream(stream)),
            Push::ConnectionError if self.first => {
                let rule = ProtocolError::command_not_served(Command::Subscribe.number());
                return (stream, RolePush::Close(rule));
            }
            Push::StrayAhead if self.first => {
                let stray = ErrorReply::new(ErrorCode::Unavailable, "a stray refusal");
                return (stream + STRAY_AHEAD, RolePush::Refuse(stray));
            }
            Push::Nothing
            | Push::ConnectionError
            | Push::StrayAhead
            | Push::LateInCatalog
            | Push::StrayInCatalog(_) => {
                return std::future::pending().await;
            }
        };
        match close {
            Ok(frame) => (stream, RolePush::Frame(frame)),
            Err(_) => std::future::pending().await,
        }
    }
}

/// Plays a frontend on this runtime, its subscriptions sent `push`: what its connections are asked is recorded in the
/// [`Played`] it returns.
pub async fn play_frontend(push: Push) -> TestResult<Arc<Played>> {
    let file = std::env::temp_dir().join(format!("played-frontend-{}", std::process::id()));
    std::fs::write(&file, SECRET)?;
    let held: RoleCredentials = RoleSecret::read(&file.display().to_string())?.credentials()?;
    let listener = listen(&format!("tcp://{}", played_frontend_address()?)).await?;
    let server =
        Server::new(CLUSTER, "frontend", Endpoint::Frontend)?.with_iterations(ROLE_ITERATIONS);
    let played = Arc::new(Played::default());
    let recorded = Arc::clone(&played);
    spawn(async move {
        while let Ok(carrier) = listener.accept().await {
            let (server, held) = (server.clone(), held.clone());
            let session = PlayedFrontend {
                subscribed: None,
                subscriber: false,
                first: false,
                catalog_connection: None,
                queued: VecDeque::new(),
                push,
                played: Arc::clone(&recorded),
            };
            spawn(async move {
                serve_connection(carrier, &server, held, session).await;
            })
            .detach();
        }
    })
    .detach();
    Ok(played)
}

/// Waits until `seen` holds, while this runtime goes on playing, at most as long as a start.
pub async fn until_seen(mut seen: impl FnMut() -> TestResult<bool>, what: &str) -> TestResult {
    for _ in 0..START_SECONDS * 50 {
        if seen()? {
            return Ok(());
        }
        compio::time::sleep(POLL).await;
    }
    Err(TestError::new(format!(
        "{what} did not happen within {START_SECONDS} s"
    )))
}

/// Waits for `pid` to exit, while this runtime goes on playing: how it did, or nothing if it runs on.
pub async fn exit_of(util: &ServiceUtil, pid: u32) -> Option<ExitStatus> {
    for _ in 0..START_SECONDS * 50 {
        if let Ok(status) = util.wait_for_exit(pid, Duration::ZERO) {
            return Some(status);
        }
        compio::time::sleep(POLL).await;
    }
    None
}
