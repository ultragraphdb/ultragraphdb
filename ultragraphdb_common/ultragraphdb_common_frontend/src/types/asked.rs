/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use futures::channel::oneshot::Sender;
use ultragraphdb_cluster_sequencer::Close;
use ultragraphdb_protocol_scram::ScramVerifier;
use ultragraphdb_protocol_wire::{Catalog, ErrorReply, Join, Members, Placement};

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
        /// The namespace.
        namespace: u64,
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
}
