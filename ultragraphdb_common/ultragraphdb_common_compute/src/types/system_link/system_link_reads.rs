/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What the view reads with Placement and Catalog, and the roles read with Members (design D6, D9).

use std::future::poll_fn;
use std::pin::pin;
use std::task::Poll;
use std::time::Duration;

use compio::runtime::spawn;
use futures::future::{Either, select};
use ultragraphdb_cluster_placement::{NodeId, PlacementMap};
use ultragraphdb_common_service::Readiness;
use ultragraphdb_core_ids::Epoch;
use ultragraphdb_protocol_wire::{
    BodyView, Catalog, ErrorCode, ErrorReply, Members, MembersView, Placement, PlacementView,
};

use super::system_link_asks::last;
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

    /// Reads `tenant`'s current version once, as a tenant first used is (`PROTOCOL.md` §11.13, *Compute's reads*).
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
            Ok((read, effective, bytes)) => {
                self.state
                    .borrow_mut()
                    .view
                    .catalog(tenant, read, effective, &bytes)?;
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
        let links = self.links();
        self.ask(&Members::new(), |cluster, frames| {
            links.learned(&MembersView::from_frame(&last(cluster, frames)?)?);
            Ok(())
        })
        .await
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
        let (map, tenants) = self
            .ask(&Placement::request(0), |cluster, frames| {
                let placed = PlacementView::from_frame(&last(cluster, frames)?)?;
                let tenants: Vec<u64> = (0..placed.run_count())
                    .filter_map(|at| placed.run(at))
                    .map(|run| run.tenant())
                    .collect();
                Ok((map_of(&placed), tenants))
            })
            .await?;
        let map = map?;
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
    /// read: a tenant is first used by the core when its placement names it, so no Begin asks for it (`PROTOCOL.md` §11.13,
    /// *Compute's reads*; client-endpoint, "A Begin asks no other process").
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

/// Reads Members at the start until it lists every partition of the placement and one of them answers, or the cluster's
/// deadline passes, then each time the links find a role connection closed (design D9); `readiness` is set once every
/// partition is listed, the view answers and a partition answers (design D10). Once set, readiness is withdrawn when the
/// connection to the system group closes, and this task sets it again once all of that holds again (role-binaries,
/// "Readiness, health and shutdown"). Spawned detached for the core's lifetime: a future dropped at the runtime's end reads
/// nothing more.
pub(super) fn spawn_members(link: SystemLink, links: RoleLinks, readiness: Readiness) {
    spawn(async move {
        let ready = until_ready(&link, &links, &readiness, Some(links.deadline())).await;
        if !ready {
            tracing::warn!("Members did not list every partition of the placement, or no partition answered, by the cluster's deadline");
        }
        // An event loop, for the core's lifetime: Members read again each time the links find a role connection closed,
        // and readiness, once withdrawn, set again (Ultra rule 2).
        loop {
            let found = pin!(links.closed_found());
            let withdrawn = pin!(withdrawn(&link, ready));
            match select(found, withdrawn).await {
                Either::Left(_) => {
                    if let Err(refusal) = link.members().await {
                        tracing::debug!(reason = refusal.message(), "Members could not be read again");
                    }
                }
                Either::Right(_) => {
                    // With no limit it returns only once ready.
                    let _ready = until_ready(&link, &links, &readiness, None).await;
                }
            }
        }
    })
    .detach();
}

/// Reads Members every [`MEMBERS_RETRY`] until the connection to the group is open, the group answers it, the view answers,
/// Members lists every partition of the placement and one of them answers, then sets `readiness`: whether it did, `false`
/// once `limit` has passed first. A future dropped at the runtime's end leaves the Members it asked with the link.
async fn until_ready(
    link: &SystemLink,
    links: &RoleLinks,
    readiness: &Readiness,
    limit: Option<Duration>,
) -> bool {
    let started = now();
    // Until all of it holds, or the limit (Ultra rule 2).
    loop {
        let holds = link.members().await.is_ok()
            && link.is_up()
            && link.view().answers()
            && every_partition(links)
            && a_partition_answers(links);
        if holds {
            readiness.set(true);
            return true;
        }
        if limit.is_some_and(|limit| now().saturating_duration_since(started) >= limit) {
            return false;
        }
        compio::time::sleep(MEMBERS_RETRY).await;
    }
}

/// Waits until the role's readiness, once set, has been withdrawn by the connection's close; never for a role that was not
/// ready by its deadline (`was_ready` false), which stays unready as before. A dropped future leaves its waker, which the
/// next change wakes for nothing.
async fn withdrawn(link: &SystemLink, was_ready: bool) {
    poll_fn(|context| {
        let mut state = link.state.borrow_mut();
        if was_ready && !state.readiness.is_ready() {
            return Poll::Ready(());
        }
        state.watchers.push(context.waker().clone());
        Poll::Pending
    })
    .await;
}

/// Whether a partition Members listed answers, each dialled when its link is not open: the dial is the question, as
/// compute's is (`PROTOCOL.md` §11.4, §11.12).
fn a_partition_answers(links: &RoleLinks) -> bool {
    links.partitions().into_iter().any(|partition| {
        links
            .partition(partition)
            .is_some_and(|link| link.answers())
    })
}

/// Whether Members listed every partition of the links' placement.
fn every_partition(links: &RoleLinks) -> bool {
    let partitions = links.every_partition();
    !partitions.is_empty() && partitions.iter().all(|partition| links.lists(*partition))
}
