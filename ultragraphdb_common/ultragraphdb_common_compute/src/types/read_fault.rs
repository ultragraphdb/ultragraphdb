/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// Why a frame's contents gave a read nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ReadFault {
    /// The frame breaks the wire's rules: the partition cannot be trusted with the connection's other streams, so it closes.
    Frame(String),
    /// What the store holds does not read: code 17 for this read alone, and the connection stays open (`PROTOCOL.md` §13.12).
    Stored(String),
}
