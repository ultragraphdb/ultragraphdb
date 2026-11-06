/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::destination::Destination;

/// A request of a transaction the frontend has not sent yet.
#[derive(Debug)]
pub(crate) enum Queued {
    /// A Write or Operation, its frame whole.
    Write {
        frame: Vec<u8>,
        destination: Destination,
        /// The bytes it counts toward `held_bytes`, when a read held it back.
        behind: u64,
    },
    /// A read, by its stream.
    Read(u64),
    /// A Commit, its frame whole, through this session.
    Commit { frame: Vec<u8>, session: u128 },
}
