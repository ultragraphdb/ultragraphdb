/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use ultragraphdb_cluster_placement::PlacementMap;
use ultragraphdb_core_catalog::{CatalogContents, describe_of};
use ultragraphdb_protocol_wire::{ErrorCode, ErrorReply};

/// What a Begin naming a token ahead of the view came to (`PROTOCOL.md` §4.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoldOutcome {
    /// The close before the token's epoch arrived: the Begin is answered.
    Answered,
    /// Its deadline was reached with that close not arrived: code 24.
    TimedOut,
}

/// One endpoint core's view of what the system group's closes announce (design D6; `PROTOCOL.md` §11.12, *Epochs*): the
/// latest closed epoch, the placement in effect and the tenants its runs list, each tenant's latest catalog version as the
/// closes and the core's own Declare replies name it, the schema id of each version it read with Catalog, and the Begins
/// holding a token ahead of it. Nothing is decided from it but what a Begin and the gate read: the system group stays the
/// authority. Sans-I/O: the core's link feeds it, and time is what it is told.
///
/// Control plane (Ultra rule 3), but for its reads: a close allocates once per close.
#[derive(Debug, Default, Clone)]
pub struct EndpointView {
    /// The latest close, `None` before the first.
    closed: Option<u64>,
    /// The placement version the map read holds.
    placement_version: u64,
    wants_placement: bool,
    /// Whether the next close is a subscription's first after it was opened again.
    reopened: bool,
    map: Option<PlacementMap>,
    tenants: BTreeSet<u64>,
    /// Each tenant's latest version learned.
    current: BTreeMap<u64, u64>,
    /// The tenants whose current version was read once.
    read: BTreeSet<u64>,
    /// Each tenant's schema id, by the version it was computed of.
    schema_ids: BTreeMap<u64, (u64, u64)>,
    /// The tenants to read again before the view answers.
    refresh: BTreeSet<u64>,
    /// The held Begins: each token's epoch and deadline.
    holds: BTreeMap<u64, (u64, Instant)>,
}

impl EndpointView {
    /// A view of nothing: no close, no placement, which it wants.
    #[must_use]
    pub fn new() -> Self {
        Self {
            wants_placement: true,
            ..Self::default()
        }
    }

    /// The close of `epoch`, assigning the placement version `placement` when not 0 and announcing each `(tenant, version)`:
    /// whether it was applied. A close at or below the latest held is applied once, so a subscription opened again at the
    /// close it held applies nothing twice; the first close after [`EndpointView::reopened`] asks for Placement and for the
    /// current version of each tenant held to be read again.
    pub fn close(&mut self, epoch: u64, placement: u64, announced: &[(u64, u64)]) -> bool {
        if std::mem::take(&mut self.reopened) {
            self.wants_placement = true;
            self.refresh = self
                .current
                .iter()
                .filter(|(_, version)| **version > 0)
                .map(|(tenant, _)| *tenant)
                .collect();
        }
        if self.closed.is_some_and(|held| epoch <= held) {
            return false;
        }
        self.closed = Some(epoch);
        if placement != 0 && placement != self.placement_version {
            self.wants_placement = true;
        }
        for (tenant, version) in announced {
            self.learned(*tenant, *version);
        }
        true
    }

    /// The subscription was opened again with `latest`, skipping closes and their announcements.
    pub fn reopened(&mut self) {
        self.reopened = true;
    }

    /// The placement Placement read: its map, the version it holds, and the tenants its runs list.
    pub fn placement(&mut self, map: PlacementMap, version: u64, tenants: Vec<u64>) {
        self.map = Some(map);
        self.placement_version = version;
        self.tenants = tenants.into_iter().collect();
        self.wants_placement = false;
    }

    /// Version `version` of `tenant` read with Catalog, its bytes `bytes`: its schema id computed, as `core_catalog`'s
    /// `describe_of` gives it.
    ///
    /// # Errors
    ///
    /// Code 17 for bytes that do not decode as a catalog version, as the group answered for a version no longer held.
    pub fn catalog(&mut self, tenant: u64, version: u64, bytes: &[u8]) -> Result<(), ErrorReply> {
        let contents = CatalogContents::decode(tenant, version, bytes)
            .map_err(|error| ErrorReply::new(ErrorCode::Unavailable, error.to_string()))?;
        let id = describe_of(tenant, 0, contents.declarations(), std::iter::empty()).schema_id();
        let _replaced = self.schema_ids.insert(tenant, (version, id));
        let _new = self.read.insert(tenant);
        let _done = self.refresh.remove(&tenant);
        self.learned(tenant, version);
        Ok(())
    }

    /// The core's own Declare made `version` of `tenant`.
    pub fn declared(&mut self, tenant: u64, version: u64) {
        self.learned(tenant, version);
    }

    /// Whether a Begin and the gate may be answered from it: it holds a close, and no Placement or Catalog it wants is
    /// still to be read.
    #[must_use]
    pub fn answers(&self) -> bool {
        // A subscription opened again answers nothing until its first close and the reads that close calls for.
        self.closed.is_some() && !self.reopened && !self.wants_placement && self.refresh.is_empty()
    }

    /// Whether it wants Placement read.
    #[must_use]
    pub fn wants_placement(&self) -> bool {
        self.wants_placement
    }

    /// The tenants whose current version it wants read again with Catalog before it answers: each it holds a version of,
    /// after a subscription opened again.
    #[must_use]
    pub fn wants_catalogs(&self) -> Vec<u64> {
        self.refresh.iter().copied().collect()
    }

    /// Whether `tenant`'s current version was never read: a tenant first used is read with Catalog once (`PROTOCOL.md`
    /// §11.13, *The frontend's reads*).
    #[must_use]
    pub fn unread(&self, tenant: u64) -> bool {
        !self.read.contains(&tenant)
    }

    /// `tenant` holds no catalog version, as a Catalog's code 7 with `detail` 0 says.
    pub fn catalog_none(&mut self, tenant: u64) {
        let _new = self.read.insert(tenant);
        let _done = self.refresh.remove(&tenant);
        let initial = describe_of(tenant, 0, &[], std::iter::empty()).schema_id();
        if self.current_version(tenant) == 0 {
            let _replaced = self.schema_ids.insert(tenant, (0, initial));
        }
    }

    /// The latest closed epoch it holds, 0 before the first close.
    #[must_use]
    pub fn latest_closed(&self) -> u64 {
        self.closed.unwrap_or(0)
    }

    /// The open epoch: one above the latest closed.
    #[must_use]
    pub fn open_epoch(&self) -> u64 {
        self.latest_closed().saturating_add(1)
    }

    /// Whether the placement's runs list `tenant`.
    #[must_use]
    pub fn holds_tenant(&self, tenant: u64) -> bool {
        self.tenants.contains(&tenant)
    }

    /// The tenants the placement's runs list, ascending.
    pub fn tenants(&self) -> impl Iterator<Item = u64> + '_ {
        self.tenants.iter().copied()
    }

    /// `tenant`'s latest catalog version it holds, 0 for none.
    #[must_use]
    pub fn current_version(&self, tenant: u64) -> u64 {
        self.current.get(&tenant).copied().unwrap_or(0)
    }

    /// The schema id of `tenant`'s current version, once that version was read; of version 0 once the tenant is known to hold
    /// none, the initial catalog's.
    #[must_use]
    pub fn schema_id(&self, tenant: u64) -> Option<u64> {
        let current = self.current_version(tenant);
        self.schema_ids
            .get(&tenant)
            .filter(|(version, _)| *version == current)
            .map(|(_, id)| *id)
    }

    /// The placement map in effect, once read.
    #[must_use]
    pub fn map(&self) -> Option<&PlacementMap> {
        self.map.as_ref()
    }

    /// A Begin of `ticket` naming a token of `token_epoch`, held until `deadline`: answered at once when the token's epoch is
    /// at most the open epoch, else `None`, and held.
    pub fn hold(
        &mut self,
        ticket: u64,
        token_epoch: u64,
        deadline: Instant,
    ) -> Option<HoldOutcome> {
        if token_epoch <= self.open_epoch() {
            return Some(HoldOutcome::Answered);
        }
        let _replaced = self.holds.insert(ticket, (token_epoch, deadline));
        None
    }

    /// The held Begins decided at `now`, after every close taken: those whose close has arrived answered, those whose
    /// deadline is reached without it timed out.
    pub fn released(&mut self, now: Instant) -> Vec<(u64, HoldOutcome)> {
        let open = self.open_epoch();
        let mut released = Vec::new();
        self.holds.retain(|ticket, (epoch, deadline)| {
            if *epoch <= open {
                released.push((*ticket, HoldOutcome::Answered));
                false
            } else if now >= *deadline {
                released.push((*ticket, HoldOutcome::TimedOut));
                false
            } else {
                true
            }
        });
        released
    }

    /// The earliest deadline a held Begin waits for.
    #[must_use]
    pub fn next_deadline(&self) -> Option<Instant> {
        self.holds.values().map(|(_, deadline)| *deadline).min()
    }

    /// `tenant`'s version `version` learned: the highest learned stays.
    fn learned(&mut self, tenant: u64, version: u64) {
        let held = self.current.entry(tenant).or_insert(0);
        *held = (*held).max(version);
    }
}
