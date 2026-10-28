/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_transport::ClientHandshake;
use ultragraphdb_protocol_wire::Welcome;

/// The connection as the core sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Link {
    Down,
    Handshake(ClientHandshake),
    Up(Welcome),
}
