/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What the view reads with Placement and Catalog, and the roles read with Members (design D6, D9).

use std::time::Duration;

use compio::runtime::spawn;
use ultragraphdb_cluster_placement::{NodeId, PlacementMap};
use ultragraphdb_common_service::Readiness;
use ultragraphdb_core_ids::Epoch;
use ultragraphdb_protocol_wire::{
    BodyView, Catalog, ErrorCode, ErrorReply, Members, MembersView, Placement, PlacementView,
};

use super::{SystemLink, unavailable};
use crate::types::client_session::now;
use crate::types::role_links::RoleLinks;

/// How long a Members read that does not yet list every partition waits before the next.
const MEMBERS_RETRY: Duration = Duration::from_millis(100);

impl SystemLink {
    /// `tenant`'s schema id, of the current version the view holds, read with Catalog when the view has not read it.
    ///
    /// # Errors
    ///
    /// The group's refusal of the Catalog; code 17 when the link is closed.
    pub async fn schema_id(&self, tenant: u64) -> Result<u64, ErrorReply> {
        if let Some(id) = self.view().schema_id(tenant) {
            return Ok(id);
        }
        let (unread, current) = {
            let view = self.view();
            (view.unread(tenant), view.current_version(tenant))
        };
        self.read_version(tenant, if unread { 0 } else { current })
            .await?;
        self.view()
            .schema_id(tenant)
            .ok_or_else(|| unavailable("the catalog version's schema id is not held"))
    }

    /// Reads `tenant`'s current version once, as a tenant first used is (`PROTOCOL.md` §11.13, *The frontend's reads*).
    ///
    /// # Errors
    ///
    /// The group's refusal; code 17 when the link is closed or closes before the answer.
    pub async fn known(&self, tenant: u64) -> Result<(), ErrorReply> {
        if !self.view().unread(tenant) {
            return Ok(());
        }
        self.read_version(tenant, 0).await
    }

    /// Reads `version` of `tenant`, 0 for its current one, into the view: code 7 with `detail` 0 to a request of version 0
    /// says the tenant holds none.
    async fn read_version(&self, tenant: u64, version: u64) -> Result<(), ErrorReply> {
        match self.catalog_read(&Catalog::request(tenant, version)).await {
            Ok((read, bytes)) => {
                self.state.borrow_mut().view.catalog(tenant, read, &bytes)?;
                self.notify();
                Ok(())
            }
            Err(refusal)
                if version == 0
                    && refusal.code() == ErrorCode::Refused
                    && refusal.detail() == 0 =>
            {
                self.state.borrow_mut().view.catalog_none(tenant);
                self.notify();
                Ok(())
            }
            Err(refusal) => Err(refusal),
        }
    }

    /// Reads Members into the core's links (design D9).
    ///
    /// # Errors
    ///
    /// The group's refusal; code 17 when the link is closed or closes before the answer.
    pub async fn members(&self) -> Result<(), ErrorReply> {
        let frames = self.ask(&Members::new()).await?;
        let frame = frames
            .last()
            .ok_or_else(|| unavailable("a Members answer of no frame"))?;
        let view = self.read(frame)?;
        let members = MembersView::from_frame(&view).map_err(|rule| ErrorReply::protocol(&rule))?;
        self.links().learned(&members);
        Ok(())
    }

    /// The links Members is read into.
    fn links(&self) -> RoleLinks {
        self.state.borrow().links.clone()
    }

    /// Reads the placement in effect into the view and the links: the even map this stage holds over its ranges' partitions,
    /// checked against them, and the tenants its runs list. A refusal naming another owner asks it again.
    ///
    /// # Errors
    ///
    /// The group's refusal; code 17 when the link is closed or closes before the answer.
    pub async fn read_placement(&self) -> Result<(), ErrorReply> {
        let frames = self.ask(&Placement::request(0)).await?;
        let frame = frames
            .last()
            .ok_or_else(|| unavailable("a Placement answer of no frame"))?;
        let view = self.read(frame)?;
        let placed =
            PlacementView::from_frame(&view).map_err(|rule| ErrorReply::protocol(&rule))?;
        let map = map_of(&placed)?;
        let tenants = (0..placed.run_count())
            .filter_map(|at| placed.run(at))
            .map(|run| run.tenant())
            .collect();
        self.links().set_map(map.clone());
        {
            let mut state = self.state.borrow_mut();
            let version = state.placement_asked;
            state.view.placement(map, version, tenants);
        }
        self.notify();
        Ok(())
    }

    /// Reads what the view wants before it answers, once, while it wants something: Placement, then each tenant's current
    /// version. Started after each close; one runs at a time.
    pub(super) fn refresh(&self) {
        {
            let mut state = self.state.borrow_mut();
            let wants = state.view.wants_placement() || !state.view.wants_catalogs().is_empty();
            if state.refreshing || !wants {
                return;
            }
            state.refreshing = true;
        }
        let link = self.clone();
        spawn(async move {
            let read = link.read_wanted().await;
            if let Err(refusal) = read {
                tracing::debug!(
                    reason = refusal.message(),
                    "the view's reads were cut short"
                );
            }
            link.state.borrow_mut().refreshing = false;
        })
        .detach();
    }

    /// Placement, then the current version of each tenant to read again and of each tenant the placement lists that was never
    /// read: a tenant is first used by the core when its placement names it, so no Begin asks for it (`PROTOCOL.md` §11.13, *The
    /// frontend's reads*; client-endpoint, "A Begin asks no other process").
    async fn read_wanted(&self) -> Result<(), ErrorReply> {
        if self.view().wants_placement() {
            self.read_placement().await?;
        }
        let tenants = self.view().wants_catalogs();
        for tenant in tenants {
            self.read_version(tenant, 0).await?;
        }
        let unread: Vec<u64> = {
            let view = self.view();
            view.tenants()
                .filter(|tenant| view.unread(*tenant))
                .collect()
        };
        for tenant in unread {
            self.read_version(tenant, 0).await?;
        }
        Ok(())
    }
}

/// The map `placed` names: this stage's even map over the highest partition its ranges name, refused when its ranges are
/// not that map's.
fn map_of(placed: &PlacementView<'_>) -> Result<PlacementMap, ErrorReply> {
    let partitions = (0..placed.range_count())
        .filter_map(|at| placed.range(at))
        .map(|range| range.partition())
        .max()
        .ok_or_else(|| unavailable("a placement of no range"))?;
    let map = PlacementMap::even(partitions, NodeId::new(1))
        .map_err(|error| unavailable(&error.to_string()))?;
    let held = map.ranges_at(Epoch::ZERO);
    let same = held.len() == placed.range_count()
        && held.iter().enumerate().all(|(at, (range, partition))| {
            placed.range(at).is_some_and(|wire| {
                wire.start() == range.start()
                    && wire.end() == range.end()
                    && wire.partition() == partition.get()
            })
        });
    if same {
        Ok(map)
    } else {
        Err(unavailable(
            "the system group's placement is not the even map this stage holds",
        ))
    }
}

/// Reads Members at the start until it lists every partition of the placement or the cluster's deadline passes, then each
/// time the links find a role connection closed (design D9); `readiness` is set once every partition is listed and the view
/// answers. Spawned detached for the core's lifetime: a future dropped at the runtime's end reads nothing more.
pub(super) fn spawn_members(link: SystemLink, links: RoleLinks, readiness: Readiness) {
    spawn(async move {
        let started = now();
        // Until every partition is listed, or the deadline (Ultra rule 2).
        loop {
            let listed = link.members().await.is_ok() && link.view().answers() && every_partition(&links);
            if listed {
                readiness.set(true);
                break;
            }
            if now().saturating_duration_since(started) >= links.deadline() {
                tracing::warn!("Members did not list every partition of the placement by the cluster's deadline");
                break;
            }
            compio::time::sleep(MEMBERS_RETRY).await;
        }
        // An event loop: it reads Members each time the links find a role connection closed, for the core's lifetime.
        loop {
            links.closed_found().await;
            if let Err(refusal) = link.members().await {
                tracing::debug!(reason = refusal.message(), "Members could not be read again");
            }
        }
    })
    .detach();
}

/// Whether Members listed every partition of the links' placement.
fn every_partition(links: &RoleLinks) -> bool {
    let partitions = links.every_partition();
    !partitions.is_empty() && partitions.iter().all(|partition| links.lists(*partition))
}
