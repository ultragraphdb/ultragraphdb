/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_client_core::Pending;
use ultragraphdb_protocol_transport::Login;

use crate::traits::connect::Connect;
use crate::types::client::Client;

/// A request whose answer the client waits for. Dropped, with the caller's future or once the answer is taken, it
/// tells the core the caller no longer waits ([`ultragraphdb_client_core::ClientCore::forget`]), so an answer no call
/// takes is let go as it arrives; one already taken is no longer held, and nothing changes.
pub(super) struct Settling<'a, C: Connect, L: Login> {
    pub client: &'a mut Client<C, L>,
    pub pending: Pending,
}

impl<C: Connect, L: Login> Drop for Settling<'_, C, L> {
    fn drop(&mut self) {
        self.client.core.forget(self.pending);
    }
}
