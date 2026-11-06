/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_placement::{BucketRange, NodeId, PartitionId, PlacementMap};
use ultragraphdb_core_ids::{Epoch, Tenant, TypePrefix};
use ultragraphdb_error_cluster::ClusterError;
use ultragraphdb_protocol_scram::ScramVerifier;

use crate::types::catalog_check::CatalogCheck;
use crate::types::catalog_record::CatalogRecord;
use crate::types::metadata_command::MetadataCommand;
use crate::types::metadata_state::MetadataState;
use crate::types::principal::Principal;

impl MetadataState {
    /// Applies `command`, the state's step function: it is checked whole against the state before anything changes, so
    /// a refused command changes nothing.
    ///
    /// # Errors
    ///
    /// - [`MetadataCommand::OfferCatalog`]: [`ClusterError::stale_catalog`] naming the predecessor and the latest
    ///   version when they differ; [`ClusterError::full`] when [`MetadataState::PENDING_MAX`] versions are pending or
    ///   the tenant retains [`MetadataState::RETAINED_MAX`]; [`ClusterError::too_many`] for bytes past
    ///   [`MetadataCommand::CATALOG_BYTES_MAX`].
    /// - [`MetadataCommand::RecordDefault`]: [`ClusterError::too_many`] for a predicate or an entry past its limit;
    ///   [`ClusterError::full`] at [`MetadataState::DEFAULTS_MAX`] records. A record that exists is kept and the command
    ///   changes nothing.
    /// - [`MetadataCommand::LeaseBlock`]: [`ClusterError::contradicts`] when the block does not start at the type's
    ///   high-water mark or is empty.
    /// - [`MetadataCommand::InitPlacement`]: [`ClusterError::full`] when the map is initialised already; the refusals
    ///   of `PlacementMap::even`.
    /// - [`MetadataCommand::ChangePlacement`] and [`MetadataCommand::SetLeader`]: [`ClusterError::no_placement`] when
    ///   no map is initialised; [`ClusterError::too_many`] past [`MetadataCommand::ASSIGNMENTS_MAX`] ranges;
    ///   [`ClusterError::full`] at [`MetadataState::PLACEMENT_PENDING_MAX`] pending placement versions or when the map
    ///   has no room for one more layout; the refusals of `PlacementMap::check_layout` and
    ///   `PlacementMap::set_preferred_leader`.
    /// - [`MetadataCommand::CreatePrincipal`]: [`ClusterError::principal_name`] for a name of 0 or more than 255 bytes;
    ///   [`ClusterError::too_many`] for a salt past [`MetadataCommand::SALT_BYTES_MAX`];
    ///   [`ClusterError::principal_exists`]; [`ClusterError::full`] at [`MetadataState::PRINCIPALS_MAX`];
    ///   [`ClusterError::contradicts`] when the identity is not the next.
    /// - [`MetadataCommand::RemovePrincipal`]: [`ClusterError::unknown_principal`]; [`ClusterError::last_administrator`]
    ///   for the only principal holding the administrator role; [`ClusterError::full`] when its sessions' evictions
    ///   would pass [`MetadataState::EVICTIONS_PENDING_MAX`]; [`ClusterError::contradicts`] when the evictions named are
    ///   not the principal's sessions in registration order.
    /// - [`MetadataCommand::RegisterSession`]: [`ClusterError::session_exists`]; [`ClusterError::unknown_principal`]
    ///   naming the identity after `#`; [`ClusterError::too_many`] for more than
    ///   [`MetadataCommand::EVICTIONS_MAX`] evictions; [`ClusterError::full`] when the evictions would pass
    ///   [`MetadataState::EVICTIONS_PENDING_MAX`]; [`ClusterError::contradicts`] when the evictions named are not
    ///   the oldest sessions.
    /// - [`MetadataCommand::LearnClose`]: [`ClusterError::epoch_ahead`] for a close other than the one after the last
    ///   learned (the first learned may close any epoch from 1); [`ClusterError::epoch_limit`] for the close of the last
    ///   epoch; [`ClusterError::too_many`] past [`MetadataCommand::VERSIONS_MAX`] versions or
    ///   [`MetadataCommand::EVICTIONS_MAX`] evictions; [`ClusterError::not_pending`] for a version announced that is
    ///   not pending; [`ClusterError::contradicts`] for a version announced while an earlier version of its tenant
    ///   stays pending, or a placement version announced other than the oldest pending. An eviction carried that is not
    ///   pending changes nothing.
    /// - [`MetadataCommand::ReportUntrimmed`]: none; an epoch below the partition's last report changes nothing.
    /// - [`MetadataCommand::OfferChecked`]: as [`MetadataCommand::OfferCatalog`].
    /// - [`MetadataCommand::OfferRestoring`]: as [`MetadataCommand::OfferCatalog`]; [`ClusterError::contradicts`] when the
    ///   version it withdraws is not being checked.
    /// - [`MetadataCommand::ReportCheck`]: none; a report of a version not being checked changes nothing.
    /// - [`MetadataCommand::CreateTenant`]: [`ClusterError::contradicts`] for a tenant the group holds;
    ///   [`ClusterError::full`] at [`MetadataState::TENANTS_MAX`].
    /// - [`MetadataCommand::DrawSecret`]: [`ClusterError::contradicts`] once a secret was drawn.
    /// - [`MetadataCommand::Chunk`] and [`MetadataCommand::Commit`]: [`ClusterError::contradicts`]; they frame a large
    ///   command in the log, and the group applies the command they hold.
    pub fn apply(&mut self, command: &MetadataCommand) -> Result<(), ClusterError> {
        match command {
            MetadataCommand::OfferCatalog {
                tenant,
                predecessor,
                bytes,
            } => self.offer(*tenant, *predecessor, bytes, CatalogCheck::Settled),
            MetadataCommand::OfferChecked {
                tenant,
                predecessor,
                bytes,
            } => self.offer(*tenant, *predecessor, bytes, CatalogCheck::begun()),
            MetadataCommand::OfferRestoring {
                tenant,
                predecessor,
                restores,
                bytes,
            } => self.offer_restoring(*tenant, *predecessor, *restores, bytes),
            MetadataCommand::ReportCheck {
                tenant,
                version,
                partition,
                refused,
            } => {
                self.report_check(*tenant, *version, *partition, *refused);
                Ok(())
            }
            MetadataCommand::RecordDefault {
                tenant,
                predicate,
                entry,
            } => self.record_default(*tenant, predicate, entry),
            MetadataCommand::LeaseBlock {
                prefix, start, end, ..
            } => self.lease(*prefix, *start, *end),
            MetadataCommand::InitPlacement { partitions, leader } => {
                self.init_placement(*partitions, *leader)
            }
            MetadataCommand::ChangePlacement { assignments } => self.change_placement(assignments),
            MetadataCommand::SetLeader { partition, node } => self.set_leader(*partition, *node),
            MetadataCommand::CreatePrincipal {
                id,
                name,
                verifier,
                administrator,
            } => self.create_principal(*id, name, verifier, *administrator),
            MetadataCommand::RemovePrincipal { name, evicts } => {
                self.remove_principal(name, evicts)
            }
            MetadataCommand::RegisterSession {
                session,
                principal,
                evicts,
            } => self.register_session(*session, *principal, evicts),
            MetadataCommand::LearnClose {
                epoch,
                versions,
                placement,
                evictions,
            } => self.learn_close(*epoch, versions, *placement, evictions),
            MetadataCommand::ReportUntrimmed { partition, epoch } => {
                self.report_untrimmed(*partition, *epoch);
                Ok(())
            }
            MetadataCommand::CreateTenant { tenant } => self.create_tenant(*tenant),
            MetadataCommand::DrawSecret { secret } => self.draw_secret(secret),
            // The log's framing of a large command, which the group reassembles before it applies the command.
            MetadataCommand::Chunk { .. } | MetadataCommand::Commit { .. } => Err(
                ClusterError::contradicts("a whole command, not a chunk of one or its commit"),
            ),
        }
    }

    /// Holds `tenant` from here on.
    fn create_tenant(&mut self, tenant: Tenant) -> Result<(), ClusterError> {
        holds(!self.tenants.contains(&tenant), "a tenant not yet created")?;
        if self.tenants.len() >= Self::TENANTS_MAX {
            return Err(ClusterError::full("tenants", Self::TENANTS_MAX));
        }
        let _new: bool = self.tenants.insert(tenant);
        Ok(())
    }

    /// Holds `secret` from here on.
    fn draw_secret(&mut self, secret: &[u8; 32]) -> Result<(), ClusterError> {
        holds(self.unknown_secret.is_none(), "a secret not yet drawn")?;
        self.unknown_secret = Some(*secret);
        Ok(())
    }

    /// Commits `bytes` as the pending version of `tenant` after `predecessor`.
    fn offer(
        &mut self,
        tenant: Tenant,
        predecessor: u64,
        bytes: &[u8],
        check: CatalogCheck,
    ) -> Result<(), ClusterError> {
        within(
            "bytes of a catalog version",
            bytes.len(),
            MetadataCommand::CATALOG_BYTES_MAX,
        )?;
        let current = self.current_version(tenant);
        if predecessor != current {
            return Err(ClusterError::stale_catalog(
                tenant.get(),
                predecessor,
                current,
            ));
        }
        let pending = self.pending_count();
        if pending >= Self::PENDING_MAX {
            return Err(ClusterError::full(
                "pending catalog versions",
                Self::PENDING_MAX,
            ));
        }
        let catalog = self.catalogs.entry(tenant).or_default();
        if catalog.versions.len() >= Self::RETAINED_MAX {
            return Err(ClusterError::full(
                "retained catalog versions of a tenant",
                Self::RETAINED_MAX,
            ));
        }
        // The latest version is below u64::MAX: 2^64 − 1 versions are never committed in one tenant.
        let version = current.saturating_add(1);
        catalog.current = version;
        if check != CatalogCheck::Settled {
            let _new: bool = self.checking.insert((tenant, version));
        }
        catalog
            .versions
            .push_back(CatalogRecord::new(tenant, version, None, bytes.to_vec()).with_check(check));
        Ok(())
    }

    /// Commits `bytes` as the pending version of `tenant` after `predecessor`, withdrawing version `restores`, which is
    /// being checked.
    fn offer_restoring(
        &mut self,
        tenant: Tenant,
        predecessor: u64,
        restores: u64,
        bytes: &[u8],
    ) -> Result<(), ClusterError> {
        holds(
            self.checking.contains(&(tenant, restores)),
            "a restoring version withdraws one being checked",
        )?;
        self.offer(tenant, predecessor, bytes, CatalogCheck::Settled)?;
        let _was: bool = self.checking.remove(&(tenant, restores));
        if let Some(record) = self.record_mut(tenant, restores) {
            record.set_check(CatalogCheck::Withdrawn);
        }
        Ok(())
    }

    /// Records `partition`'s report on version `version` of `tenant`, which changes nothing once the version is no longer
    /// being checked or before its epoch is assigned: the version stands once every partition holding the tenant at its
    /// epoch has passed it and none refused it.
    fn report_check(
        &mut self,
        tenant: Tenant,
        version: u64,
        partition: PartitionId,
        refused: bool,
    ) {
        let effective = self
            .version_record(tenant, version)
            .and_then(CatalogRecord::effective);
        let Some(effective) = effective else {
            return;
        };
        let holders = self.holders(tenant, effective);
        let Some(record) = self.record_mut(tenant, version) else {
            return;
        };
        let CatalogCheck::Checking {
            mut passed,
            refused: was_refused,
        } = record.check().clone()
        else {
            return;
        };
        let refused = was_refused || refused;
        if !refused {
            let _new: bool = passed.insert(partition);
        }
        let settled = !refused && holders.iter().all(|holder| passed.contains(holder));
        if settled {
            record.set_check(CatalogCheck::Settled);
            let _was: bool = self.checking.remove(&(tenant, version));
        } else {
            record.set_check(CatalogCheck::Checking { passed, refused });
        }
    }

    /// Version `version` of `tenant`, to change.
    fn record_mut(&mut self, tenant: Tenant, version: u64) -> Option<&mut CatalogRecord> {
        self.catalogs
            .get_mut(&tenant)?
            .versions
            .iter_mut()
            .find(|record| record.version() == version)
    }

    /// The versions pending across every tenant.
    fn pending_count(&self) -> usize {
        self.catalogs
            .values()
            .map(|catalog| {
                catalog
                    .versions
                    .iter()
                    .filter(|v| v.effective().is_none())
                    .count()
            })
            .sum()
    }

    /// Records the default entry of `predicate` in `tenant`, keeping one that exists.
    fn record_default(
        &mut self,
        tenant: Tenant,
        predicate: &str,
        entry: &[u8],
    ) -> Result<(), ClusterError> {
        within(
            "bytes of a predicate's name",
            predicate.len(),
            MetadataCommand::PREDICATE_BYTES_MAX,
        )?;
        within(
            "bytes of a default entry",
            entry.len(),
            MetadataCommand::ENTRY_BYTES_MAX,
        )?;
        let key = (tenant, predicate.to_string());
        if self.defaults.contains_key(&key) {
            return Ok(());
        }
        if self.defaults.len() >= Self::DEFAULTS_MAX {
            return Err(ClusterError::full(
                "default-entry records",
                Self::DEFAULTS_MAX,
            ));
        }
        let _absent: Option<Vec<u8>> = self.defaults.insert(key, entry.to_vec());
        Ok(())
    }

    /// Leases `start..end` of `prefix`'s sequences, starting at its high-water mark.
    fn lease(&mut self, prefix: TypePrefix, start: u64, end: u64) -> Result<(), ClusterError> {
        holds(
            start == self.next_sequence(prefix),
            "a block starts at its type's high-water mark",
        )?;
        holds(start < end, "a block holds a sequence")?;
        let _previous: Option<u64> = self.high_water.insert(prefix.get(), end);
        Ok(())
    }

    /// Initialises the placement map evenly over `partitions`.
    fn init_placement(&mut self, partitions: u32, leader: NodeId) -> Result<(), ClusterError> {
        if self.placement.is_some() {
            return Err(ClusterError::full("placement maps", 1));
        }
        self.placement = Some(PlacementMap::even(partitions, leader)?);
        Ok(())
    }

    /// Commits the layout `assignments` as the next pending placement version.
    fn change_placement(
        &mut self,
        assignments: &[(BucketRange, PartitionId)],
    ) -> Result<(), ClusterError> {
        let map = self
            .placement
            .as_ref()
            .ok_or_else(ClusterError::no_placement)?;
        within(
            "ranges of a placement change",
            assignments.len(),
            MetadataCommand::ASSIGNMENTS_MAX,
        )?;
        if self.placement_pending.len() >= Self::PLACEMENT_PENDING_MAX {
            return Err(ClusterError::full(
                "pending placement versions",
                Self::PLACEMENT_PENDING_MAX,
            ));
        }
        // Every pending version becomes a layout once announced, so the map keeps room for all of them.
        if self.placement_pending.len() >= map.layouts_free() {
            return Err(ClusterError::full(
                "placement layouts",
                PlacementMap::LAYOUTS_MAX as usize,
            ));
        }
        PlacementMap::check_layout(assignments)?;
        // At most 255 layouts follow the first, so the number never nears u64::MAX.
        self.placement_version = self.placement_version.saturating_add(1);
        self.placement_pending
            .push_back((self.placement_version, assignments.to_vec()));
        Ok(())
    }

    /// Makes `node` the preferred leader of `partition`.
    fn set_leader(&mut self, partition: PartitionId, node: NodeId) -> Result<(), ClusterError> {
        let map = self
            .placement
            .as_mut()
            .ok_or_else(ClusterError::no_placement)?;
        map.set_preferred_leader(partition, node)
    }

    /// Creates the principal `id` named `name`.
    fn create_principal(
        &mut self,
        id: u64,
        name: &str,
        verifier: &ScramVerifier,
        administrator: bool,
    ) -> Result<(), ClusterError> {
        if name.is_empty() || name.len() > MetadataCommand::NAME_BYTES_MAX {
            return Err(ClusterError::principal_name(name.len()));
        }
        within(
            "bytes of a verifier's salt",
            verifier.salt().len(),
            MetadataCommand::SALT_BYTES_MAX,
        )?;
        if self.principals.contains_key(name) {
            return Err(ClusterError::principal_exists(name));
        }
        if self.principals.len() >= Self::PRINCIPALS_MAX {
            return Err(ClusterError::full("principals", Self::PRINCIPALS_MAX));
        }
        holds(
            id == self.next_principal,
            "a principal takes the next identity",
        )?;
        self.next_principal = id.saturating_add(1);
        let principal = Principal::new(id, name.to_string(), verifier.clone(), administrator);
        let _absent: Option<Principal> = self.principals.insert(name.to_string(), principal);
        Ok(())
    }

    /// Removes the principal `name`, unless it is the last administrator, evicting `evicts`, its sessions.
    fn remove_principal(&mut self, name: &str, evicts: &[u128]) -> Result<(), ClusterError> {
        let principal = self
            .principals
            .get(name)
            .ok_or_else(|| ClusterError::unknown_principal(name))?;
        let administrators = self
            .principals
            .values()
            .filter(|p| p.administrator())
            .count();
        if principal.administrator() && administrators == 1 {
            return Err(ClusterError::last_administrator(name));
        }
        if self.evictions.len() + evicts.len() > Self::EVICTIONS_PENDING_MAX {
            return Err(ClusterError::full(
                "session evictions awaiting a close",
                Self::EVICTIONS_PENDING_MAX,
            ));
        }
        let id = principal.id();
        let held = self
            .sessions
            .iter()
            .filter(|(_, owner)| *owner == id)
            .map(|(session, _)| session);
        holds(
            held.eq(evicts.iter()),
            "a removal evicts the principal's sessions",
        )?;
        self.sessions.retain(|(_, owner)| *owner != id);
        for session in evicts {
            let _owner: Option<u64> = self.owners.remove(session);
        }
        self.evictions.extend(evicts.iter().copied());
        let _removed: Option<Principal> = self.principals.remove(name);
        Ok(())
    }

    /// Registers `session` for `principal`, evicting `evicts`: held sessions, oldest first, the oldest with no commit in
    /// flight when the command was decided, which only the frontend knew.
    fn register_session(
        &mut self,
        session: u128,
        principal: u64,
        evicts: &[u128],
    ) -> Result<(), ClusterError> {
        if self.owners.contains_key(&session) {
            return Err(ClusterError::session_exists(session));
        }
        if self.principal_by_id(principal).is_none() {
            return Err(ClusterError::unknown_principal(&format!("#{principal}")));
        }
        within(
            "session evictions",
            evicts.len(),
            MetadataCommand::EVICTIONS_MAX,
        )?;
        if self.evictions.len() + evicts.len() > Self::EVICTIONS_PENDING_MAX {
            return Err(ClusterError::full(
                "session evictions awaiting a close",
                Self::EVICTIONS_PENDING_MAX,
            ));
        }
        // Each found after the one before: a session skipped for its commit in flight lies between, so the scan from the
        // front reaches past at most those.
        let mut at = Vec::with_capacity(evicts.len());
        let mut from = 0;
        for evicted in evicts {
            let found = self
                .sessions
                .iter()
                .skip(from)
                .position(|(held, _)| held == evicted)
                .map(|offset| from + offset);
            holds(
                found.is_some(),
                "a registration evicts held sessions, oldest first",
            )?;
            if let Some(found) = found {
                at.push(found);
                from = found + 1;
            }
        }
        for found in at.into_iter().rev() {
            if let Some((evicted, _)) = self.sessions.remove(found) {
                let _owner: Option<u64> = self.owners.remove(&evicted);
            }
        }
        self.evictions.extend(evicts.iter().copied());
        self.sessions.push_back((session, principal));
        let _absent: Option<u64> = self.owners.insert(session, principal);
        Ok(())
    }

    /// Learns the close of `epoch`, assigning `versions` and `placement` the epoch after it and carrying `evictions`
    /// away.
    fn learn_close(
        &mut self,
        epoch: Epoch,
        versions: &[(Tenant, u64)],
        placement: u64,
        evictions: &[u128],
    ) -> Result<(), ClusterError> {
        within(
            "catalog versions in a close",
            versions.len(),
            MetadataCommand::VERSIONS_MAX,
        )?;
        within(
            "session evictions in a close",
            evictions.len(),
            MetadataCommand::EVICTIONS_MAX,
        )?;
        // The last learned is below 2^42 − 1, which no close follows, so one more cannot overflow.
        let next = self.learned.get() + 1;
        let follows = if self.learned == Epoch::ZERO {
            epoch.get() >= 1
        } else {
            epoch.get() == next
        };
        if !follows {
            return Err(ClusterError::epoch_ahead(epoch.get(), next));
        }
        let effective = epoch
            .next()
            .map_err(|_| ClusterError::epoch_limit(epoch.get()))?;
        for (tenant, version) in versions {
            let pending = self.catalogs.get(tenant).is_some_and(|catalog| {
                catalog
                    .versions
                    .iter()
                    .any(|v| v.version() == *version && v.effective().is_none())
            });
            if !pending {
                return Err(ClusterError::not_pending(tenant.get(), *version));
            }
        }
        // A version takes effect no earlier than the one it is built on: every version below one announced is in
        // effect already or announced with it, so the effective epochs rise with the versions.
        let in_order = versions.iter().all(|(tenant, version)| {
            self.catalogs.get(tenant).is_none_or(|catalog| {
                catalog
                    .versions
                    .iter()
                    .filter(|v| v.version() < *version && v.effective().is_none())
                    .all(|v| versions.contains(&(*tenant, v.version())))
            })
        });
        holds(in_order, "a close announces a tenant's versions in order")?;
        // The last check, and the first change: nothing has changed when it refuses.
        self.announce_placement(placement, effective)?;
        for (tenant, version) in versions {
            let record = self.catalogs.get_mut(tenant).and_then(|catalog| {
                catalog
                    .versions
                    .iter_mut()
                    .find(|v| v.version() == *version)
            });
            if let Some(record) = record {
                record.take_effect(effective);
            }
        }
        self.evictions
            .retain(|session| !evictions.contains(session));
        self.learned = epoch;
        Ok(())
    }

    /// Takes the layout of placement version `placement`, which a close announced, into the map from `effective`; 0
    /// announces none. Refused, with the map unchanged, unless it is the oldest pending.
    fn announce_placement(&mut self, placement: u64, effective: Epoch) -> Result<(), ClusterError> {
        // A close announces at most one placement version, the oldest pending, so their epochs rise with their numbers
        // and no layout is ever placed at an epoch a partition may have opened.
        holds(
            placement == 0 || placement == self.pending_placement(),
            "a close announces the oldest pending placement version",
        )?;
        if placement == 0 {
            return Ok(());
        }
        let layout = self
            .placement_pending
            .front()
            .map(|(_, layout)| layout.as_slice())
            .unwrap_or_default();
        // The epoch after this close is after every layout's, each announced by an earlier close.
        self.placement
            .as_mut()
            .ok_or_else(ClusterError::no_placement)?
            .change(effective, layout)?;
        let _announced = self.placement_pending.pop_front();
        Ok(())
    }

    /// Records that `partition`'s untrimmed log begins at `epoch`, and releases the versions no such log names.
    fn report_untrimmed(&mut self, partition: PartitionId, epoch: Epoch) {
        let latest = self.untrimmed.entry(partition).or_insert(epoch);
        *latest = (*latest).max(epoch);
        let Some(floor) = self.untrimmed_floor() else {
            return;
        };
        for catalog in self.catalogs.values_mut() {
            let in_effect = catalog
                .versions
                .iter()
                .rposition(|v| v.effective().is_some_and(|e| e <= floor));
            // A version being checked may yet be restored from the one before it.
            let restorable = catalog
                .versions
                .iter()
                .position(|v| matches!(v.check(), CatalogCheck::Checking { .. }))
                .map_or(usize::MAX, |at| at.saturating_sub(1));
            for _ in 0..in_effect.unwrap_or(0).min(restorable) {
                if let Some(released) = catalog.versions.pop_front() {
                    catalog.released = released.version();
                }
            }
        }
    }

    /// The oldest epoch any partition of the placement map's latest layout may name in its untrimmed log, `None` while
    /// a partition of it has not reported, or before the map is initialised.
    fn untrimmed_floor(&self) -> Option<Epoch> {
        let map = self.placement.as_ref()?;
        let mut partitions: Vec<PartitionId> = map
            .ranges_at(map.latest_change())
            .into_iter()
            .map(|(_, partition)| partition)
            .collect();
        partitions.sort_unstable();
        partitions.dedup();
        partitions
            .iter()
            .map(|partition| self.untrimmed.get(partition).copied())
            .try_fold(Epoch::MAX, |floor, reported| reported.map(|r| floor.min(r)))
    }
}

/// Refuses the command as contradicting the state unless `what` holds: a decision recorded in a command the group did
/// not decide against this state, which replay meets only in a log it did not write.
fn holds(holds: bool, what: &'static str) -> Result<(), ClusterError> {
    if holds {
        Ok(())
    } else {
        Err(ClusterError::contradicts(what))
    }
}

/// `count`, refused past `max` as `what` too many.
fn within(what: &'static str, count: usize, max: usize) -> Result<(), ClusterError> {
    if count > max {
        return Err(ClusterError::too_many(what, count, max));
    }
    Ok(())
}
