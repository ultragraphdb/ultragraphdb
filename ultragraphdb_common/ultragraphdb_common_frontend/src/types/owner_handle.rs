/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use futures::channel::{mpsc, oneshot};
use ultragraphdb_cluster_placement::PlacementMap;
use ultragraphdb_cluster_sequencer::Close;
use ultragraphdb_protocol_scram::ScramVerifier;
use ultragraphdb_protocol_wire::{
    Catalog, Declared, Describe, ErrorCode, ErrorReply, Join, Members, Placement,
};

use crate::types::asked::Asked;
pub use crate::types::asked::DropStep;
use crate::types::report_asked::ReportAsked;
use crate::types::tenant_facts::TenantFacts;

/// An endpoint core's way to the owner: every question goes by message, and nothing the owner decides is kept here.
///
/// The channel is unbounded because each connection waits for its answer before it asks again, so the connections' own
/// bound bounds it.
#[derive(Debug, Clone)]
pub struct OwnerHandle {
    inbox: mpsc::UnboundedSender<Asked>,
}

impl OwnerHandle {
    pub(crate) fn new(inbox: mpsc::UnboundedSender<Asked>) -> Self {
        Self { inbox }
    }

    /// Asks the owner what `build` says and waits for its answer; `None` when the owner stopped.
    async fn ask<T>(&self, build: impl FnOnce(oneshot::Sender<T>) -> Asked) -> Option<T> {
        let (reply, answer) = oneshot::channel();
        self.inbox.unbounded_send(build(reply)).ok()?;
        answer.await.ok()
    }

    /// As [`OwnerHandle::ask`], an owner that stopped refused as unavailable.
    async fn ask_or_refuse<T>(
        &self,
        build: impl FnOnce(oneshot::Sender<Result<T, ErrorReply>>) -> Asked,
    ) -> Result<T, ErrorReply> {
        self.ask(build).await.unwrap_or_else(|| Err(stopped()))
    }

    /// Registers `join`.
    ///
    /// # Errors
    ///
    /// The refusal of [`crate::Membership::join`], or unavailable when the owner stopped.
    pub async fn join(&self, join: Join) -> Result<(), ErrorReply> {
        self.ask_or_refuse(|reply| Asked::Join { join, reply })
            .await
    }

    /// Who joined; `None` when the owner stopped.
    pub async fn members(&self) -> Option<Members> {
        self.ask(|reply| Asked::Members { reply }).await
    }

    /// The placement map at `epoch`, 0 for the latest.
    ///
    /// # Errors
    ///
    /// Unavailable when the owner stopped.
    pub async fn placement(&self, epoch: u64) -> Result<Placement, ErrorReply> {
        self.ask_or_refuse(|reply| Asked::Placement { epoch, reply })
            .await
    }

    /// A catalog version of `tenant`, `version` 0 for the current one.
    ///
    /// # Errors
    ///
    /// Code 7 for a version the tenant never held, or unavailable when the owner stopped.
    pub async fn catalog(&self, tenant: u64, version: u64) -> Result<Catalog, ErrorReply> {
        self.ask_or_refuse(|reply| Asked::Catalog {
            tenant,
            version,
            reply,
        })
        .await
    }

    /// Leases `size` sequences of type `prefix` for `partition`: the block's first sequence, answered once durable.
    ///
    /// # Errors
    ///
    /// Code 7 naming the maximum for a size above it, or unavailable when the owner stopped.
    pub async fn lease(&self, prefix: u16, partition: u32, size: u64) -> Result<u64, ErrorReply> {
        self.ask_or_refuse(|reply| Asked::Lease {
            prefix,
            partition,
            size,
            reply,
        })
        .await
    }

    /// Asks for the close of `epoch` as soon as the sequencer may.
    ///
    /// # Errors
    ///
    /// Code 7 for an epoch past the open one, or unavailable when the owner stopped.
    pub async fn close_request(&self, epoch: u64) -> Result<(), ErrorReply> {
        self.ask_or_refuse(|reply| Asked::CloseRequest { epoch, reply })
            .await
    }

    /// Whether a subscriber holding the closes through `from` may start.
    ///
    /// # Errors
    ///
    /// Code 1 naming both epochs for a `from` above the latest close, or unavailable when the owner stopped.
    pub async fn subscribe(&self, from: u64) -> Result<(), ErrorReply> {
        self.ask_or_refuse(|reply| Asked::Subscribe { from, reply })
            .await
    }

    /// The durable close after `after`, once there is one.
    ///
    /// # Errors
    ///
    /// Code 1 when that close is no longer held, or unavailable when the owner stopped.
    pub async fn next_close(&self, after: u64) -> Result<Close, ErrorReply> {
        self.ask_or_refuse(|reply| Asked::NextClose { after, reply })
            .await
    }

    /// The principal holding `name`, and its verifier; `None` when none does or the owner stopped.
    pub async fn verifier(&self, name: String) -> Option<(u64, ScramVerifier)> {
        self.ask(|reply| Asked::Verifier { name, reply })
            .await
            .flatten()
    }

    /// Registers `session` for `principal`, answered once durable. A full registry evicts its oldest session with no commit in
    /// flight.
    ///
    /// # Errors
    ///
    /// Code 14 when the full registry holds no session it may evict yet, every other one with a commit in flight; code 7 naming
    /// what else refused it, or unavailable when the owner stopped.
    pub async fn register_session(&self, session: u128, principal: u64) -> Result<(), ErrorReply> {
        self.ask_or_refuse(|reply| Asked::RegisterSession {
            session,
            principal,
            reply,
        })
        .await
    }

    /// The owner check of a commit through `session` by `principal`, which holds the session in flight until
    /// [`OwnerHandle::commit_ended`].
    ///
    /// # Errors
    ///
    /// As [`OwnerHandle::check_session`], and code 14 when a commit of the session is in flight.
    pub async fn begin_commit(&self, session: u128, principal: u64) -> Result<(), ErrorReply> {
        self.ask_or_refuse(|reply| Asked::BeginCommit {
            session,
            principal,
            writes: true,
            reply,
        })
        .await
    }

    /// The owner check of a retried Commit's Recall, held in flight as [`OwnerHandle::begin_commit`] holds a commit. A Recall
    /// records nothing in a partition's table, so a full registry may evict its session meanwhile (`PROTOCOL.md` §11.5).
    ///
    /// # Errors
    ///
    /// As [`OwnerHandle::begin_commit`].
    pub async fn begin_recall(&self, session: u128, principal: u64) -> Result<(), ErrorReply> {
        self.ask_or_refuse(|reply| Asked::BeginCommit {
            session,
            principal,
            writes: false,
            reply,
        })
        .await
    }

    /// The commit in flight through `session` was answered, or given up. An owner stopped holds none in flight.
    pub fn commit_ended(&self, session: u128) {
        drop(self.inbox.unbounded_send(Asked::CommitEnded { session }));
    }

    /// Whether the registry holds `session`; an owner stopped holds nothing.
    pub async fn session_held(&self, session: u128) -> bool {
        self.ask(|reply| Asked::SessionHeld { session, reply })
            .await
            .unwrap_or(false)
    }

    /// What a request naming `tenant` needs to know.
    ///
    /// # Errors
    ///
    /// Unavailable when the owner stopped.
    pub async fn tenant_facts(&self, tenant: u64) -> Result<TenantFacts, ErrorReply> {
        self.ask(|reply| Asked::TenantFacts { tenant, reply })
            .await
            .ok_or_else(stopped)
    }

    /// The placement map; `None` while the group holds none or the owner stopped.
    pub async fn map(&self) -> Option<PlacementMap> {
        self.ask(|reply| Asked::Map { reply }).await.flatten()
    }

    /// `tenant`'s schema id, of its latest catalog version.
    ///
    /// # Errors
    ///
    /// Unavailable when the owner stopped, or a held version does not decode.
    pub async fn schema_id(&self, tenant: u64) -> Result<u64, ErrorReply> {
        self.ask_or_refuse(|reply| Asked::SchemaId { tenant, reply })
            .await
    }

    /// What `tenant`'s catalog declares in effect at the latest close, and its metadata records.
    ///
    /// # Errors
    ///
    /// Unavailable when the owner stopped, or a held version does not decode.
    pub async fn describe(&self, tenant: u64) -> Result<Describe, ErrorReply> {
        self.ask_or_refuse(|reply| Asked::Describe { tenant, reply })
            .await
    }

    /// A catalog version of `declared` built on `built_on`, committed as pending: the first version of its effective epoch, once
    /// the epoch is assigned and, for a version tightening a predicate, every partition holding the tenant has passed its check.
    ///
    /// # Errors
    ///
    /// Code 2 of kind Catalog for a version built on one no longer current or on one being checked; code 7, `detail` 523,000,
    /// for one past its cap; code 7 for declarations tightening and loosening at once, and for a change the data refuses;
    /// code 24 for a partition silent past `catalog_check_ms`.
    pub async fn declare(
        &self,
        tenant: u64,
        built_on: u64,
        declared: Vec<Declared>,
    ) -> Result<u64, ErrorReply> {
        self.ask_or_refuse(|reply| Asked::Declare {
            tenant,
            built_on,
            declared,
            reply,
        })
        .await
    }

    /// A partition's report on a catalog version, once the metadata group holds it (`PROTOCOL.md` §13.19).
    ///
    /// # Errors
    ///
    /// Code 17 when the owner stopped.
    pub async fn report(&self, report: ReportAsked) -> Result<(), ErrorReply> {
        self.ask_or_refuse(|reply| Asked::Report { report, reply })
            .await
    }

    /// The catalog version a Drop calls for, pending and durable; none where it would change nothing.
    ///
    /// # Errors
    ///
    /// Code 7 naming what refused it, or unavailable when the owner stopped.
    pub async fn drop_catalog(&self, tenant: u64, step: DropStep) -> Result<(), ErrorReply> {
        self.ask_or_refuse(|reply| Asked::DropCatalog {
            tenant,
            step,
            reply,
        })
        .await
    }

    /// The default entry of `predicate`, recorded durably: whether it was held already.
    ///
    /// # Errors
    ///
    /// Code 7 naming what refused it, or unavailable when the owner stopped.
    pub async fn record_default(&self, tenant: u64, predicate: String) -> Result<bool, ErrorReply> {
        self.ask_or_refuse(|reply| Asked::RecordDefault {
            tenant,
            predicate,
            reply,
        })
        .await
    }

    /// The owner check of a request through `session` by `principal`.
    ///
    /// # Errors
    ///
    /// Code 12 for a session the registry does not hold, code 21 `detail` 2 for another principal's, or unavailable
    /// when the owner stopped.
    pub async fn check_session(&self, session: u128, principal: u64) -> Result<(), ErrorReply> {
        self.ask_or_refuse(|reply| Asked::CheckSession {
            session,
            principal,
            reply,
        })
        .await
    }
}

fn stopped() -> ErrorReply {
    ErrorReply::new(ErrorCode::Unavailable, "the frontend's owner stopped")
}
