/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::net::IpAddr;
use std::time::{Duration, Instant};

use futures::channel::oneshot::Sender;

/// What a login asks the throttle, or tells it, each with where its answer goes: the first endpoint core holds the failed
/// authentications of every endpoint core (design D15).
#[derive(Debug)]
pub enum LoginAsked {
    /// How long an attempt from `source` waits before its exchange (design D15).
    Delay {
        /// Where the attempt comes from.
        source: IpAddr,
        /// Its answer.
        reply: Sender<Duration>,
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
