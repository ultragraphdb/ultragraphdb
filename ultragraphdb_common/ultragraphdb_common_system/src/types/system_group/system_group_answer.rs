/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use futures::channel::oneshot::Sender;
use ultragraphdb_cluster_placement::{PartitionId, TenantRun as PlacedRun};
use ultragraphdb_core_ids::{Epoch, Tenant, TypePrefix};
use ultragraphdb_error_cluster::{ClusterError, ClusterErrorEnum};
use ultragraphdb_error_role::RoleError;
use ultragraphdb_protocol_scram::ScramVerifier;
use ultragraphdb_protocol_wire::{
    BucketRange, Catalog, ErrorCode, ErrorReply, Placement, PreferredLeader, TenantRun,
};
use ultragraphdb_traits_log::OrderedLog;

use crate::types::system_asked::SystemAsked;
use crate::types::system_group::{
    Durable, SystemGroup, stopped, system_group_sessions, system_group_syncing,
};
use crate::types::tenant_facts::TenantFacts;

impl<M: OrderedLog, S: OrderedLog> SystemGroup<M, S> {
    /// Takes what an endpoint core asked at `now_us`: answered now from what is durable, or kept for the turn's sync.
    ///
    /// An asker gone has no one to tell, so every send's failure is dropped.
    pub(super) fn take(&mut self, asked: SystemAsked, now_us: u64) -> Result<(), RoleError> {
        self.marks.expire(now_us);
        match asked {
            SystemAsked::Join { join, reply } => drop(reply.send(self.membership.join(&join))),
            SystemAsked::Members { reply } => drop(reply.send(self.membership.members())),
            SystemAsked::Placement { epoch, reply } => drop(reply.send(self.placement(epoch))),
            SystemAsked::Catalog {
                tenant,
                version,
                reply,
            } => drop(reply.send(self.catalog(tenant, version))),
            SystemAsked::Lease {
                prefix,
                partition,
                size,
                reply,
            } => self.asked_lease(prefix, partition, size, reply)?,
            SystemAsked::CloseRequest { epoch, reply } => {
                drop(reply.send(self.close_request(epoch)))
            }
            SystemAsked::Subscribe { from, reply } => drop(reply.send(self.subscribe(from))),
            SystemAsked::NextClose { after, reply } => self.next_close(after, reply),
            SystemAsked::Verifier { name, reply } => drop(reply.send(self.verifier(&name))),
            SystemAsked::RegisterSession {
                session,
                principal,
                reply,
            } => self.asked_register(session, principal, reply)?,
            SystemAsked::CheckSession {
                session,
                principal,
                reply,
            } => drop(reply.send(self.check(session, principal))),
            SystemAsked::SessionHeld { session, reply } => {
                let held = self.metadata.state().session_principal(session).is_ok();
                let _unheard: Result<(), bool> = reply.send(held);
            }
            other @ (SystemAsked::TenantFacts { .. }
            | SystemAsked::Map { .. }
            | SystemAsked::SchemaId { .. }
            | SystemAsked::Describe { .. }
            | SystemAsked::Declare { .. }
            | SystemAsked::DropCatalog { .. }
            | SystemAsked::RecordDefault { .. }
            | SystemAsked::Report { .. }) => self.take_catalog(other)?,
            other @ (SystemAsked::Connect { .. }
            | SystemAsked::Disconnect { .. }
            | SystemAsked::Session { .. }
            | SystemAsked::Check { .. }
            | SystemAsked::Ended { .. }
            | SystemAsked::Defaults { .. }
            | SystemAsked::CatalogAt { .. }
            | SystemAsked::Challenge { .. }
            | SystemAsked::Verify { .. }
            | SystemAsked::LatestClose { .. }) => self.take_system(other, now_us)?,
        }
        Ok(())
    }

    /// Takes what an endpoint core asked of a tenant's catalog and facts, the placement map and the partitions' reports.
    /// An asker gone has no one to tell, so every send's failure is dropped.
    fn take_catalog(&mut self, asked: SystemAsked) -> Result<(), RoleError> {
        match asked {
            SystemAsked::TenantFacts { tenant, reply } => {
                let _unheard: Result<(), TenantFacts> = reply.send(self.tenant_facts(tenant));
            }
            SystemAsked::Map { reply } => {
                drop(reply.send(self.metadata.state().placement().cloned()));
            }
            SystemAsked::SchemaId { tenant, reply } => drop(reply.send(self.schema_id(tenant))),
            SystemAsked::Describe { tenant, reply } => drop(reply.send(self.describe(tenant))),
            SystemAsked::Declare {
                tenant,
                built_on,
                declared,
                reply,
            } => self.declare(tenant, built_on, &declared, reply)?,
            SystemAsked::DropCatalog {
                tenant,
                step,
                reply,
            } => self.drop_catalog(tenant, &step, reply)?,
            SystemAsked::RecordDefault {
                tenant,
                predicate,
                reply,
            } => self.record_default(tenant, &predicate, reply)?,
            SystemAsked::Report { report, reply } => self.report(report, reply)?,
            // `take` hands the catalog's asks here alone.
            SystemAsked::Join { .. }
            | SystemAsked::Members { .. }
            | SystemAsked::Placement { .. }
            | SystemAsked::Catalog { .. }
            | SystemAsked::Lease { .. }
            | SystemAsked::CloseRequest { .. }
            | SystemAsked::Subscribe { .. }
            | SystemAsked::NextClose { .. }
            | SystemAsked::Verifier { .. }
            | SystemAsked::RegisterSession { .. }
            | SystemAsked::CheckSession { .. }
            | SystemAsked::SessionHeld { .. }
            | SystemAsked::Connect { .. }
            | SystemAsked::Disconnect { .. }
            | SystemAsked::Session { .. }
            | SystemAsked::Check { .. }
            | SystemAsked::Ended { .. }
            | SystemAsked::Defaults { .. }
            | SystemAsked::CatalogAt { .. }
            | SystemAsked::Challenge { .. }
            | SystemAsked::Verify { .. }
            | SystemAsked::LatestClose { .. } => {}
        }
        Ok(())
    }

    /// Takes what a role connection to the group asked at `now_us` (`PROTOCOL.md` §11.12, the system group's commands).
    /// An asker gone has no one to tell, so every send's failure is dropped.
    fn take_system(&mut self, asked: SystemAsked, now_us: u64) -> Result<(), RoleError> {
        match asked {
            SystemAsked::Connect { reply } => drop(reply.send(self.marks.open())),
            SystemAsked::Disconnect { connection } => self.marks.close(connection, now_us),
            SystemAsked::Session {
                session,
                principal,
                reply,
            } => self.asked_session(session, principal, now_us, reply)?,
            SystemAsked::Check {
                connection,
                checks,
                reply,
            } => drop(reply.send(system_group_sessions::checked(
                self.metadata.reads(),
                &mut self.marks,
                now_us,
                connection,
                checks,
            ))),
            SystemAsked::Ended {
                connection,
                sessions,
            } => system_group_sessions::ended(&mut self.marks, connection, &sessions),
            SystemAsked::Defaults { tenant, reply } => drop(reply.send(self.defaults(tenant))),
            SystemAsked::CatalogAt {
                tenant,
                epoch,
                reply,
            } => drop(reply.send(self.catalog_at(tenant, epoch))),
            SystemAsked::Challenge { name, reply } => drop(reply.send(self.challenge(&name))),
            SystemAsked::Verify {
                name,
                auth_message,
                proof,
                reply,
            } => drop(reply.send(self.verify(&name, &auth_message, &proof))),
            SystemAsked::LatestClose { reply } => drop(reply.send(self.closed)),
            // `take` hands the group's own asks here alone.
            SystemAsked::Join { .. }
            | SystemAsked::Members { .. }
            | SystemAsked::Placement { .. }
            | SystemAsked::Catalog { .. }
            | SystemAsked::Lease { .. }
            | SystemAsked::CloseRequest { .. }
            | SystemAsked::Subscribe { .. }
            | SystemAsked::NextClose { .. }
            | SystemAsked::Verifier { .. }
            | SystemAsked::RegisterSession { .. }
            | SystemAsked::CheckSession { .. }
            | SystemAsked::SessionHeld { .. }
            | SystemAsked::TenantFacts { .. }
            | SystemAsked::Map { .. }
            | SystemAsked::SchemaId { .. }
            | SystemAsked::Describe { .. }
            | SystemAsked::Declare { .. }
            | SystemAsked::DropCatalog { .. }
            | SystemAsked::Report { .. }
            | SystemAsked::RecordDefault { .. } => {}
        }
        Ok(())
    }

    /// A Lease, kept for the turn's sync: of type prefix 0 refused now.
    /// An asker gone has no one to tell, so each reply's failed send is dropped.
    fn asked_lease(
        &mut self,
        prefix: u16,
        partition: u32,
        size: u64,
        reply: Sender<Result<u64, ErrorReply>>,
    ) -> Result<(), RoleError> {
        let Ok(prefix) = TypePrefix::new(prefix) else {
            let refusal = ErrorReply::new(ErrorCode::Protocol, "a Lease of type prefix 0");
            drop(reply.send(Err(refusal)));
            return Ok(());
        };
        match self.lease(prefix, partition, size) {
            Ok(start) => self.durable.push(Durable::Lease(start, reply)),
            Err(refused) => drop(reply.send(Err(refused_or_stop(&refused)?))),
        }
        Ok(())
    }

    /// The sequencer told to close `epoch` early.
    fn close_request(&mut self, epoch: u64) -> Result<(), ErrorReply> {
        match Epoch::new(epoch) {
            Ok(epoch) => self
                .sequencer
                .request_close(epoch)
                .map_err(|error| refused(&error)),
            Err(error) => Err(ErrorReply::new(ErrorCode::Protocol, error.to_string())),
        }
    }

    /// A Subscribe from `from`, refused above the latest close.
    fn subscribe(&self, from: u64) -> Result<(), ErrorReply> {
        let last = self.sequencer.last_closed().get();
        if from > last {
            return Err(ErrorReply::new(
                ErrorCode::Protocol,
                format!("a Subscribe from epoch {from}, above the latest close {last}"),
            ));
        }
        Ok(())
    }

    /// The identity and verifier of the principal `name`.
    fn verifier(&self, name: &str) -> Option<(u64, ScramVerifier)> {
        self.metadata
            .state()
            .principal(name)
            .map(|principal| (principal.id(), principal.verifier().clone()))
    }

    /// A session's registration, kept for the turn's sync.
    /// An asker gone has no one to tell, so each reply's failed send is dropped.
    fn asked_register(
        &mut self,
        session: u128,
        principal: u64,
        reply: Sender<Result<(), ErrorReply>>,
    ) -> Result<(), RoleError> {
        match self.register(session, principal) {
            Ok(()) => self.durable.push(Durable::Registered(reply)),
            // No session the registry may evict yet: every other one has a commit in flight, or a close is still to carry the
            // evictions awaiting it. Either passes, so the client registers again after a delay (`PROTOCOL.md` §11.5).
            Err(refused) if matches!(refused.0, ClusterErrorEnum::Full { .. }) => {
                let busy = ErrorReply::new(ErrorCode::SessionBusy, refused.to_string());
                drop(reply.send(Err(busy)));
            }
            Err(refused) => drop(reply.send(Err(refused_or_stop(&refused)?))),
        }
        Ok(())
    }

    fn tenant_facts(&self, tenant: u64) -> TenantFacts {
        system_group_syncing::tenant_facts(self.metadata.reads(), self.closed, tenant)
    }

    fn lease(
        &mut self,
        prefix: TypePrefix,
        partition: u32,
        size: u64,
    ) -> Result<u64, ClusterError> {
        let (block, _position) =
            self.metadata
                .lease_block(prefix, PartitionId::new(partition)?, size)?;
        Ok(block.start().get())
    }

    fn register(&mut self, session: u128, principal: u64) -> Result<(), ClusterError> {
        let name = self
            .metadata
            .state()
            .principal_by_id(principal)
            .map(|held| held.name().to_string())
            .ok_or_else(|| ClusterError::unknown_principal(&principal.to_string()))?;
        let marks = &self.marks;
        let _registered = self
            .metadata
            .register_session(session, &name, |held| marks.protects(held))?;
        Ok(())
    }

    /// The owner check: `session` held by the registry, its eviction appended or not, and `principal`'s.
    fn check(&self, session: u128, principal: u64) -> Result<(), ErrorReply> {
        system_group_syncing::check(self.metadata.reads(), session, principal)
    }

    /// The next close after `after` for a subscriber waiting on it, answered now or once the sequencer closes it. A
    /// subscriber gone has no one to tell, so each reply's failed send is dropped.
    fn next_close(
        &mut self,
        after: u64,
        reply: futures::channel::oneshot::Sender<
            Result<ultragraphdb_cluster_sequencer::Close, ErrorReply>,
        >,
    ) {
        let protocol = |message: String| ErrorReply::new(ErrorCode::Protocol, message);
        let cursor = match Epoch::new(after) {
            Ok(cursor) => cursor,
            Err(error) => {
                drop(reply.send(Err(protocol(error.to_string()))));
                return;
            }
        };
        match self.sequencer.close_after(cursor) {
            Ok(Some(close)) => drop(reply.send(Ok(close))),
            Ok(None) => {
                // A connection that read a frame first gave its wait up, so those are dropped as this one joins them.
                self.closes_waited
                    .retain(|(_, waiting)| !waiting.is_canceled());
                self.closes_waited.push((cursor, reply));
            }
            Err(error) => drop(reply.send(Err(protocol(error.to_string())))),
        }
    }

    fn placement(&self, epoch: u64) -> Result<Placement, ErrorReply> {
        let state = self.metadata.state();
        let map = state.placement().ok_or_else(|| {
            ErrorReply::new(
                ErrorCode::Unavailable,
                "the metadata group holds no placement",
            )
        })?;
        let change = map.latest_change();
        let from = change.get().max(1);
        let mut reply = Placement::reply(self.metadata.applied().get(), epoch, from, 0);
        let ranges = map.ranges_at(change);
        let mut partitions: Vec<PartitionId> =
            ranges.iter().map(|(_, partition)| *partition).collect();
        partitions.sort_unstable();
        partitions.dedup();
        for (range, partition) in ranges {
            reply.push_range(BucketRange::new(
                range.start(),
                range.end(),
                partition.get(),
            ));
        }
        for tenant in state.tenants() {
            reply.push_run(TenantRun::new(tenant.get(), PlacedRun::of(tenant).get()));
        }
        for partition in partitions {
            if let Some(leader) = map.preferred_leader(partition) {
                reply.push_leader(PreferredLeader::new(partition.get(), leader.get()));
            }
        }
        Ok(reply)
    }

    pub(super) fn catalog(&self, tenant: u64, version: u64) -> Result<Catalog, ErrorReply> {
        let held = Tenant::new(tenant);
        let state = self.metadata.state();
        let wanted = if version == 0 {
            state.current_version(held)
        } else {
            version
        };
        let never = || {
            ErrorReply::new(
                ErrorCode::Refused,
                format!("tenant {tenant} never held catalog version {wanted}"),
            )
            .with_detail(wanted)
        };
        let versions = state.versions(held);
        let record = versions
            .iter()
            .find(|record| record.version() == wanted)
            .ok_or_else(never)?;
        Ok(Catalog::reply(
            tenant,
            wanted,
            record.effective().map_or(0, Epoch::get),
            self.metadata.applied().get(),
            record.bytes().to_vec(),
        ))
    }
}

/// The refusal an asker hears of `error`; a failed log stops the owner instead.
pub(super) fn refused_or_stop(error: &ClusterError) -> Result<ErrorReply, RoleError> {
    if matches!(error.0, ClusterErrorEnum::Log(_)) {
        return Err(stopped(error));
    }
    Ok(refused(error))
}

/// `error` as code 7, naming what refused it, `detail` the maximum for a Lease past it.
fn refused(error: &ClusterError) -> ErrorReply {
    let reply = ErrorReply::new(ErrorCode::Refused, error.to_string());
    if let ClusterErrorEnum::BlockSize { max, .. } = error.0 {
        return reply.with_detail(max);
    }
    reply
}
