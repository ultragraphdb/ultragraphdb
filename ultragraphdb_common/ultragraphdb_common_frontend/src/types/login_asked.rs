/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::net::IpAddr;
use std::time::{Duration, Instant};

use futures::channel::oneshot::Sender;
use ultragraphdb_protocol_scram::ScramVerifier;

/// What a login asks the owner, or tells it, each with where its answer goes: the owner holds the failed authentications of every
/// endpoint core (design D15).
#[derive(Debug)]
pub enum LoginAsked {
    /// The principal holding `name` and its verifier, and how long the attempt from `source` waits first (design D15).
    Verifier {
        /// The SASLprep-prepared name.
        name: String,
        /// Where the attempt comes from; a Unix socket's peer has no address and waits nothing.
        source: Option<IpAddr>,
        /// Its answer.
        reply: Sender<(Option<(u64, ScramVerifier)>, Duration)>,
    },
    /// Whether a connection from `source` may go on at `at`: not while its source is banned.
    Admit {
        /// The connection's peer.
        source: IpAddr,
        /// When it was accepted.
        at: Instant,
        /// Its answer.
        reply: Sender<bool>,
    },
    /// An authentication from `source` ended at `at`, succeeded or failed. No answer.
    Outcome {
        /// Where it came from.
        source: IpAddr,
        /// When it ended.
        at: Instant,
        /// Whether it succeeded.
        succeeded: bool,
    },
}
