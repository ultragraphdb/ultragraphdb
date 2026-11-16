/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use futures::channel::oneshot::Sender;
use ultragraphdb_cluster_placement::PlacementMap;
use ultragraphdb_cluster_sequencer::Close;
use ultragraphdb_protocol_scram::ScramVerifier;
use ultragraphdb_protocol_wire::{
    Catalog, Declared, Defaults, Describe, ErrorReply, Join, Members, Placement, SessionCheck,
};

use crate::types::report_asked::ReportAsked;
use crate::types::tenant_facts::TenantFacts;

/// The catalog version a Drop calls for (`PROTOCOL.md` §11.13).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DropStep {
    /// A predicate's declaration removed.
    Predicate(String),
    /// The catalog set to the initial catalog, and the tenant's default-entry records removed.
    Everything,
    /// The catalog kept: a Drop of a graph or of a tenant's data calls for no version.
    Kept,
}

/// A proof that holds: the principal, and the ServerSignature of the exchange (`PROTOCOL.md` §13.9).
pub type Verified = (u64, [u8; 32]);

/// What an endpoint core asks the owner, each with where its answer goes.
#[derive(Debug)]
pub enum SystemAsked {
    /// Register a Join.
    Join {
        /// The Join.
        join: Join,
        /// Its answer.
        reply: Sender<Result<(), ErrorReply>>,
    },
    /// Who joined.
    Members {
        /// Its answer.
        reply: Sender<Members>,
    },
    /// The placement map at `epoch`.
    Placement {
        /// The epoch; 0 for the latest.
        epoch: u64,
        /// Its answer.
        reply: Sender<Result<Placement, ErrorReply>>,
    },
    /// A catalog version.
    Catalog {
        /// The tenant.
        tenant: u64,
        /// The version; 0 for the current one.
        version: u64,
        /// Its answer.
        reply: Sender<Result<Catalog, ErrorReply>>,
    },
    /// Lease a block of sequences: its first sequence, once durable.
    Lease {
        /// The type prefix.
        prefix: u16,
        /// The asking partition.
        partition: u32,
        /// The sequences asked for.
        size: u64,
        /// Its answer.
        reply: Sender<Result<u64, ErrorReply>>,
    },
    /// Close `epoch` as soon as the sequencer may.
    CloseRequest {
        /// The epoch.
        epoch: u64,
        /// Its answer.
        reply: Sender<Result<(), ErrorReply>>,
    },
    /// Whether a subscriber may start after `from`.
    Subscribe {
        /// The last close the subscriber holds.
        from: u64,
        /// Its answer.
        reply: Sender<Result<(), ErrorReply>>,
    },
    /// The durable close after `after`, answered when there is one.
    NextClose {
        /// The last close the subscriber holds.
        after: u64,
        /// Its answer.
        reply: Sender<Result<Close, ErrorReply>>,
    },
    /// The principal holding `name` and its verifier.
    Verifier {
        /// The SASLprep-prepared name.
        name: String,
        /// Its answer.
        reply: Sender<Option<(u64, ScramVerifier)>>,
    },
    /// Register a session for a principal, once durable.
    RegisterSession {
        /// The session.
        session: u128,
        /// The principal.
        principal: u64,
        /// Its answer.
        reply: Sender<Result<(), ErrorReply>>,
    },
    /// The owner check: whether `principal` may act through `session`.
    CheckSession {
        /// The session.
        session: u128,
        /// The connection's principal.
        principal: u64,
        /// Its answer.
        reply: Sender<Result<(), ErrorReply>>,
    },
    /// The owner check of a commit, which holds the session in flight until [`SystemAsked::CommitEnded`].
    BeginCommit {
        /// The session.
        session: u128,
        /// The connection's principal.
        principal: u64,
        /// Whether what it starts may record the session in a partition's table, as a Commit, a Drop and a one-frame write
        /// do and a Recall does not: a full registry evicts no such session.
        writes: bool,
        /// Its answer.
        reply: Sender<Result<(), ErrorReply>>,
    },
    /// The commit in flight through `session` was answered, or given up.
    CommitEnded {
        /// The session.
        session: u128,
    },
    /// Whether the registry holds `session`.
    SessionHeld {
        /// The session.
        session: u128,
        /// Its answer.
        reply: Sender<bool>,
    },
    /// What a request naming `tenant` needs to know.
    TenantFacts {
        /// The tenant.
        tenant: u64,
        /// Its answer.
        reply: Sender<TenantFacts>,
    },
    /// The placement map.
    Map {
        /// Its answer; `None` while the group holds none.
        reply: Sender<Option<PlacementMap>>,
    },
    /// `tenant`'s schema id.
    SchemaId {
        /// The tenant.
        tenant: u64,
        /// Its answer.
        reply: Sender<Result<u64, ErrorReply>>,
    },
    /// What `tenant`'s catalog declares and its metadata records.
    Describe {
        /// The tenant.
        tenant: u64,
        /// Its answer.
        reply: Sender<Result<Describe, ErrorReply>>,
    },
    /// A catalog version of `declared` built on `built_on`, answered with the version and its effective epoch's first
    /// version.
    Declare {
        /// The tenant.
        tenant: u64,
        /// The version it builds on.
        built_on: u64,
        /// What it declares.
        declared: Vec<Declared>,
        /// Its answer.
        reply: Sender<Result<(u64, u64), ErrorReply>>,
    },
    /// The catalog version a Drop calls for, pending and durable, or none where it would change nothing: its number, 0
    /// for none.
    DropCatalog {
        /// The tenant.
        tenant: u64,
        /// What the Drop removed.
        step: DropStep,
        /// Its answer.
        reply: Sender<Result<u64, ErrorReply>>,
    },
    /// A number for a new role connection, against which the marks its checks begin are held.
    Connect {
        /// Its answer.
        reply: Sender<u64>,
    },
    /// The role connection `connection` closed: its marks are held for twice `transaction_ms` from now.
    Disconnect {
        /// The connection.
        connection: u64,
    },
    /// A Session: `session` registered for `principal`, once durable (`PROTOCOL.md` §13.23).
    Session {
        /// The session.
        session: u128,
        /// The principal.
        principal: u64,
        /// Its answer.
        reply: Sender<Result<(), ErrorReply>>,
    },
    /// A Check's checks, decided in their order on `connection` (`PROTOCOL.md` §13.24).
    Check {
        /// The connection sending it.
        connection: u64,
        /// The checks.
        checks: Vec<SessionCheck>,
        /// The checks, each with its outcome.
        reply: Sender<Vec<SessionCheck>>,
    },
    /// An Ended's sessions, sent on `connection` (`PROTOCOL.md` §13.25).
    Ended {
        /// The connection sending it.
        connection: u64,
        /// The sessions.
        sessions: Vec<u128>,
    },
    /// The predicates `tenant` holds default-entry records for (`PROTOCOL.md` §13.28).
    Defaults {
        /// The tenant.
        tenant: u64,
        /// Its answer.
        reply: Sender<Result<Defaults, ErrorReply>>,
    },
    /// The catalog version a client may read at the closed epoch `epoch` (`PROTOCOL.md` §13.7).
    CatalogAt {
        /// The tenant.
        tenant: u64,
        /// The epoch.
        epoch: u64,
        /// Its answer.
        reply: Sender<Result<Catalog, ErrorReply>>,
    },
    /// The salt and iteration count of the principal `name`, or an unknown name's (`PROTOCOL.md` §13.8).
    Challenge {
        /// The SASLprep-prepared name.
        name: String,
        /// Its answer: the iteration count and the salt.
        reply: Sender<Result<(u32, Vec<u8>), ErrorReply>>,
    },
    /// Whether `proof` holds for `name` over `auth_message` (`PROTOCOL.md` §13.9).
    Verify {
        /// The SASLprep-prepared name.
        name: String,
        /// The exchange's AuthMessage.
        auth_message: String,
        /// The client's proof.
        proof: [u8; 32],
        /// Its answer: the principal and the ServerSignature when the proof holds.
        reply: Sender<Result<Option<Verified>, ErrorReply>>,
    },
    /// The latest close the sequencer holds durable, 0 for none.
    LatestClose {
        /// Its answer.
        reply: Sender<u64>,
    },
    /// A partition's report on a catalog version, answered once the group holds it.
    Report {
        /// The report.
        report: ReportAsked,
        /// Its answer.
        reply: Sender<Result<(), ErrorReply>>,
    },
    /// The default entry of `predicate`, recorded durably, and whether it was held already.
    RecordDefault {
        /// The tenant.
        tenant: u64,
        /// The predicate's IRI.
        predicate: String,
        /// Its answer.
        reply: Sender<Result<bool, ErrorReply>>,
    },
}
