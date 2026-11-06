/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use futures::channel::oneshot::Sender;
use ultragraphdb_cluster_metadata::{CatalogRecord, MetadataCommand};
use ultragraphdb_core_catalog::CatalogContents;
use ultragraphdb_core_ids::{Tenant, Version};
use ultragraphdb_core_mapping::Declaration;
use ultragraphdb_error_role::RoleError;
use ultragraphdb_protocol_wire::{Declared, Describe, ErrorCode, ErrorReply};
use ultragraphdb_traits_catalog::DefaultEntry;
use ultragraphdb_traits_log::OrderedLog;

use crate::types::asked::DropStep;
use crate::types::owner::owner_answer::refused_or_stop;
use crate::types::owner::{Durable, Owner};
use crate::utils::declarations::{describe_of, merged, version_bytes};

/// The kind a Catalog conflict names (`PROTOCOL.md` §9.4).
const CONFLICT_CATALOG: u64 = 5;

/// How many bits of a version its sequence holds: an epoch's first version is the epoch shifted by these (`PROTOCOL.md` §1).
const SEQUENCE_BITS: u32 = 22;

/// What a version's bytes may hold: more is refused at its Commit (`PROTOCOL.md` §11.13).
pub(crate) const CATALOG_CAP: usize = MetadataCommand::CATALOG_BYTES_MAX;

impl<M: OrderedLog, S: OrderedLog> Owner<M, S> {
    /// The declarations of `tenant`'s version `version`, none for version 0.
    fn contents(&self, tenant: u64, version: u64) -> Result<Vec<Declaration>, ErrorReply> {
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

    /// What `tenant`'s catalog declares in effect at the latest close, and the default entries the group records.
    pub(super) fn describe(&self, tenant: u64) -> Result<Describe, ErrorReply> {
        let state = self.metadata.state();
        let held = Tenant::new(tenant);
        let closed = self.sequencer.last_closed();
        let in_effect = state
            .catalog_at(held, closed)
            .map_err(|error| ErrorReply::new(ErrorCode::Unavailable, error.to_string()))?;
        let declarations = match in_effect {
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

    /// A catalog version of `declared`, built on `built_on`, offered as pending: its answer waits for its effective epoch.
    pub(super) fn declare(
        &mut self,
        tenant: u64,
        built_on: u64,
        declared: &[Declared],
        reply: Sender<Result<u64, ErrorReply>>,
    ) -> Result<(), RoleError> {
        let current = self.metadata.state().current_version(Tenant::new(tenant));
        if built_on != current {
            drop(reply.send(Err(stale(built_on, current))));
            return Ok(());
        }
        let bytes = self
            .contents(tenant, current)
            .and_then(|held| merged(held, declared))
            .and_then(|next| version_bytes(tenant, current + 1, next));
        let bytes = match bytes {
            Ok(bytes) => bytes,
            Err(refusal) => {
                drop(reply.send(Err(refusal)));
                return Ok(());
            }
        };
        match self
            .metadata
            .offer_catalog(Tenant::new(tenant), current, &bytes)
        {
            Ok((version, _position)) => {
                self.versions_waited
                    .push((Tenant::new(tenant), version, reply));
            }
            Err(refused) => drop(reply.send(Err(refused_or_stop(&refused)?))),
        }
        Ok(())
    }

    /// Answers each declaring Commit whose version an epoch was assigned: the epoch's first version.
    pub(super) fn answer_versions(&mut self) {
        let state = self.metadata.state();
        let waited = std::mem::take(&mut self.versions_waited);
        self.versions_waited = waited
            .into_iter()
            .filter_map(|(tenant, version, reply)| {
                let versions = state.versions(tenant);
                let effective = versions
                    .iter()
                    .find(|record| record.version() == version)
                    .and_then(|record| record.effective());
                match effective {
                    Some(epoch) => {
                        // An asker gone has no one to tell; the version stands.
                        drop(reply.send(Ok(epoch.get() << SEQUENCE_BITS)));
                        None
                    }
                    None => Some((tenant, version, reply)),
                }
            })
            .collect();
    }

    /// The catalog version a Drop calls for, pending and durable; a version that would change nothing is not built.
    pub(super) fn drop_catalog(
        &mut self,
        tenant: u64,
        step: &DropStep,
        reply: Sender<Result<(), ErrorReply>>,
    ) -> Result<(), RoleError> {
        let current = self.metadata.state().current_version(Tenant::new(tenant));
        let held = match self.contents(tenant, current) {
            Ok(held) => held,
            Err(refusal) => {
                drop(reply.send(Err(refusal)));
                return Ok(());
            }
        };
        let next: Vec<Declaration> = match step {
            DropStep::Everything => Vec::new(),
            DropStep::Predicate(predicate) => held
                .iter()
                .filter(|declaration| !declares(declaration, predicate))
                .cloned()
                .collect(),
        };
        if next.len() == held.len() {
            drop(reply.send(Ok(())));
            return Ok(());
        }
        let bytes = match version_bytes(tenant, current + 1, next) {
            Ok(bytes) => bytes,
            Err(refusal) => {
                drop(reply.send(Err(refusal)));
                return Ok(());
            }
        };
        match self
            .metadata
            .offer_catalog(Tenant::new(tenant), current, &bytes)
        {
            Ok(_) => self.durable.push(Durable::Registered(reply)),
            Err(refused) => drop(reply.send(Err(refused_or_stop(&refused)?))),
        }
        Ok(())
    }

    /// The default entry of `predicate`, durable before its answer.
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

fn stale(built_on: u64, current: u64) -> ErrorReply {
    ErrorReply::new(
        ErrorCode::Conflict,
        format!("the version is built on {built_on}, and {current} is current"),
    )
    .with_detail(CONFLICT_CATALOG)
}
