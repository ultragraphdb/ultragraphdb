/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A frontend played by the test: a real storage process joins it as it would a real one, and it delivers a close out of its
//! order, or holds a Join's reply, which no real frontend can be made to do. It records the leases and Joins it is asked.

use std::collections::BTreeMap;
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
use ultragraphdb_protocol_transport::{Endpoint, ReceivedFrame};
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, BucketRange, Command, EpochClose, ErrorCode, ErrorReply, FLAG_LAST,
    Header, Join, JoinView, Lease, LeaseView, Placement, PreferredLeader, TenantRun,
};
use ultragraphdb_tests::{CLUSTER, SECRET, START_SECONDS};
use ultragraphdb_tool_bdd::{TestError, TestResult};

/// Where the played frontend listens: a port of its own, so no other scenario's frontend meets it.
pub const PLAYED_FRONTEND_ADDRESS: &str = "127.0.0.1:17095";

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
}

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
    holding: AtomicBool,
}

impl Played {
    /// Holds each Join's reply until called again with false, or lets them go.
    pub fn hold_joins(&self, holding: bool) {
        self.holding.store(holding, Ordering::Release);
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

    fn lease(&self, type_prefix: u16, partition: u32) {
        if let Ok(mut leases) = self.leases.lock() {
            let asked = leases.entry((type_prefix, partition)).or_default();
            *asked = asked.saturating_add(1);
        }
    }

    /// Records a Join naming `partitions`, then waits while Joins are held, at most as long as a start.
    async fn join(&self, partitions: &[u32]) {
        if let (Ok(leases), Ok(mut at_join)) = (self.leases.lock(), self.leases_at_join.lock()) {
            let _first: &mut Leases = at_join.get_or_insert_with(|| leases.clone());
        }
        if let Ok(mut joined) = self.joined.lock() {
            joined.extend_from_slice(partitions);
        }
        for _ in 0..START_SECONDS * 50 {
            if !self.holding.load(Ordering::Acquire) {
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
    push: Push,
    played: Arc<Played>,
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
    async fn answer(&mut self, stream: u64, frame: ReceivedFrame) -> Answer {
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
            let _opened: usize = self.played.subscribes.fetch_add(1, Ordering::AcqRel);
            self.subscribed = Some(stream);
            return Answer::Open;
        }
        if command == Command::Catalog {
            // The played frontend holds no catalog version, so it refuses each as the group refuses one a tenant never held.
            return Answer::Refuse(ErrorReply::new(
                ErrorCode::Refused,
                "the played frontend holds no catalog version",
            ));
        }
        Answer::Refuse(ErrorReply::new(
            ErrorCode::Unavailable,
            format!("command {} is not played", command.number()),
        ))
    }

    async fn next_push(&mut self) -> (u64, RolePush) {
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
            Push::Nothing => return std::future::pending().await,
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
    let listener = listen(&format!("tcp://{PLAYED_FRONTEND_ADDRESS}")).await?;
    let server =
        Server::new(CLUSTER, "frontend", Endpoint::Frontend)?.with_iterations(ROLE_ITERATIONS);
    let played = Arc::new(Played::default());
    let recorded = Arc::clone(&played);
    spawn(async move {
        while let Ok(carrier) = listener.accept().await {
            let (server, held) = (server.clone(), held.clone());
            let session = PlayedFrontend {
                subscribed: None,
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
