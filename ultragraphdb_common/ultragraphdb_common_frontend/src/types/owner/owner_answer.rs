/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_placement::{NamespaceRun as PlacedRun, PartitionId};
use ultragraphdb_core_ids::{Epoch, Namespace, TypePrefix};
use ultragraphdb_error_cluster::{ClusterError, ClusterErrorEnum};
use ultragraphdb_error_role::RoleError;
use ultragraphdb_protocol_wire::{
    BucketRange, Catalog, ErrorCode, ErrorReply, NamespaceRun, Placement, PreferredLeader,
};
use ultragraphdb_traits_log::OrderedLog;

use crate::types::asked::Asked;
use crate::types::owner::{Durable, Owner, stopped};

/// The `detail` of code 21 for a write another principal's session may not make (`PROTOCOL.md` §11.3).
const ACTION_WRITE: u64 = 2;

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
                namespace,
                version,
                reply,
            } => drop(reply.send(self.catalog(namespace, version))),
            Asked::Lease {
                prefix,
                partition,
                size,
                reply,
            } => {
                let Ok(prefix) = TypePrefix::new(prefix) else {
                    let refusal = ErrorReply::new(ErrorCode::Protocol, "a Lease of type prefix 0");
                    drop(reply.send(Err(refusal)));
                    return Ok(());
                };
                match self.lease(prefix, partition, size) {
                    Ok(start) => self.durable.push(Durable::Lease(start, reply)),
                    Err(refused) => drop(reply.send(Err(refused_or_stop(&refused)?))),
                }
            }
            Asked::CloseRequest { epoch, reply } => {
                let answer = match Epoch::new(epoch) {
                    Ok(epoch) => self
                        .sequencer
                        .request_close(epoch)
                        .map_err(|error| refused(&error)),
                    Err(error) => Err(ErrorReply::new(ErrorCode::Protocol, error.to_string())),
                };
                drop(reply.send(answer));
            }
            Asked::Subscribe { from, reply } => {
                let last = self.sequencer.last_closed().get();
                drop(reply.send(if from > last {
                    Err(ErrorReply::new(
                        ErrorCode::Protocol,
                        format!("a Subscribe from epoch {from}, above the latest close {last}"),
                    ))
                } else {
                    Ok(())
                }));
            }
            Asked::NextClose { after, reply } => self.next_close(after, reply),
            Asked::Verifier { name, reply } => {
                let held = self
                    .metadata
                    .state()
                    .principal(&name)
                    .map(|principal| (principal.id(), principal.verifier().clone()));
                drop(reply.send(held));
            }
            Asked::RegisterSession {
                session,
                principal,
                reply,
            } => match self.register(session, principal) {
                Ok(()) => self.durable.push(Durable::Registered(reply)),
                Err(refused) => drop(reply.send(Err(refused_or_stop(&refused)?))),
            },
            Asked::CheckSession {
                session,
                principal,
                reply,
            } => drop(reply.send(self.check(session, principal))),
        }
        Ok(())
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
        let _registered = self.metadata.register_session(session, &name)?;
        Ok(())
    }

    fn check(&self, session: u128, principal: u64) -> Result<(), ErrorReply> {
        let held = self
            .metadata
            .state()
            .session_principal(session)
            .map_err(|_| {
                ErrorReply::new(
                    ErrorCode::SessionEvicted,
                    "the registry evicted this session, or never registered it",
                )
            })?;
        if held == principal {
            return Ok(());
        }
        Err(ErrorReply::new(
            ErrorCode::PermissionDenied,
            "the session is another principal's",
        )
        .with_detail(ACTION_WRITE))
    }

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
        for namespace in state.namespaces() {
            reply.push_run(NamespaceRun::new(
                namespace.get(),
                PlacedRun::of(namespace).get(),
            ));
        }
        for partition in partitions {
            if let Some(leader) = map.preferred_leader(partition) {
                reply.push_leader(PreferredLeader::new(partition.get(), leader.get()));
            }
        }
        Ok(reply)
    }

    fn catalog(&self, namespace: u64, version: u64) -> Result<Catalog, ErrorReply> {
        let held = Namespace::new(namespace);
        let state = self.metadata.state();
        let wanted = if version == 0 {
            state.current_version(held)
        } else {
            version
        };
        let never = || {
            ErrorReply::new(
                ErrorCode::Refused,
                format!("namespace {namespace} never held catalog version {wanted}"),
            )
            .with_detail(wanted)
        };
        let versions = state.versions(held);
        let record = versions
            .iter()
            .find(|record| record.version() == wanted)
            .ok_or_else(never)?;
        Ok(Catalog::reply(
            namespace,
            wanted,
            record.effective().map_or(0, Epoch::get),
            self.metadata.applied().get(),
            record.bytes().to_vec(),
        ))
    }
}

/// The refusal an asker hears of `error`; a failed log stops the owner instead.
fn refused_or_stop(error: &ClusterError) -> Result<ErrorReply, RoleError> {
    if matches!(error.0, ClusterErrorEnum::Log(_)) {
        return Err(stopped(error));
    }
    Ok(refused(error))
}

/// `error` as code 7, naming what refused it.
fn refused(error: &ClusterError) -> ErrorReply {
    ErrorReply::new(ErrorCode::Refused, error.to_string())
}
