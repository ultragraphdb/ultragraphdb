/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use futures::channel::oneshot::Sender;
use ultragraphdb_cluster_placement::PlacementMap;
use ultragraphdb_cluster_sequencer::Close;
use ultragraphdb_protocol_scram::ScramVerifier;
use ultragraphdb_protocol_wire::{
    Catalog, Declared, Describe, ErrorReply, Join, Members, Placement,
};

use crate::types::tenant_facts::TenantFacts;

/// The catalog version a Drop calls for (`PROTOCOL.md` §11.13).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DropStep {
    /// A predicate's declaration removed.
    Predicate(String),
    /// The catalog set to the initial catalog.
    Everything,
}

/// What an endpoint core asks the owner, each with where its answer goes.
#[derive(Debug)]
pub enum Asked {
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
    /// The principal holding `name`, and its verifier.
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
    /// The owner check of a commit, which holds the session in flight until [`Asked::CommitEnded`].
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
    /// A catalog version of `declared` built on `built_on`, answered with its effective epoch's first version.
    Declare {
        /// The tenant.
        tenant: u64,
        /// The version it builds on.
        built_on: u64,
        /// What it declares.
        declared: Vec<Declared>,
        /// Its answer.
        reply: Sender<Result<u64, ErrorReply>>,
    },
    /// The catalog version a Drop calls for, pending and durable, or none where it would change nothing.
    DropCatalog {
        /// The tenant.
        tenant: u64,
        /// What the Drop removed.
        step: DropStep,
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
