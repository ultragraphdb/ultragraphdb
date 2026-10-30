/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_transport::{ClientAuthentication, ClientHandshake};
use ultragraphdb_protocol_wire::Welcome;

/// The connection as the core sees it: closed, its handshake, its first Authenticate exchange by a mechanism `M`
/// after the Welcome, and up.
#[derive(Debug, Clone)]
pub(crate) enum Link<M> {
    Down,
    Handshake(ClientHandshake),
    Authenticating(Welcome, ClientAuthentication<M>),
    Up(Welcome),
}
