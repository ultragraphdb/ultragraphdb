/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// What a role connection said of a stream a session opened on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkEvent {
    /// A frame on the stream, whole, to relay or read.
    Frame(Vec<u8>),
    /// The connection closed, or could not be opened, under the stream.
    Closed,
}
