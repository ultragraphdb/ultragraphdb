/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use futures::channel::oneshot::Sender;
use ultragraphdb_cluster_metadata::{CatalogCheck, CatalogRecord, MetadataCommand};
use ultragraphdb_core_catalog::{CatalogContents, describe_of};
use ultragraphdb_core_ids::{Tenant, Version};
use ultragraphdb_core_mapping::Declaration;
use ultragraphdb_error_cluster::{ClusterError, ClusterErrorEnum};
use ultragraphdb_error_role::RoleError;
use ultragraphdb_protocol_wire::{Declared, Describe, ErrorCode, ErrorReply};
use ultragraphdb_traits_catalog::DefaultEntry;
use ultragraphdb_traits_log::OrderedLog;

use crate::types::system_asked::DropStep;
use crate::types::system_group::system_group_answer::refused_or_stop;
use crate::types::system_group::system_group_reads::unknown_tenant;
use crate::types::system_group::{Durable, SystemGroup};
use crate::utils::declarations::{merged, tightens, version_bytes};

/// The kind a Catalog conflict names (`PROTOCOL.md` §9.4).
const CONFLICT_CATALOG: u64 = 5;

/// How many bits of a version its sequence holds: an epoch's first version is the epoch shifted by these (`PROTOCOL.md` §1).
const SEQUENCE_BITS: u32 = 22;

/// What a version's bytes may hold, `catalog_bytes_max`: more is refused at its Commit (`PROTOCOL.md` §11.9, §11.13).
pub(crate) const CATALOG_CAP: usize = MetadataCommand::CATALOG_BYTES_MAX;

impl<M: OrderedLog, S: OrderedLog> SystemGroup<M, S> {
    /// The declarations of `tenant`'s version `version`, none for version 0.
    pub(super) fn contents(
        &self,
        tenant: u64,
        version: u64,
    ) -> Result<Vec<Declaration>, ErrorReply> {
        if version == 0 {
            return Ok(Vec::new());
        }
        let state = self.metadata.state();
        let versions = state.versions(Tenant::new(tenant));
        let record: Option<&&CatalogRecord> = versions.iter().find(|r| r.version() == version);
        decoded(tenant, version, record.map(|record| record.bytes()))
    }

    /// `tenant`'s schema id, of its latest catalog version, as the connection's principal sees it: this stage's every principal
    /// sees it whole. Computed once per version, which alone changes it, and kept per tenant.
    pub(super) fn schema_id(&mut self, tenant: u64) -> Result<u64, ErrorReply> {
        let current = self.metadata.state().current_version(Tenant::new(tenant));
        if let Some((version, id)) = self.schema_ids.get(&tenant)
            && *version == current
        {
            return Ok(*id);
        }
        let declarations = self.contents(tenant, current)?;
        let id = describe_of(tenant, 0, &declarations, std::iter::empty()).schema_id();
        let _replaced = self.schema_ids.insert(tenant, (current, id));
        Ok(id)
    }

    /// What `tenant`'s catalog declares in effect at the latest close, no version being checked or withdrawn among it, and the
    /// default entries the group records.
    pub(super) fn describe(&self, tenant: u64) -> Result<Describe, ErrorReply> {
        let state = self.metadata.state();
        let held = Tenant::new(tenant);
        let closed = self.sequencer.last_closed();
        let declarations = match state.settled_at(held, closed) {
            Some(record) => decoded(tenant, record.version(), Some(record.bytes()))?,
            None => Vec::new(),
        };
        let recorded = state.default_entries(held).map(|(predicate, _)| predicate);
        Ok(describe_of(
            tenant,
            Version::bound_of(closed),
            &declarations,
            recorded,
        ))
    }

    /// A catalog version of `declared`, built on `built_on`, offered as pending: its answer waits for its effective epoch and, when
    /// it tightens a predicate, for every partition's check.
    /// An asker gone has no one to tell, so each reply's failed send is dropped.
    pub(super) fn declare(
        &mut self,
        tenant: u64,
        built_on: u64,
        declared: &[Declared],
        reply: Sender<Result<(u64, u64), ErrorReply>>,
    ) -> Result<(), RoleError> {
        if !self.metadata.state().holds_tenant(Tenant::new(tenant)) {
            drop(reply.send(Err(unknown_tenant(tenant))));
            return Ok(());
        }
        let current = self.metadata.state().current_version(Tenant::new(tenant));
        if built_on != current {
            drop(reply.send(Err(stale(built_on, current))));
            return Ok(());
        }
        if let Some(checked) = self.metadata.state().checked_version(Tenant::new(tenant)) {
            drop(reply.send(Err(being_checked(checked))));
            return Ok(());
        }
        let (tightening, bytes) = match self.version_of(tenant, current, declared) {
            Ok(built) => built,
            Err(refusal) => {
                drop(reply.send(Err(refusal)));
                return Ok(());
            }
        };
        let held = Tenant::new(tenant);
        let offered = if tightening {
            self.metadata.offer_checked(held, current, &bytes)
        } else {
            self.metadata.offer_catalog(held, current, &bytes)
        };
        match offered {
            Ok((version, _position)) => {
                // Durable in this turn's sync, so the next close assigns its epoch (`PROTOCOL.md` §11.13, *Its epoch*).
                self.durable.push(Durable::Offered);
                self.versions_waited.push((held, version, reply));
            }
            Err(refused) => drop(reply.send(Err(offer_refused(&refused, built_on)?))),
        }
        Ok(())
    }

    /// Whether `declared` tightens a predicate of `tenant`'s version `current`, and the bytes of the version it builds on it.
    fn version_of(
        &self,
        tenant: u64,
        current: u64,
        declared: &[Declared],
    ) -> Result<(bool, Vec<u8>), ErrorReply> {
        let held = self.contents(tenant, current)?;
        let tightening = tightens(&held, declared)?;
        let next = merged(held, declared)?;
        Ok((tightening, version_bytes(tenant, current + 1, next)?))
    }

    /// Answers each declaring Commit whose version stands: its epoch assigned and no check left, the epoch's first version.
    pub(super) fn answer_versions(&mut self) {
        let state = self.metadata.state();
        let waited = std::mem::take(&mut self.versions_waited);
        self.versions_waited = waited
            .into_iter()
            .filter_map(|(tenant, version, reply)| {
                let record = state.version_record(tenant, version);
                let settled = record.is_some_and(|r| *r.check() == CatalogCheck::Settled);
                match record.and_then(|r| r.effective()).filter(|_| settled) {
                    Some(epoch) => {
                        // An asker gone has no one to tell; the version stands.
                        drop(reply.send(Ok((version, epoch.get() << SEQUENCE_BITS))));
                        None
                    }
                    None => Some((tenant, version, reply)),
                }
            })
            .collect();
    }

    /// The catalog version a Drop calls for, pending and durable, its number answered; a version that would change nothing is not
    /// built, and 0 is answered. A Drop of everything removes the tenant's default-entry records in the same turn, durable with
    /// the version or alone (`PROTOCOL.md` §11.13, §13.27).
    /// An asker gone has no one to tell, so each reply's failed send is dropped.
    pub(super) fn drop_catalog(
        &mut self,
        tenant: u64,
        step: &DropStep,
        reply: Sender<Result<u64, ErrorReply>>,
    ) -> Result<(), RoleError> {
        let held_tenant = Tenant::new(tenant);
        if !self.metadata.state().holds_tenant(held_tenant) {
            drop(reply.send(Err(unknown_tenant(tenant))));
            return Ok(());
        }
        let built = match self.dropped_version(tenant, step)? {
            Ok(built) => built,
            Err(refusal) => {
                drop(reply.send(Err(refusal)));
                return Ok(());
            }
        };
        let removed = if *step == DropStep::Everything {
            match self.metadata.remove_defaults(held_tenant) {
                Ok(removed) => removed.is_some(),
                Err(refused) => {
                    drop(reply.send(Err(refused_or_stop(&refused)?)));
                    return Ok(());
                }
            }
        } else {
            false
        };
        match built {
            Some(version) => self.durable.push(Durable::Dropped(version, reply)),
            None if removed => self.durable.push(Durable::Dropped(0, reply)),
            None => drop(reply.send(Ok(0))),
        }
        Ok(())
    }

    /// The version `step` builds on `tenant`'s current one, offered as pending: its number, `None` when it would change nothing,
    /// or the refusal its asker hears.
    ///
    /// # Errors
    ///
    /// What [`refused_or_stop`] stops the group with: a failed log.
    fn dropped_version(
        &mut self,
        tenant: u64,
        step: &DropStep,
    ) -> Result<Result<Option<u64>, ErrorReply>, RoleError> {
        let current = self.metadata.state().current_version(Tenant::new(tenant));
        let held = match self.contents(tenant, current) {
            Ok(held) => held,
            Err(refusal) => return Ok(Err(refusal)),
        };
        let next: Vec<Declaration> = match step {
            DropStep::Everything => Vec::new(),
            DropStep::Predicate(predicate) => held
                .iter()
                .filter(|declaration| !declares(declaration, predicate))
                .cloned()
                .collect(),
            DropStep::Kept => return Ok(Ok(None)),
        };
        if next.len() == held.len() {
            return Ok(Ok(None));
        }
        let bytes = match version_bytes(tenant, current + 1, next) {
            Ok(bytes) => bytes,
            Err(refusal) => return Ok(Err(refusal)),
        };
        match self
            .metadata
            .offer_catalog(Tenant::new(tenant), current, &bytes)
        {
            Ok((version, _position)) => Ok(Ok(Some(version))),
            Err(refused) => Ok(Err(refused_or_stop(&refused)?)),
        }
    }

    /// The default entry of `predicate`, durable before its answer.
    /// An asker gone has no one to tell, so each reply's failed send is dropped.
    pub(super) fn record_default(
        &mut self,
        tenant: u64,
        predicate: &str,
        reply: Sender<Result<bool, ErrorReply>>,
    ) -> Result<(), RoleError> {
        let entry = DefaultEntry::new(tenant, predicate.to_string(), Vec::new());
        match self.metadata.record_default_entry(&entry) {
            Ok(Some(_position)) => self.durable.push(Durable::Recorded(false, reply)),
            Ok(None) => drop(reply.send(Ok(true))),
            Err(refused) => drop(reply.send(Err(refused_or_stop(&refused)?))),
        }
        Ok(())
    }
}

/// Whether `declaration` declares the predicate `predicate`.
fn declares(declaration: &Declaration, predicate: &str) -> bool {
    match declaration {
        Declaration::Predicate(held) => held.predicate().as_str() == predicate,
        Declaration::EntityType(_) | Declaration::Relation(_) => false,
    }
}

fn decoded(
    tenant: u64,
    version: u64,
    bytes: Option<&[u8]>,
) -> Result<Vec<Declaration>, ErrorReply> {
    let Some(bytes) = bytes else {
        return Err(ErrorReply::new(
            ErrorCode::Unavailable,
            format!("catalog version {version} of tenant {tenant} is no longer held"),
        ));
    };
    CatalogContents::decode(tenant, version, bytes)
        .map(|contents| contents.declarations().to_vec())
        .map_err(|error| ErrorReply::new(ErrorCode::Unavailable, error.to_string()))
}

/// The refusal of a version built on `checked`, which its partitions are still checking (`PROTOCOL.md` §11.13).
fn being_checked(checked: u64) -> ErrorReply {
    ErrorReply::new(
        ErrorCode::Conflict,
        format!("catalog version {checked} is being checked, and its Commit is not answered"),
    )
    .with_detail(CONFLICT_CATALOG)
}

/// What a declaring Commit hears of an offer the group refused: a version an offer of this turn got ahead of is stale, and a
/// failed log stops the owner.
fn offer_refused(refused: &ClusterError, built_on: u64) -> Result<ErrorReply, RoleError> {
    if let ClusterErrorEnum::StaleCatalog { current, .. } = refused.0 {
        return Ok(stale(built_on, current));
    }
    refused_or_stop(refused)
}

fn stale(built_on: u64, current: u64) -> ErrorReply {
    ErrorReply::new(
        ErrorCode::Conflict,
        format!("the version is built on {built_on}, and {current} is current"),
    )
    .with_detail(CONFLICT_CATALOG)
}
