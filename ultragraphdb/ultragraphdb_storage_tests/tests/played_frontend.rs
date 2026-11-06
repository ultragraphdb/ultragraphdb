/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A frontend played by the test: a real storage process joins it as it would a real one, and it delivers a close out of its
//! order, which no real frontend can be made to do.

use std::process::ExitStatus;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use compio::runtime::spawn;
use service_utils::ServiceUtil;
use ultragraphdb_cluster_placement::{NodeId, PlacementMap};
use ultragraphdb_common_role::{
    Answer, ROLE_ITERATIONS, RoleCredentials, RoleSecret, Server, Session, listen, serve_connection,
};
use ultragraphdb_core_ids::{Epoch, Tenant};
use ultragraphdb_protocol_transport::{Endpoint, ReceivedFrame};
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, BucketRange, Command, EpochClose, ErrorCode, ErrorReply, FLAG_LAST,
    Header, Join, JoinView, Lease, LeaseView, Placement, PreferredLeader, TenantRun,
};
use ultragraphdb_tests::{CLUSTER, SECRET, START_SECONDS};
use ultragraphdb_tool_bdd::TestResult;

/// Where the played frontend listens: a port of its own, so no other scenario's frontend meets it.
pub const PLAYED_FRONTEND_ADDRESS: &str = "127.0.0.1:17095";

/// The close it delivers first: a subscriber holding none expects close 1.
pub const OUT_OF_ORDER_EPOCH: u64 = 5;

/// The sequence the next lease starts at, so no two connections' blocks overlap.
static NEXT_SEQUENCE: AtomicU64 = AtomicU64::new(1);

/// How often a wait for the process's exit looks.
const POLL: Duration = Duration::from_millis(20);

/// One connection at the played frontend.
#[derive(Debug, Default)]
struct PlayedFrontend {
    subscribed: Option<u64>,
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
            self.subscribed = Some(stream);
            return Answer::Open;
        }
        Answer::Refuse(ErrorReply::new(
            ErrorCode::Unavailable,
            format!("command {} is not played", command.number()),
        ))
    }

    async fn next_push(&mut self) -> (u64, Vec<u8>) {
        let Some(stream) = self.subscribed.take() else {
            return std::future::pending().await;
        };
        let close = EpochClose::new(OUT_OF_ORDER_EPOCH, 1)
            .frame(Header::new(CLUSTER, Command::EpochClose).with_stream(stream));
        match close {
            Ok(frame) => (stream, frame),
            Err(_) => std::future::pending().await,
        }
    }
}

/// Plays a frontend on this runtime.
pub async fn play_frontend() -> TestResult {
    let file = std::env::temp_dir().join(format!("played-frontend-{}", std::process::id()));
    std::fs::write(&file, SECRET)?;
    let held: RoleCredentials = RoleSecret::read(&file.display().to_string())?.credentials()?;
    let listener = listen(&format!("tcp://{PLAYED_FRONTEND_ADDRESS}")).await?;
    let server =
        Server::new(CLUSTER, "frontend", Endpoint::Frontend)?.with_iterations(ROLE_ITERATIONS);
    spawn(async move {
        while let Ok(carrier) = listener.accept().await {
            let (server, held) = (server.clone(), held.clone());
            spawn(async move {
                serve_connection(carrier, &server, held, PlayedFrontend::default()).await;
            })
            .detach();
        }
    })
    .detach();
    Ok(())
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
