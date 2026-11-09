/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use futures::StreamExt;
use futures::channel::mpsc::UnboundedReceiver;
use ultragraphdb_protocol_wire::Join;

use crate::types::role_links::RoleLinks;

/// One endpoint core's side of a [`JoinFanOut`](crate::JoinFanOut): the joins core 0 registered, in their order.
#[derive(Debug)]
pub struct CoreJoins {
    joined: UnboundedReceiver<Join>,
}

impl CoreJoins {
    pub(crate) fn new(joined: UnboundedReceiver<Join>) -> Self {
        Self { joined }
    }

    /// Applies every join to `links`, the core's own, until the fan-out is dropped. A future dropped at its wait applies no more,
    /// and leaves `links` as the joins before it left them.
    pub async fn follow(mut self, links: RoleLinks) {
        while let Some(join) = self.joined.next().await {
            links.joined(&join);
        }
    }
}
