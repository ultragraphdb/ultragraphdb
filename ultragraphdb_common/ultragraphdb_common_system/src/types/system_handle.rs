/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use futures::channel::{mpsc, oneshot};
use ultragraphdb_cluster_placement::PlacementMap;
use ultragraphdb_cluster_sequencer::Close;
use ultragraphdb_protocol_scram::ScramVerifier;
use ultragraphdb_protocol_wire::{
    Catalog, Declared, Defaults, Describe, ErrorCode, ErrorReply, Join, Members, Placement,
    SessionCheck,
};

use crate::types::report_asked::ReportAsked;
pub use crate::types::system_asked::DropStep;
use crate::types::system_asked::SystemAsked;
use crate::types::tenant_facts::TenantFacts;

/// An endpoint core's way to the owner: every question goes by message, and nothing the owner decides is kept here.
///
/// The channel is unbounded because each connection waits for its answer before it asks again, so the connections' own
/// bound bounds it.
///
/// Every call's future asks the owner and waits for its answer, as `SystemHandle::ask` says: dropped, it leaves what it
/// asked with the owner, which decides it, a durable change included, and its answer is lost.
#[derive(Debug, Clone)]
pub struct SystemHandle {
    inbox: mpsc::UnboundedSender<SystemAsked>,
}

impl SystemHandle {
    pub(crate) fn new(inbox: mpsc::UnboundedSender<SystemAsked>) -> Self {
        Self { inbox }
    }

    /// Asks the owner what `build` says and waits for its answer; `None` when the owner stopped.
    /// A future dropped before its answer leaves the ask with the owner, which decides it and finds no one to tell.
    async fn ask<T>(&self, build: impl FnOnce(oneshot::Sender<T>) -> SystemAsked) -> Option<T> {
        let (reply, answer) = oneshot::channel();
        self.inbox.unbounded_send(build(reply)).ok()?;
        answer.await.ok()
    }

    /// As [`SystemHandle::ask`], an owner that stopped refused as unavailable.
    /// A dropped future leaves what [`SystemHandle::ask`] says.
    async fn ask_or_refuse<T>(
        &self,
        build: impl FnOnce(oneshot::Sender<Result<T, ErrorReply>>) -> SystemAsked,
    ) -> Result<T, ErrorReply> {
        self.ask(build).await.unwrap_or_else(|| Err(stopped()))
    }

    /// Registers `join`.
    ///
    /// # Errors
    ///
    /// The refusal of [`crate::Membership::join`], or unavailable when the owner stopped.
    pub async fn join(&self, join: Join) -> Result<(), ErrorReply> {
        self.ask_or_refuse(|reply| SystemAsked::Join { join, reply })
            .await
    }

    /// Who joined; `None` when the owner stopped.
    pub async fn members(&self) -> Option<Members> {
        self.ask(|reply| SystemAsked::Members { reply }).await
    }

    /// The placement map at `epoch`, 0 for the latest.
    ///
    /// # Errors
    ///
    /// Unavailable when the owner stopped.
    pub async fn placement(&self, epoch: u64) -> Result<Placement, ErrorReply> {
        self.ask_or_refuse(|reply| SystemAsked::Placement { epoch, reply })
            .await
    }

    /// A catalog version of `tenant`, `version` 0 for the current one.
    ///
    /// # Errors
    ///
    /// Code 7 for a version the tenant never held, or unavailable when the owner stopped.
    pub async fn catalog(&self, tenant: u64, version: u64) -> Result<Catalog, ErrorReply> {
        self.ask_or_refuse(|reply| SystemAsked::Catalog {
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
        self.ask_or_refuse(|reply| SystemAsked::Lease {
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
        self.ask_or_refuse(|reply| SystemAsked::CloseRequest { epoch, reply })
            .await
    }

    /// Whether a subscriber holding the closes through `from` may start.
    ///
    /// # Errors
    ///
    /// Code 1 naming both epochs for a `from` above the latest close, or unavailable when the owner stopped.
    pub async fn subscribe(&self, from: u64) -> Result<(), ErrorReply> {
        self.ask_or_refuse(|reply| SystemAsked::Subscribe { from, reply })
            .await
    }

    /// The durable close after `after`, once there is one.
    ///
    /// # Errors
    ///
    /// Code 1 when that close is no longer held, or unavailable when the owner stopped.
    pub async fn next_close(&self, after: u64) -> Result<Close, ErrorReply> {
        self.ask_or_refuse(|reply| SystemAsked::NextClose { after, reply })
            .await
    }

    /// The principal holding `name` and its verifier, `None` when none does or the owner stopped.
    pub async fn verifier(&self, name: String) -> Option<(u64, ScramVerifier)> {
        self.ask(|reply| SystemAsked::Verifier { name, reply })
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
        self.ask_or_refuse(|reply| SystemAsked::RegisterSession {
            session,
            principal,
            reply,
        })
        .await
    }

    /// The owner check of a commit through `session` by `principal`, which holds the session in flight until
    /// [`SystemHandle::commit_ended`].
    ///
    /// # Errors
    ///
    /// As [`SystemHandle::check_session`], and code 14 when a commit of the session is in flight.
    pub async fn begin_commit(&self, session: u128, principal: u64) -> Result<(), ErrorReply> {
        self.ask_or_refuse(|reply| SystemAsked::BeginCommit {
            session,
            principal,
            writes: true,
            reply,
        })
        .await
    }

    /// The owner check of a retried Commit's Recall, held in flight as [`SystemHandle::begin_commit`] holds a commit. A Recall
    /// records nothing in a partition's table, so a full registry may evict its session meanwhile (`PROTOCOL.md` §11.5).
    ///
    /// # Errors
    ///
    /// As [`SystemHandle::begin_commit`].
    pub async fn begin_recall(&self, session: u128, principal: u64) -> Result<(), ErrorReply> {
        self.ask_or_refuse(|reply| SystemAsked::BeginCommit {
            session,
            principal,
            writes: false,
            reply,
        })
        .await
    }

    /// The commit in flight through `session` was answered, or given up. An owner stopped holds none in flight.
    pub fn commit_ended(&self, session: u128) {
        drop(
            self.inbox
                .unbounded_send(SystemAsked::CommitEnded { session }),
        );
    }

    /// Whether the registry holds `session`; an owner stopped holds nothing.
    pub async fn session_held(&self, session: u128) -> bool {
        self.ask(|reply| SystemAsked::SessionHeld { session, reply })
            .await
            .unwrap_or(false)
    }

    /// What a request naming `tenant` needs to know.
    ///
    /// # Errors
    ///
    /// Unavailable when the owner stopped.
    pub async fn tenant_facts(&self, tenant: u64) -> Result<TenantFacts, ErrorReply> {
        self.ask(|reply| SystemAsked::TenantFacts { tenant, reply })
            .await
            .ok_or_else(stopped)
    }

    /// The placement map; `None` while the group holds none or the owner stopped.
    pub async fn map(&self) -> Option<PlacementMap> {
        self.ask(|reply| SystemAsked::Map { reply }).await.flatten()
    }

    /// `tenant`'s schema id, of its latest catalog version.
    ///
    /// # Errors
    ///
    /// Unavailable when the owner stopped, or a held version does not decode.
    pub async fn schema_id(&self, tenant: u64) -> Result<u64, ErrorReply> {
        self.ask_or_refuse(|reply| SystemAsked::SchemaId { tenant, reply })
            .await
    }

    /// What `tenant`'s catalog declares in effect at the latest close, and its metadata records.
    ///
    /// # Errors
    ///
    /// Unavailable when the owner stopped, or a held version does not decode.
    pub async fn describe(&self, tenant: u64) -> Result<Describe, ErrorReply> {
        self.ask_or_refuse(|reply| SystemAsked::Describe { tenant, reply })
            .await
    }

    /// A catalog version of `declared` built on `built_on`, committed as pending: the first version of its effective epoch, once
    /// the epoch is assigned and, for a version tightening a predicate, every partition holding the tenant has passed its check.
    ///
    /// # Errors
    ///
    /// Code 2 of kind Catalog for a version built on one no longer current or on one being checked; code 7, `detail` 16,777,216,
    /// for one past its cap; code 7 for declarations tightening and loosening at once, and for a change the data refuses;
    /// code 24 for a partition silent past `catalog_check_ms`.
    pub async fn declare(
        &self,
        tenant: u64,
        built_on: u64,
        declared: Vec<Declared>,
    ) -> Result<u64, ErrorReply> {
        self.declare_version(tenant, built_on, declared)
            .await
            .map(|(_, commit_version)| commit_version)
    }

    /// As [`SystemHandle::declare`], answering a Declare of `kind` 1 (`PROTOCOL.md` §13.27): the catalog version committed
    /// as pending, and the first version of its effective epoch.
    ///
    /// # Errors
    ///
    /// As [`SystemHandle::declare`], and code 7, `detail` the tenant, for a tenant the group does not hold.
    pub async fn declare_version(
        &self,
        tenant: u64,
        built_on: u64,
        declared: Vec<Declared>,
    ) -> Result<(u64, u64), ErrorReply> {
        self.ask_or_refuse(|reply| SystemAsked::Declare {
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
        self.ask_or_refuse(|reply| SystemAsked::Report { report, reply })
            .await
    }

    /// The catalog version a Drop calls for, pending and durable; none where it would change nothing.
    ///
    /// # Errors
    ///
    /// Code 7 naming what refused it, or unavailable when the owner stopped.
    pub async fn drop_catalog(&self, tenant: u64, step: DropStep) -> Result<(), ErrorReply> {
        self.drop_version(tenant, step).await.map(|_| ())
    }

    /// As [`SystemHandle::drop_catalog`], answering a Declare of `kind` 2 (`PROTOCOL.md` §13.27): the catalog version
    /// committed as pending, 0 when the Drop changes nothing in the catalog. A Drop of everything removes the tenant's
    /// default-entry records with it, durable before the answer.
    ///
    /// # Errors
    ///
    /// As [`SystemHandle::drop_catalog`], and code 7, `detail` the tenant, for a tenant the group does not hold.
    pub async fn drop_version(&self, tenant: u64, step: DropStep) -> Result<u64, ErrorReply> {
        self.ask_or_refuse(|reply| SystemAsked::DropCatalog {
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
        self.ask_or_refuse(|reply| SystemAsked::RecordDefault {
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
        self.ask_or_refuse(|reply| SystemAsked::CheckSession {
            session,
            principal,
            reply,
        })
        .await
    }

    /// A number for a new role connection, against which the marks its checks begin are held; `None` when the group
    /// stopped.
    pub async fn connect(&self) -> Option<u64> {
        self.ask(|reply| SystemAsked::Connect { reply }).await
    }

    /// The role connection `connection` closed: the group holds its marks for twice `transaction_ms` from now and then
    /// ends them (`PROTOCOL.md` §11.5). A group stopped holds no mark.
    pub fn disconnect(&self, connection: u64) {
        drop(
            self.inbox
                .unbounded_send(SystemAsked::Disconnect { connection }),
        );
    }

    /// A Session (`PROTOCOL.md` §13.23): `session` registered for `principal`, answered once durable. A full registry evicts
    /// its least recently registered session with no mark.
    ///
    /// # Errors
    ///
    /// Decided in this order: Error 1 naming the session when the registry holds it, for any principal; code 7 naming the
    /// principal when the group does not hold it; code 14 when the full registry holds no session it may evict, or must
    /// evict within twice `transaction_ms` of the group beginning to listen. Unavailable when the group stopped.
    pub async fn session(&self, session: u128, principal: u64) -> Result<(), ErrorReply> {
        self.ask_or_refuse(|reply| SystemAsked::Session {
            session,
            principal,
            reply,
        })
        .await
    }

    /// A Check's `checks` sent on `connection`, decided in their order (`PROTOCOL.md` §13.24): each with its outcome, 12,
    /// 21, 14 or 1 as section 11.5 orders them; a check of kind 2 that passes marks its session against `connection`.
    ///
    /// # Errors
    ///
    /// Unavailable when the group stopped.
    pub async fn check(
        &self,
        connection: u64,
        checks: Vec<SessionCheck>,
    ) -> Result<Vec<SessionCheck>, ErrorReply> {
        self.ask(|reply| SystemAsked::Check {
            connection,
            checks,
            reply,
        })
        .await
        .ok_or_else(stopped)
    }

    /// An Ended's `sessions` sent on `connection` (`PROTOCOL.md` §13.25): each mark begun on `connection`, or on a
    /// connection that closed, ends; any other is passed over. Nothing answers it.
    pub fn ended(&self, connection: u64, sessions: Vec<u128>) {
        drop(self.inbox.unbounded_send(SystemAsked::Ended {
            connection,
            sessions,
        }));
    }

    /// The predicates `tenant` holds default-entry records for, in the order of their texts' bytes, and the log position
    /// they were read at (`PROTOCOL.md` §13.28).
    ///
    /// # Errors
    ///
    /// Code 7, `detail` the tenant, for a tenant the group does not hold; unavailable when the group stopped.
    pub async fn defaults(&self, tenant: u64) -> Result<Defaults, ErrorReply> {
        self.ask_or_refuse(|reply| SystemAsked::Defaults { tenant, reply })
            .await
    }

    /// The catalog version of `tenant` a client may read at the closed epoch `epoch`: the latest in effect at it, leaving
    /// out a version being checked and one withdrawn; `epoch` 0 asks for the current version, pending or not
    /// (`PROTOCOL.md` §13.7).
    ///
    /// # Errors
    ///
    /// Error 1 naming both epochs for an `epoch` above the latest close; code 7, `detail` 0, when no such version is held;
    /// unavailable when the group stopped.
    pub async fn catalog_at(&self, tenant: u64, epoch: u64) -> Result<Catalog, ErrorReply> {
        self.ask_or_refuse(|reply| SystemAsked::CatalogAt {
            tenant,
            epoch,
            reply,
        })
        .await
    }

    /// The iteration count and salt a Challenge for `name` answers (`PROTOCOL.md` §13.8): the principal's, or for a name no
    /// principal holds a salt derived from the group's secret and the count it derives a principal's verifier with.
    ///
    /// # Errors
    ///
    /// Unavailable when the group stopped.
    pub async fn challenge(&self, name: String) -> Result<(u32, Vec<u8>), ErrorReply> {
        self.ask_or_refuse(|reply| SystemAsked::Challenge { name, reply })
            .await
    }

    /// What a Verify answers (`PROTOCOL.md` §13.9): the principal and the ServerSignature when `proof` holds for `name`
    /// over `auth_message`, `None` alike for a wrong proof and a name no principal holds.
    ///
    /// # Errors
    ///
    /// Unavailable when the group stopped.
    pub async fn verify(
        &self,
        name: String,
        auth_message: String,
        proof: [u8; 32],
    ) -> Result<Option<(u64, [u8; 32])>, ErrorReply> {
        self.ask_or_refuse(|reply| SystemAsked::Verify {
            name,
            auth_message,
            proof,
            reply,
        })
        .await
    }

    /// The latest close the sequencer holds durable, 0 for none: where a Subscribe of `latest` 1 begins (`PROTOCOL.md`
    /// §13.3).
    ///
    /// # Errors
    ///
    /// Unavailable when the group stopped.
    pub async fn latest_close(&self) -> Result<u64, ErrorReply> {
        self.ask(|reply| SystemAsked::LatestClose { reply })
            .await
            .ok_or_else(stopped)
    }
}

fn stopped() -> ErrorReply {
    ErrorReply::new(ErrorCode::Unavailable, "the frontend's owner stopped")
}
