/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use ultragraphdb_protocol_transport::Login;

use crate::types::client_core::ClientCore;

/// The core's state in brief: its configuration, principal, connection generation, and the streams, requests and
/// sessions it holds; the login, which may hold a password, is left out.
impl<L: Login> fmt::Debug for ClientCore<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClientCore")
            .field("config", &self.config)
            .field("principal", &self.principal)
            .field("generation", &self.generation)
            .field("streams", &self.streams.len())
            .field("pending", &self.pending.len())
            .field("sessions", &self.sessions)
            .finish()
    }
}
