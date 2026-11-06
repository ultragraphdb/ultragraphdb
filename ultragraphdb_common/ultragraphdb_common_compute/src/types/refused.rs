/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::ErrorReply;

/// Why a frame gave nothing.
pub(super) enum Refused {
    /// The partition refused the Scan.
    Partition(ErrorReply),
    /// Compute refuses the frame.
    Frame(String),
    /// The frame's entries hold what the store cannot have written.
    Stored(String),
}
