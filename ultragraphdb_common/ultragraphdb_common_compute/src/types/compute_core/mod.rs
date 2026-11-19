/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod compute_core_catalog;
mod compute_core_lookups;
mod lookup_partition;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ultragraphdb_cluster_placement::{NodeId, PlacementMap};
use ultragraphdb_common_role::RoleClient;
use ultragraphdb_core_ids::Epoch;
use ultragraphdb_protocol_transport::{Address, TransportError};
use ultragraphdb_protocol_wire::{BodyView, ErrorReply, Placement, PlacementView};

use crate::types::partition_link::PartitionLink;
use crate::types::read_times::ReadTimes;
use crate::types::role_links::RoleLinks;
use crate::utils::bounded::bounded;
use crate::utils::refusals::unavailable;

/// One core's way to the partitions and what it read of the system group: its reads go on the core's links, one connection to
/// each partition's role address, which its client sessions' Bind streams share (design D2). No state is shared between
/// cores, and a read runs wholly on the core that holds this (`PROTOCOL.md` §11.12).
///
/// Shared by an `Rc` among the core's sessions and reads, so what it learns sits in a `RefCell`, borrowed for one step and
/// never across an `.await` (Ultra rule 9).
#[derive(Debug)]
pub struct ComputeCore {
    /// The core's links to the other roles, where Members put each partition's role address.
    links: RoleLinks,
    /// The system group's role address, which it reads Placement and Catalog at (`PROTOCOL.md` §11.12).
    group: Address,
    times: ReadTimes,
    placement: RefCell<Option<Rc<PlacementMap>>>,
    /// The steps the core's reads have taken, shared by an `Rc` with each of its partitions' links and each read's sink on the
    /// core's one thread, which count a step as they take it (Ultra rule 9).
    steps: Rc<Cell<u64>>,
}

impl ComputeCore {
    /// The core whose reads travel on `links`, the core's one connection to each partition, beside the Bind streams its client
    /// sessions open there (design D2): a read's Scans and Closed leave on the link to their partition's role address, which
    /// Members gave `links`, and a link closed under a read answers it code 17 and is dialled again by the next request
    /// (`PROTOCOL.md` §11.12, *Errors*). The placement map is read at the system group's role address `group`, by `times`.
    #[must_use]
    pub fn over(links: RoleLinks, group: Address, times: ReadTimes) -> Rc<Self> {
        Rc::new(Self {
            links,
            group,
            times,
            placement: RefCell::default(),
            steps: Rc::default(),
        })
    }

    /// The connections to partitions the core holds open, and how many of them hold a stream: a read that ended, by its last
    /// frame, its refusal or its task's drop, holds none.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn links_held(&self) -> (usize, usize) {
        self.links.links_held()
    }

    /// The steps this core's reads have taken: each Entries frame a read consumed and each Result frame it built but its
    /// last, after each of which the read gave the core back (design D3).
    #[must_use]
    pub fn steps(&self) -> u64 {
        self.steps.get()
    }

    /// The count of the core's steps, which each of its links and each read's sink counts its steps in.
    pub(crate) fn step_count(&self) -> Rc<Cell<u64>> {
        Rc::clone(&self.steps)
    }

    /// The cluster's identity.
    pub(crate) fn cluster_id(&self) -> u64 {
        self.links.cluster()
    }

    /// The times reads keep their versions by.
    pub(crate) fn times(&self) -> ReadTimes {
        self.times
    }

    /// A connection to the system group.
    /// A dropped future leaves no connection.
    async fn dial_group(&self) -> Result<RoleClient, TransportError> {
        let cluster = self.links.role_cluster();
        RoleClient::dial(
            &self.group,
            cluster.id(),
            self.links.name(),
            cluster.secret().password(),
        )
        .await
    }

    /// The placement map, which this stage fixes once formed, so a process holds it for its lifetime (`PROTOCOL.md` §11.12).
    /// A dropped future leaves the map unfetched, and the next caller fetches it.
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
            || "the system group's placement map".to_string(),
            fetched,
        )
        .await?;
        let map = Rc::new(map);
        *self.placement.borrow_mut() = Some(Rc::clone(&map));
        Ok(map)
    }

    /// The placement map the system group answers, checked against the even map this stage holds. A dropped future drops
    /// its connection to the group.
    async fn fetch_placement(&self) -> Result<PlacementMap, TransportError> {
        let mut client = self.dial_group().await?;
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
                "the system group's placement is not the even map this stage holds",
            ))
        }
    }

    /// The core's way to `partition`: its link to the role address Members listed for it, dialled when it has none or the one it
    /// had closed, which the core's sessions share.
    ///
    /// # Errors
    ///
    /// Code 17 for a partition no storage process has joined for, or listed at an address that does not parse.
    pub(crate) fn link(&self, partition: u32) -> Result<Rc<PartitionLink>, ErrorReply> {
        let link = self.links.partition(partition).ok_or_else(|| {
            unavailable(format!(
                "partition {partition} has joined no storage process at an address compute can dial"
            ))
        })?;
        Ok(Rc::new(PartitionLink::new(
            link,
            partition,
            (self.times.answer_wait(), self.step_count()),
        )))
    }
}
