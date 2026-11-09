/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::net::IpAddr;
use std::time::{Duration, Instant};

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

use crate::types::asked::Asked;
use crate::types::auth_throttle::AuthThrottle;
use crate::types::login_asked::LoginAsked;
use crate::types::owner::{Durable, Owner, owner_syncing, stopped};
use crate::types::tenant_facts::TenantFacts;

impl<M: OrderedLog, S: OrderedLog> Owner<M, S> {
    /// Takes what an endpoint core asked: answered now from what is durable, or kept for the turn's sync.
    ///
    /// An asker gone has no one to tell, so every send's failure is dropped.
    pub(super) fn take(&mut self, asked: Asked) -> Result<(), RoleError> {
        match asked {
            Asked::Join { join, reply } => drop(reply.send(self.membership.join(&join))),
            Asked::Members { reply } => drop(reply.send(self.membership.members())),
            Asked::Placement { epoch, reply } => drop(reply.send(self.placement(epoch))),
            Asked::Catalog {
                tenant,
                version,
                reply,
            } => drop(reply.send(self.catalog(tenant, version))),
            Asked::Lease {
                prefix,
                partition,
                size,
                reply,
            } => self.asked_lease(prefix, partition, size, reply)?,
            Asked::CloseRequest { epoch, reply } => drop(reply.send(self.close_request(epoch))),
            Asked::Subscribe { from, reply } => drop(reply.send(self.subscribe(from))),
            Asked::NextClose { after, reply } => self.next_close(after, reply),
            Asked::Login(login) => self.take_login(login),
            Asked::RegisterSession {
                session,
                principal,
                reply,
            } => self.asked_register(session, principal, reply)?,
            Asked::CheckSession {
                session,
                principal,
                reply,
            } => drop(reply.send(self.check(session, principal))),
            Asked::BeginCommit {
                session,
                principal,
                writes,
                reply,
            } => drop(reply.send(self.begin_commit(session, principal, writes))),
            Asked::CommitEnded { session } => {
                let _was = self.in_flight.remove(&session);
            }
            Asked::SessionHeld { session, reply } => {
                let held = self.metadata.state().session_principal(session).is_ok();
                let _unheard: Result<(), bool> = reply.send(held);
            }
            Asked::TenantFacts { tenant, reply } => drop(reply.send(self.tenant_facts(tenant))),
            Asked::Map { reply } => {
                drop(reply.send(self.metadata.state().placement().cloned()));
            }
            Asked::SchemaId { tenant, reply } => drop(reply.send(self.schema_id(tenant))),
            Asked::Describe { tenant, reply } => drop(reply.send(self.describe(tenant))),
            Asked::Declare {
                tenant,
                built_on,
                declared,
                reply,
            } => self.declare(tenant, built_on, &declared, reply)?,
            Asked::DropCatalog {
                tenant,
                step,
                reply,
            } => self.drop_catalog(tenant, &step, reply)?,
            Asked::RecordDefault {
                tenant,
                predicate,
                reply,
            } => self.record_default(tenant, &predicate, reply)?,
            Asked::Report { report, reply } => self.report(report, reply)?,
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

    /// Answers a login's ask from the principals and the failed authentications every core counts here, or counts what it tells.
    fn take_login(&mut self, login: LoginAsked) {
        match login {
            LoginAsked::Verifier {
                name,
                source,
                reply,
            } => {
                let delay = source.map_or(Duration::ZERO, |source| self.logins.delay(source));
                drop(reply.send((self.verifier(&name), delay)));
            }
            LoginAsked::Admit { source, at, reply } => {
                let _unheard: Result<(), bool> = reply.send(self.logins.admits(source, at));
            }
            LoginAsked::Outcome {
                source,
                at,
                succeeded,
            } => self.login_outcome(source, at, succeeded),
        }
    }

    /// Counts an authentication's outcome from `source`, logging the ban a failure begins.
    fn login_outcome(&mut self, source: IpAddr, at: Instant, succeeded: bool) {
        if succeeded {
            return self.logins.succeeded(source);
        }
        if self.logins.failed(source, at) {
            tracing::warn!(
                target: "audit",
                event = "ban",
                source = %AuthThrottle::source_of(source),
                "a source past its failed authentications is banned"
            );
        }
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

    /// The owner check of a commit, one in flight per session (`PROTOCOL.md` §11.5).
    fn begin_commit(
        &mut self,
        session: u128,
        principal: u64,
        writes: bool,
    ) -> Result<(), ErrorReply> {
        owner_syncing::begin_commit(
            self.metadata.reads(),
            &mut self.in_flight,
            (session, principal, writes),
        )
    }

    fn tenant_facts(&self, tenant: u64) -> TenantFacts {
        owner_syncing::tenant_facts(self.metadata.reads(), self.closed, tenant)
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
        let in_flight = &self.in_flight;
        let _registered = self
            .metadata
            .register_session(session, &name, |held| in_flight.get(&held) == Some(&true))?;
        Ok(())
    }

    /// The owner check: `session` held by the registry, its eviction appended or not, and `principal`'s.
    fn check(&self, session: u128, principal: u64) -> Result<(), ErrorReply> {
        owner_syncing::check(self.metadata.reads(), session, principal)
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

    fn catalog(&self, tenant: u64, version: u64) -> Result<Catalog, ErrorReply> {
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
