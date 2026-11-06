/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod compute_core_lookups;

use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::rc::Rc;
use std::time::Duration;

use futures::channel::oneshot;
use ultragraphdb_cluster_placement::{NodeId, PlacementMap};
use ultragraphdb_common_role::{RoleClient, RoleCluster};
use ultragraphdb_core_ids::Epoch;
use ultragraphdb_protocol_transport::{Address, TransportError};
use ultragraphdb_protocol_wire::{
    BodyView, ErrorReply, Members, MembersView, Placement, PlacementView, RoleKind,
};

use crate::types::link::Link;
use crate::utils::bounded::bounded;
use crate::utils::refusals::unavailable;

/// The times a read keeps its versions by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadTimes {
    scan_stall: Duration,
    read_retain: Duration,
}

impl ReadTimes {
    /// `storage.scan_stall_ms` and `storage.read_retain_ms`.
    #[must_use]
    pub fn new(scan_stall: Duration, read_retain: Duration) -> Self {
        Self {
            scan_stall,
            read_retain,
        }
    }

    /// How long after a read begins it starts holding its versions, well within what a partition retains between Scans.
    pub(crate) fn pin_after(&self) -> Duration {
        self.read_retain / 4
    }

    /// How often a held version's Scan takes a frame, well within what a partition waits on a stream.
    pub(crate) fn pin_every(&self) -> Duration {
        self.scan_stall / 3
    }

    /// How long a peer may leave compute's request unanswered past any deadline the request names: twice `scan_stall_ms`, since a
    /// partition refuses a Scan with no frame able to leave once that has passed. A peer silent past it is hung.
    pub(crate) fn answer_wait(&self) -> Duration {
        self.scan_stall.saturating_mul(2)
    }
}

/// Who else waits on a partition's dial under way.
type DialWaiters = Vec<oneshot::Sender<Result<Rc<Link>, ErrorReply>>>;

/// One core's connections to the frontend and the partitions, and what it read of them: no state is shared between cores, and
/// a read runs wholly on the core that holds this (`PROTOCOL.md` §11.12).
#[derive(Debug)]
pub struct ComputeCore {
    cluster: RoleCluster,
    frontend: Address,
    times: ReadTimes,
    name: String,
    placement: RefCell<Option<Rc<PlacementMap>>>,
    addresses: RefCell<HashMap<u32, Address>>,
    links: RefCell<HashMap<u32, Rc<Link>>>,
    dialling: RefCell<HashMap<u32, DialWaiters>>,
}

/// A dial under way: dropped, whoever waits on it dials for itself.
struct DialUnderWay<'a> {
    core: &'a ComputeCore,
    partition: u32,
}

impl Drop for DialUnderWay<'_> {
    fn drop(&mut self) {
        drop(self.core.dialling.borrow_mut().remove(&self.partition));
    }
}

impl ComputeCore {
    /// The core of a process of `cluster` joined at `frontend`, dialling as the role `name`, reading by `times`.
    #[must_use]
    pub fn new(cluster: RoleCluster, frontend: Address, times: ReadTimes, name: &str) -> Rc<Self> {
        Rc::new(Self {
            cluster,
            frontend,
            times,
            name: name.to_string(),
            placement: RefCell::default(),
            addresses: RefCell::default(),
            links: RefCell::default(),
            dialling: RefCell::default(),
        })
    }

    /// The cluster's identity.
    pub(crate) fn cluster_id(&self) -> u64 {
        self.cluster.id()
    }

    /// The times reads keep their versions by.
    pub(crate) fn times(&self) -> ReadTimes {
        self.times
    }

    /// A connection to the frontend.
    async fn dial_frontend(&self) -> Result<RoleClient, TransportError> {
        RoleClient::dial(
            &self.frontend,
            self.cluster.id(),
            &self.name,
            self.cluster.secret().password(),
        )
        .await
    }

    /// The placement map, which this stage fixes once formed, so a process holds it for its lifetime (`PROTOCOL.md` §11.12).
    pub(crate) async fn placement(&self) -> Result<Rc<PlacementMap>, ErrorReply> {
        if let Some(map) = self.placement.borrow().as_ref() {
            return Ok(Rc::clone(map));
        }
        let fetched = async {
            self.fetch_placement()
                .await
                .map_err(|error| unavailable(format!("the placement map: {error}")))
        };
        let map = bounded(
            self.times.answer_wait(),
            || "the frontend's placement map".to_string(),
            fetched,
        )
        .await?;
        let map = Rc::new(map);
        *self.placement.borrow_mut() = Some(Rc::clone(&map));
        Ok(map)
    }

    async fn fetch_placement(&self) -> Result<PlacementMap, TransportError> {
        let mut client = self.dial_frontend().await?;
        let reply = client.request(&Placement::request(0)).await?;
        let view = reply.view().map_err(TransportError::protocol)?;
        let placed = PlacementView::from_frame(&view).map_err(TransportError::protocol)?;
        let partitions = (0..placed.range_count())
            .filter_map(|index| placed.range(index))
            .map(|range| range.partition())
            .max()
            .ok_or_else(|| TransportError::authentication("a placement of no range"))?;
        let map = PlacementMap::even(partitions, NodeId::new(1))
            .map_err(|error| TransportError::authentication(error.to_string()))?;
        let held = map.ranges_at(Epoch::ZERO);
        let same = held.len() == placed.range_count()
            && held.iter().enumerate().all(|(index, (range, partition))| {
                placed.range(index).is_some_and(|wire| {
                    wire.start() == range.start()
                        && wire.end() == range.end()
                        && wire.partition() == partition.get()
                })
            });
        if same {
            Ok(map)
        } else {
            Err(TransportError::authentication(
                "the frontend's placement is not the even map this stage holds",
            ))
        }
    }

    /// Where `partition` listens, asking the frontend's Members when it is not yet known.
    async fn address_of(&self, partition: u32) -> Result<Address, ErrorReply> {
        if let Some(address) = self.addresses.borrow().get(&partition) {
            return Ok(address.clone());
        }
        let fetched = async {
            self.fetch_members()
                .await
                .map_err(|error| unavailable(format!("the members: {error}")))
        };
        bounded(
            self.times.answer_wait(),
            || "the frontend's members".to_string(),
            fetched,
        )
        .await?;
        self.addresses
            .borrow()
            .get(&partition)
            .cloned()
            .ok_or_else(|| {
                unavailable(format!(
                    "partition {partition} has joined no storage process yet"
                ))
            })
    }

    async fn fetch_members(&self) -> Result<(), TransportError> {
        let mut client = self.dial_frontend().await?;
        let reply = client.request(&Members::new()).await?;
        let view = reply.view().map_err(TransportError::protocol)?;
        let members = MembersView::from_frame(&view).map_err(TransportError::protocol)?;
        let mut addresses = self.addresses.borrow_mut();
        for member in (0..members.len()).filter_map(|index| members.member(index)) {
            if member.kind() == RoleKind::Storage
                && let Some(text) = members.address(member.address() as usize)
            {
                let _old = addresses.insert(member.partition(), Address::parse(text)?);
            }
        }
        Ok(())
    }

    /// The core's one connection to `partition`, dialled when it has none or the one it had closed, concurrent reads sharing one
    /// dial.
    pub(crate) async fn link(&self, partition: u32) -> Result<Rc<Link>, ErrorReply> {
        loop {
            let held = self.links.borrow().get(&partition).cloned();
            if let Some(link) = held.filter(|link| !link.is_closed()) {
                return Ok(link);
            }
            let waiting = match self.dialling.borrow_mut().entry(partition) {
                Entry::Vacant(free) => {
                    let _none = free.insert(Vec::new());
                    None
                }
                Entry::Occupied(mut under_way) => {
                    let (told, answer) = oneshot::channel();
                    under_way.get_mut().push(told);
                    Some(answer)
                }
            };
            let Some(answer) = waiting else {
                return self.dial_for_all(partition).await;
            };
            // A dial dropped with its read's task answers no one, and the next to look dials.
            if let Ok(dialled) = answer.await {
                return dialled;
            }
        }
    }

    /// Dials `partition`, the connection or its refusal answering everyone who waited on it.
    async fn dial_for_all(&self, partition: u32) -> Result<Rc<Link>, ErrorReply> {
        let under_way = DialUnderWay {
            core: self,
            partition,
        };
        let dialled = self.dial(partition).await;
        if let Ok(link) = &dialled {
            let _closed = self.links.borrow_mut().insert(partition, Rc::clone(link));
        }
        let waiters = self.dialling.borrow_mut().remove(&partition);
        drop(under_way);
        for told in waiters.into_iter().flatten() {
            // A waiter gone is a read that ended meanwhile.
            drop(told.send(dialled.clone()));
        }
        dialled
    }

    /// A new connection to `partition`.
    async fn dial(&self, partition: u32) -> Result<Rc<Link>, ErrorReply> {
        let address = self.address_of(partition).await?;
        let dialled = async {
            RoleClient::dial(
                &address,
                self.cluster.id(),
                &self.name,
                self.cluster.secret().password(),
            )
            .await
            .map_err(|error| {
                unavailable(format!("the connection to partition {partition}: {error}"))
            })
        };
        let wait = self.times.answer_wait();
        let what = || format!("the connection to partition {partition}");
        let client = bounded(wait, what, dialled).await.inspect_err(|_| {
            // Where it listens may have moved with a restart: asked again by the next read.
            drop(self.addresses.borrow_mut().remove(&partition));
        })?;
        Ok(Link::start(partition, self.cluster.id(), wait, client))
    }
}
