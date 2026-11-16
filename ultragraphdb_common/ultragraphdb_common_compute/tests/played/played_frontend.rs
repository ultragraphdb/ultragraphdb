/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The system group the test plays at an address of its own on the test's runtime, where the frontend answered it until
//! the settings switch: it answers compute's Placement with the bucket ranges it is given, its Members with the played
//! partitions' addresses and the compute processes that joined, its Join by listing the compute process, and its Catalog
//! with the versions it holds, across frames within compute's `max_frame`, each after a delay the test may set. It may
//! stop answering, turning every connection away before its Welcome, as a group gone does.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
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
    BodyBuilder, BodyView, BucketRange, Catalog, CatalogView, Command, ErrorCode, ErrorReply,
    FLAG_LAST, FrameView, Header, Join, JoinView, MAX_FRAME_FLOOR, Members, Placement,
    PreferredLeader, RoleKind,
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

/// What the played group counted.
#[derive(Debug, Default)]
struct Counts {
    dials: Cell<u32>,
    members: Cell<u32>,
    placement: Cell<u32>,
    catalog: Cell<u32>,
    turned_away: Cell<u32>,
}

/// What a played group answers, and what it was asked.
#[derive(Debug, Clone)]
pub struct PlayedFrontend {
    address: String,
    ranges: Rc<[(u32, u32, u32)]>,
    storage: Rc<RefCell<Vec<(u32, String)>>>,
    compute: Rc<RefCell<Vec<String>>>,
    joins: Rc<RefCell<Vec<String>>>,
    versions: Rc<RefCell<Vec<Catalog>>>,
    members_delay: Rc<Cell<Duration>>,
    placement_delay: Rc<Cell<Duration>>,
    catalog_delay: Rc<Cell<Duration>>,
    answering: Rc<Cell<bool>>,
    /// The frames each version is answered in, or `None` for as many as its bytes take within `max_frame`.
    split: Rc<Cell<Option<usize>>>,
    counts: Rc<Counts>,
}

impl PlayedFrontend {
    /// A group of `cluster` answering the placement of `ranges`, listening at a port of its own on this runtime.
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
            joins: Rc::default(),
            versions: Rc::default(),
            members_delay: Rc::default(),
            placement_delay: Rc::default(),
            catalog_delay: Rc::default(),
            answering: Rc::new(Cell::new(true)),
            split: Rc::default(),
            counts: Rc::default(),
        };
        let serving = played.clone();
        spawn(async move {
            while let Ok(carrier) = listener.accept().await {
                counted(&serving.counts.dials);
                if !serving.answering.get() {
                    counted(&serving.counts.turned_away);
                    drop(carrier);
                    continue;
                }
                let session = FrontendSession {
                    played: serving.clone(),
                    max_frame: MAX_FRAME_FLOOR,
                    queued: VecDeque::new(),
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

    /// Lists no compute process, as a group restarted holds none until each joins again.
    pub fn forget_compute(&self) {
        self.compute.borrow_mut().clear();
    }

    /// Holds `version` of `tenant`'s catalog, its bytes `bytes`, applying from `epoch`.
    pub fn hold_version(&self, tenant: u64, version: u64, epoch: u64, bytes: Vec<u8>) {
        self.versions
            .borrow_mut()
            .push(Catalog::reply(tenant, version, epoch, 1, bytes));
    }

    /// Whether it answers: one that does not turns every connection away before its Welcome.
    pub fn answer(&self, answering: bool) {
        self.answering.set(answering);
    }

    /// Answers each version in `frames` frames, one byte to a frame and the frames past its bytes empty, the last flagged
    /// `LAST`: a group splitting a version as far as the protocol lets it.
    pub fn answer_in(&self, frames: usize) {
        self.split.set(Some(frames));
    }

    /// Answers each Members `delay` after it arrives.
    pub fn delay_members(&self, delay: Duration) {
        self.members_delay.set(delay);
    }

    /// Answers each Placement `delay` after it arrives.
    pub fn delay_placement(&self, delay: Duration) {
        self.placement_delay.set(delay);
    }

    /// Answers each Catalog `delay` after it arrives.
    pub fn delay_catalog(&self, delay: Duration) {
        self.catalog_delay.set(delay);
    }

    /// How many Members it was asked.
    pub fn members_asked(&self) -> u32 {
        self.counts.members.get()
    }

    /// How many Placements it was asked.
    pub fn placement_asked(&self) -> u32 {
        self.counts.placement.get()
    }

    /// How many Catalogs it was asked.
    pub fn catalog_asked(&self) -> u32 {
        self.counts.catalog.get()
    }

    /// The addresses of the Joins it took, in order.
    pub fn joins(&self) -> Vec<String> {
        self.joins.borrow().clone()
    }

    /// How many connections it accepted, those it turned away among them.
    pub fn dials(&self) -> u32 {
        self.counts.dials.get()
    }

    /// How many connections it turned away while it did not answer.
    pub fn turned_away(&self) -> u32 {
        self.counts.turned_away.get()
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
        let mut compute = self.compute.borrow().clone();
        compute.sort_unstable();
        compute.dedup();
        for address in compute {
            members.push_compute(address);
        }
        for (partition, address) in self.storage.borrow().iter() {
            members.push_storage(*partition, address.clone());
        }
        members
    }

    /// The Join of a compute process `view` names, taken: the process is listed from now on.
    fn join(&self, view: &JoinView<'_>) -> Join {
        let address = view.address().to_string();
        self.joins.borrow_mut().push(address.clone());
        if view.kind() == RoleKind::Compute {
            self.list_compute(&address);
        }
        Join::compute(address)
    }

    /// The version `asked` names, the tenant's latest for `version` 0.
    fn version(&self, asked: &CatalogView<'_>) -> Option<Catalog> {
        self.versions
            .borrow()
            .iter()
            .filter(|held| held.tenant() == asked.tenant())
            .filter(|held| asked.version() == 0 || held.version() == asked.version())
            .max_by_key(|held| held.version())
            .cloned()
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

/// One connection at a played group.
#[derive(Debug)]
struct FrontendSession {
    played: PlayedFrontend,
    max_frame: usize,
    /// The frames of a Catalog answer after its first, pushed in order.
    queued: VecDeque<(u64, Vec<u8>)>,
}

impl FrontendSession {
    /// A Catalog request's answer: the version's frames within the connection's `max_frame`, the first the reply and the
    /// rest queued; code 7 naming the version for one it does not hold.
    fn catalog(&mut self, view: &FrameView<'_>, stream: u64) -> Answer {
        let asked = match CatalogView::from_frame(view) {
            Ok(asked) => asked,
            Err(rule) => return Answer::Refuse(ErrorReply::protocol(&rule)),
        };
        let Some(version) = self.played.version(&asked) else {
            return Answer::Refuse(
                ErrorReply::new(ErrorCode::Refused, "no such version").with_detail(asked.version()),
            );
        };
        let header = Header::new(view.header().cluster(), Command::Catalog).with_stream(stream);
        let built = match self.played.split.get() {
            Some(count) => split(&version, header, count),
            None => version.frames(header, self.max_frame),
        };
        let mut frames = match built {
            Ok(frames) => frames.into_iter(),
            Err(rule) => return Answer::Refuse(ErrorReply::protocol(&rule)),
        };
        let Some(first) = frames.next() else {
            return Answer::Done;
        };
        self.queued.extend(frames.map(|frame| (stream, frame)));
        Answer::Reply(first)
    }
}

/// `version` in `count` frames of `header`, one byte to a frame, the frames past its bytes empty, the last alone flagged
/// `LAST`.
fn split(
    version: &Catalog,
    header: Header,
    count: usize,
) -> Result<Vec<Vec<u8>>, ultragraphdb_error_protocol::ProtocolError> {
    (0..count)
        .map(|index| {
            let byte = version
                .bytes()
                .get(index..index + 1)
                .unwrap_or_default()
                .to_vec();
            let part = Catalog::reply(
                version.tenant(),
                version.version(),
                version.epoch(),
                version.position(),
                byte,
            );
            let flags = if index + 1 == count { FLAG_LAST } else { 0 };
            part.frame(header.clone().with_flags(flags))
        })
        .collect()
}

/// `count` raised by one.
fn counted(count: &Cell<u32>) {
    count.set(count.get() + 1);
}

impl Session for FrontendSession {
    async fn answer(&mut self, stream: u64, frame: ReceivedFrame) -> Answer {
        let view = match frame.view() {
            Ok(view) => view,
            Err(rule) => return Answer::Refuse(ErrorReply::protocol(&rule)),
        };
        let cluster = view.header().cluster();
        let command = view.command();
        let played = self.played.clone();
        if command == Command::Placement {
            counted(&played.counts.placement);
            compio::time::sleep(played.placement_delay.get()).await;
            return reply(&played.placement(), cluster, stream);
        }
        if command == Command::Members {
            counted(&played.counts.members);
            compio::time::sleep(played.members_delay.get()).await;
            return reply(&played.members(), cluster, stream);
        }
        if command == Command::Join {
            return match JoinView::from_frame(&view) {
                Ok(join) => reply(&played.join(&join), cluster, stream),
                Err(rule) => Answer::Refuse(ErrorReply::protocol(&rule)),
            };
        }
        if command == Command::Catalog {
            counted(&played.counts.catalog);
            compio::time::sleep(played.catalog_delay.get()).await;
            return self.catalog(&view, stream);
        }
        Answer::Refuse(ErrorReply::new(
            ErrorCode::Unavailable,
            format!("command {} is not played", command.number()),
        ))
    }

    async fn next_push(&mut self) -> (u64, ultragraphdb_common_role::Push) {
        match self.queued.pop_front() {
            Some((stream, frame)) => (stream, ultragraphdb_common_role::Push::Frame(frame)),
            None => std::future::pending().await,
        }
    }

    fn welcomed(&mut self, max_frame: u32) {
        self.max_frame = max_frame as usize; // lossless: a `usize` is at least 32 bits
    }
}
