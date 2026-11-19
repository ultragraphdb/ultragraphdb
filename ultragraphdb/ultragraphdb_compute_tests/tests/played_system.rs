/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A system group played by the test: real storage processes join it, and the compute process asks it. It closes each partition's
//! epochs as a scenario says, which no real system group can be made to do per partition, and it tells compute that partitions
//! listen where the test's tap does, so a scenario sees every Scan compute sends. It answers compute's endpoint as the group serves
//! compute (`PROTOCOL.md` §11.12a): the administrator's SCRAM exchange, each Session, each Check passed, each Ended.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use compio::runtime::{Runtime, spawn};
use ultragraphdb_cluster_placement::{NodeId, PlacementMap};
use ultragraphdb_common_role::{
    Answer, Push, ROLE_ITERATIONS, RoleCredentials, RoleSecret, Server, Session, listen,
    serve_connection,
};
use ultragraphdb_core_ids::Epoch;
use ultragraphdb_protocol_scram::ScramVerifier;
use ultragraphdb_protocol_transport::{Endpoint, ReceivedFrame};
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, BucketRange, Challenge, ChallengeView, Check, CheckOutcome, CheckView,
    Command, EpochClose, ErrorCode, ErrorReply, FLAG_LAST, FrameView, Header, Join, JoinView,
    Lease, LeaseView, Members, Placement, PreferredLeader, Session as SessionReply, SessionView,
    TenantRun, Verify, VerifyView,
};
use ultragraphdb_tests::{CLUSTER, SECRET, scratch};
use ultragraphdb_tool_bdd::TestResult;

use crate::endpoint_steps::{ADMIN, PASSWORD};
use crate::scene::PRINCIPAL;

/// How long between two epochs a partition is not held at.
const EPOCH_MS: u128 = 4;

/// How often a played connection looks whether a close is due.
const LOOK: Duration = Duration::from_millis(1);

/// The tenants the played placement lists a run for; every scenario's lies among them.
const TENANTS_LISTED: u64 = 1_024;

/// The SCRAM iterations of the administrator's verifier and of an unknown name's.
const SCRAM_ITERATIONS: u32 = 4_096;

/// The secret an unknown name's verifier derives from, so the exchange refuses it as it refuses a wrong password.
const UNKNOWN_SECRET: [u8; 32] = [3; 32];

/// Which epochs each partition has been closed through: time's epoch, unless a scenario holds it at an earlier one.
#[derive(Debug)]
pub struct EpochClock {
    started: Instant,
    held: [AtomicU64; 8],
    sequences: AtomicU64,
}

impl EpochClock {
    /// A clock starting now, holding no partition.
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            started: now(),
            held: Default::default(),
            sequences: AtomicU64::new(1),
        })
    }

    /// The epoch closed by now where nothing holds a partition.
    pub fn latest(&self) -> u64 {
        let elapsed = now().duration_since(self.started).as_millis();
        u64::try_from(elapsed / EPOCH_MS).unwrap_or(u64::MAX / 2) + 1
    }

    /// The epoch `partition` is closed through by now.
    fn target(&self, partition: u32) -> u64 {
        let latest = self.latest();
        let held = self
            .held
            .get(partition as usize)
            .map_or(0, |held| held.load(Ordering::Acquire));
        if held == 0 { latest } else { latest.min(held) }
    }

    /// Holds `partition` at `epoch`: it is closed through no later one.
    pub fn hold(&self, partition: u32, epoch: u64) {
        if let Some(held) = self.held.get(partition as usize) {
            held.store(epoch, Ordering::Release);
        }
    }

    /// Lets `partition` go on with time.
    pub fn release(&self, partition: u32) {
        self.hold(partition, 0);
    }

    fn next_block(&self, size: u64) -> u64 {
        self.sequences.fetch_add(size, Ordering::Relaxed)
    }
}

#[expect(
    clippy::disallowed_methods,
    reason = "the played system group's own clock, in the test's shell"
)]
fn now() -> Instant {
    Instant::now()
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

/// The even placement over `partitions`, as a real system group's first start forms it.
fn placement(partitions: u32) -> TestResult<Placement> {
    let map = PlacementMap::even(partitions, NodeId::new(1))?;
    let mut reply = Placement::reply(0, 0, 1, 0);
    for (range, partition) in map.ranges_at(Epoch::ZERO) {
        reply.push_range(BucketRange::new(
            range.start(),
            range.end(),
            partition.get(),
        ));
    }
    for partition in 1..=partitions {
        reply.push_leader(PreferredLeader::new(partition, 1));
    }
    for tenant in 0..TENANTS_LISTED {
        let run = ultragraphdb_core_ids::Tenant::new(tenant).bucket_bits();
        reply.push_run(TenantRun::new(tenant, run));
    }
    Ok(reply)
}

/// What one played system group answers: where each partition listens, as what joins it reads it.
#[derive(Debug, Clone)]
pub struct Played {
    pub partitions: u32,
    pub clock: Arc<EpochClock>,
    pub listed: Vec<(u32, String)>,
}

/// One connection at a played system group.
#[derive(Debug)]
struct PlayedConnection {
    played: Played,
    admin: ScramVerifier,
    partition: u32,
    subscribed: Option<u64>,
    delivered: u64,
}

impl PlayedConnection {
    /// The principal and verifier of `name`: the administrator's, or an unknown name's, principal 0.
    fn verifier_of(&self, name: &str) -> (u64, ScramVerifier) {
        if name == ADMIN {
            (PRINCIPAL, self.admin.clone())
        } else {
            (
                0,
                ScramVerifier::unknown(&UNKNOWN_SECRET, name, SCRAM_ITERATIONS),
            )
        }
    }

    /// The answer to an endpoint's command: `None` for a command the group answers otherwise.
    fn endpoint_answer(&self, view: &FrameView<'_>, stream: u64) -> Option<Answer> {
        let command = view.command();
        let answer = if command == Command::Challenge {
            match ChallengeView::from_frame(view) {
                Ok(challenge) => {
                    let (_, verifier) = self.verifier_of(challenge.name());
                    let reply = Challenge::request(challenge.name())
                        .with_salt(verifier.iterations(), verifier.salt().to_vec());
                    built(&reply, stream, command)
                }
                Err(rule) => Answer::Refuse(ErrorReply::protocol(&rule)),
            }
        } else if command == Command::Verify {
            match VerifyView::from_frame(view) {
                Ok(verify) => {
                    let (principal, verifier) = self.verifier_of(verify.name());
                    let request =
                        Verify::request(verify.name(), verify.auth_message(), *verify.proof());
                    let reply = match verifier.sign(verify.auth_message(), verify.proof()) {
                        Some(signature) if principal != 0 => {
                            request.with_verified(principal, signature)
                        }
                        Some(_) | None => request,
                    };
                    built(&reply, stream, command)
                }
                Err(rule) => Answer::Refuse(ErrorReply::protocol(&rule)),
            }
        } else if command == Command::Session {
            match SessionView::from_frame(view) {
                Ok(session) => built(
                    &SessionReply::new(session.session(), session.principal()),
                    stream,
                    command,
                ),
                Err(rule) => Answer::Refuse(ErrorReply::protocol(&rule)),
            }
        } else if command == Command::Check {
            match CheckView::from_frame(view) {
                Ok(checks) => {
                    let passed = (0..checks.len())
                        .filter_map(|at| checks.get(at))
                        .map(|check| check.with_outcome(CheckOutcome::Passed))
                        .collect();
                    built(&Check::new(passed), stream, command)
                }
                Err(rule) => Answer::Refuse(ErrorReply::protocol(&rule)),
            }
        } else if command == Command::Ended {
            Answer::Done
        } else {
            return None;
        };
        Some(answer)
    }
}

impl Session for PlayedConnection {
    async fn answer(&mut self, stream: u64, frame: ReceivedFrame) -> Answer {
        let view = match frame.view() {
            Ok(view) => view,
            Err(rule) => return Answer::Refuse(ErrorReply::protocol(&rule)),
        };
        if let Some(answer) = self.endpoint_answer(&view, stream) {
            return answer;
        }
        let command = view.command();
        if command == Command::Join {
            let Ok(join) = JoinView::from_frame(&view) else {
                return Answer::Refuse(ErrorReply::new(ErrorCode::Protocol, "a Join"));
            };
            self.partition = join.partition(0).unwrap_or(0);
            let partitions: Vec<u32> = (0..join.partition_count())
                .filter_map(|index| join.partition(index))
                .collect();
            return if partitions.is_empty() {
                built(&Join::compute(join.address()), stream, command)
            } else {
                built(&Join::storage(join.address(), partitions), stream, command)
            };
        }
        if command == Command::Members {
            let mut members = Members::new();
            for (partition, address) in &self.played.listed {
                members.push_storage(*partition, address.clone());
            }
            return built(&members, stream, command);
        }
        if command == Command::Placement {
            return match placement(self.played.partitions) {
                Ok(reply) => built(&reply, stream, command),
                Err(error) => {
                    Answer::Refuse(ErrorReply::new(ErrorCode::Unavailable, error.to_string()))
                }
            };
        }
        if command == Command::Lease {
            return match LeaseView::from_frame(&view) {
                Ok(asked) => {
                    let start = self.played.clock.next_block(asked.size());
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
        if command == Command::CloseRequest {
            return Answer::Done;
        }
        if command == Command::Catalog {
            // The played system group holds no catalog version, so it refuses each as the group refuses one a tenant never held.
            return Answer::Refuse(ErrorReply::new(
                ErrorCode::Refused,
                "the played system group holds no catalog version",
            ));
        }
        Answer::Refuse(ErrorReply::new(
            ErrorCode::Unavailable,
            format!("command {} is not played", command.number()),
        ))
    }

    async fn next_push(&mut self) -> (u64, Push) {
        let Some(stream) = self.subscribed else {
            return std::future::pending().await;
        };
        loop {
            if self.played.clock.target(self.partition) > self.delivered {
                let epoch = self.delivered + 1;
                let close = EpochClose::new(epoch, epoch * 1_000)
                    .frame(Header::new(CLUSTER, Command::EpochClose).with_stream(stream));
                if let Ok(frame) = close {
                    self.delivered = epoch;
                    return (stream, Push::Frame(frame));
                }
            }
            compio::time::sleep(LOOK).await;
        }
    }
}

/// Serves `played` at the TCP port `port`, on a thread of the test's own, for the life of the test process.
#[expect(
    clippy::disallowed_methods,
    reason = "the played system group answers beside the scenarios, on a thread of the test's own"
)]
pub fn play(port: u32, played: Played) -> TestResult {
    let file = scratch(&format!("played-system-secret-{port}"));
    std::fs::write(&file, SECRET)?;
    let held: RoleCredentials = RoleSecret::read(&file.display().to_string())?.credentials()?;
    let server = Server::new(CLUSTER, "system", Endpoint::System)?.with_iterations(ROLE_ITERATIONS);
    let admin = ScramVerifier::derive(PASSWORD, b"the played administrator", SCRAM_ITERATIONS)?;
    drop(std::thread::spawn(move || -> TestResult {
        Runtime::new()?.block_on(async move {
            let listener = listen(&format!("tcp://127.0.0.1:{port}")).await?;
            while let Ok(carrier) = listener.accept().await {
                let (server, held, played, admin) =
                    (server.clone(), held.clone(), played.clone(), admin.clone());
                spawn(async move {
                    let connection = PlayedConnection {
                        played,
                        admin,
                        partition: 0,
                        subscribed: None,
                        delivered: 0,
                    };
                    serve_connection(carrier, &server, held, connection).await;
                })
                .detach();
            }
            Ok(())
        })
    }));
    Ok(())
}
