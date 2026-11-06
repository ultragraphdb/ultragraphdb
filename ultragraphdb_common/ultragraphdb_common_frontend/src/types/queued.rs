/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::counted_frame::CountedFrame;
use crate::types::destination::Destination;

/// A request of a transaction the frontend has not sent yet.
#[derive(Debug)]
pub(crate) enum Queued {
    /// A Write or Operation, its frame whole, counted on its way unless a read holds it back.
    Write {
        frame: CountedFrame,
        destination: Destination,
        /// The bytes it counts toward what a connection's reads hold back, when a read held it back.
        behind: u64,
    },
    /// A read, by its stream.
    Read(u64),
    /// A Commit, its frame whole, through this session.
    Commit { frame: Vec<u8>, session: u128 },
}
