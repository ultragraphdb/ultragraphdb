/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A frontend the test plays at an address of its own on the test's runtime: it answers compute's Placement with the bucket
//! ranges it is given and its Members with the played partitions' addresses, each after a delay the test may set.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use compio::runtime::spawn;
use ultragraphdb_cluster_placement::{NodeId, PlacementMap};
use ultragraphdb_common_role::{
    Answer, ROLE_ITERATIONS, RoleCluster, Server, Session, serve_connection,
};
use ultragraphdb_core_ids::Epoch;
use ultragraphdb_protocol_transport::{Address, Endpoint, ReceivedFrame};
use ultragraphdb_protocol_transport_compio::Listener;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BucketRange, Command, ErrorCode, ErrorReply, FLAG_LAST, Header, Members,
    Placement, PreferredLeader,
};
use ultragraphdb_tool_bdd::TestResult;

/// The bucket ranges of the even placement over `partitions`, as a real frontend's first start forms it: each range's start, end
/// and partition.
pub fn even_ranges(partitions: u32) -> TestResult<Vec<(u32, u32, u32)>> {
    let map = PlacementMap::even(partitions, NodeId::new(1))?;
    Ok(map
        .ranges_at(Epoch::ZERO)
        .into_iter()
        .map(|(range, partition)| (range.start(), range.end(), partition.get()))
        .collect())
}

/// What a played frontend answers, and what it was asked.
#[derive(Debug, Clone)]
pub struct PlayedFrontend {
    address: String,
    ranges: Rc<[(u32, u32, u32)]>,
    storage: Rc<RefCell<Vec<(u32, String)>>>,
    compute: Rc<RefCell<Vec<String>>>,
    members_delay: Rc<Cell<Duration>>,
    members_asked: Rc<Cell<u32>>,
    placement_delay: Rc<Cell<Duration>>,
}

impl PlayedFrontend {
    /// A frontend of `cluster` answering the placement of `ranges`, listening at a port of its own on this runtime.
    pub async fn start(cluster: &RoleCluster, ranges: Vec<(u32, u32, u32)>) -> TestResult<Self> {
        let listener = Listener::bind(&Address::any_port("127.0.0.1")?).await?;
        let address = listener.local_address()?.to_string();
        let credentials = cluster.secret().credentials()?;
        let server = Server::new(cluster.id(), "frontend", Endpoint::Frontend)?
            .with_iterations(ROLE_ITERATIONS);
        let played = Self {
            address,
            ranges: Rc::from(ranges),
            storage: Rc::default(),
            compute: Rc::default(),
            members_delay: Rc::default(),
            members_asked: Rc::default(),
            placement_delay: Rc::default(),
        };
        let serving = played.clone();
        spawn(async move {
            while let Ok(carrier) = listener.accept().await {
                let session = FrontendSession {
                    played: serving.clone(),
                };
                let (server, credentials) = (server.clone(), credentials.clone());
                spawn(
                    async move { serve_connection(carrier, &server, credentials, session).await },
                )
                .detach();
            }
        })
        .detach();
        Ok(played)
    }

    /// Where it listens.
    pub fn address(&self) -> &str {
        &self.address
    }

    /// Lists `partition` as served at `address`.
    pub fn list(&self, partition: u32, address: &str) {
        self.storage
            .borrow_mut()
            .push((partition, address.to_string()));
    }

    /// Lists a compute process at `address`.
    pub fn list_compute(&self, address: &str) {
        self.compute.borrow_mut().push(address.to_string());
    }

    /// Answers each Members `delay` after it arrives.
    pub fn delay_members(&self, delay: Duration) {
        self.members_delay.set(delay);
    }

    /// Answers each Placement `delay` after it arrives.
    pub fn delay_placement(&self, delay: Duration) {
        self.placement_delay.set(delay);
    }

    /// How many Members it was asked.
    pub fn members_asked(&self) -> u32 {
        self.members_asked.get()
    }

    fn placement(&self) -> Placement {
        let mut reply = Placement::reply(0, 0, 1, 0);
        for (start, end, partition) in self.ranges.iter() {
            reply.push_range(BucketRange::new(*start, *end, *partition));
        }
        let mut leaders: Vec<u32> = self
            .ranges
            .iter()
            .map(|(_, _, partition)| *partition)
            .collect();
        leaders.sort_unstable();
        leaders.dedup();
        leaders
            .into_iter()
            .for_each(|partition| reply.push_leader(PreferredLeader::new(partition, 1)));
        reply
    }

    fn members(&self) -> Members {
        let mut members = Members::new();
        for address in self.compute.borrow().iter() {
            members.push_compute(address.clone());
        }
        for (partition, address) in self.storage.borrow().iter() {
            members.push_storage(*partition, address.clone());
        }
        members
    }
}

/// `body` as the reply on `stream` of `cluster`.
fn reply<B: BodyBuilder>(body: &B, cluster: u64, stream: u64) -> Answer {
    let header = Header::new(cluster, B::COMMAND)
        .with_stream(stream)
        .with_flags(FLAG_LAST);
    match body.frame(header) {
        Ok(frame) => Answer::Reply(frame),
        Err(rule) => Answer::Refuse(ErrorReply::protocol(&rule)),
    }
}

/// One connection at a played frontend.
#[derive(Debug)]
struct FrontendSession {
    played: PlayedFrontend,
}

impl Session for FrontendSession {
    async fn answer(&mut self, stream: u64, frame: ReceivedFrame) -> Answer {
        let view = match frame.view() {
            Ok(view) => view,
            Err(rule) => return Answer::Refuse(ErrorReply::protocol(&rule)),
        };
        let cluster = view.header().cluster();
        let command = view.command();
        if command == Command::Placement {
            compio::time::sleep(self.played.placement_delay.get()).await;
            return reply(&self.played.placement(), cluster, stream);
        }
        if command == Command::Members {
            self.played
                .members_asked
                .set(self.played.members_asked.get() + 1);
            compio::time::sleep(self.played.members_delay.get()).await;
            return reply(&self.played.members(), cluster, stream);
        }
        Answer::Refuse(ErrorReply::new(
            ErrorCode::Unavailable,
            format!("command {} is not played", command.number()),
        ))
    }
}
