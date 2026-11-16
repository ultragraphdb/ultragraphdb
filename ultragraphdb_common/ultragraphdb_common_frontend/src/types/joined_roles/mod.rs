/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Instant;

use compio::runtime::spawn;
use futures::StreamExt;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use ultragraphdb_common_role::{RoleClient, RoleCluster};
use ultragraphdb_common_service::{Readiness, RoleKind as ServiceRole};
use ultragraphdb_protocol_transport::Address;
use ultragraphdb_protocol_wire::{Join, RoleKind};

use crate::types::join_fan_out::JoinFanOut;
use crate::types::role_links::RoleLinks;

/// How long a failed connection back waits before the next.
const RETRY: std::time::Duration = std::time::Duration::from_millis(100);

/// What the connection back to a role that joined needs: the cluster to dial it in, the core's links it joins, and the
/// frontend's readiness, which the first partition reached sets.
#[derive(Debug, Clone)]
struct Back {
    cluster: RoleCluster,
    links: RoleLinks,
    readiness: Readiness,
}

/// What the frontend does with each Join its role sessions registered with the owner, in the order they were told: the
/// core's [`RoleLinks`] learn it, each other endpoint core hears of it through a [`JoinFanOut`], and the frontend connects
/// back to the role (design D1). Group 5 drops it, once the frontend reads Members (design D9).
#[derive(Debug)]
pub struct JoinedRoles {
    back: Back,
    cores: JoinFanOut,
    joined: UnboundedReceiver<Join>,
}

impl JoinedRoles {
    /// What a core of `cluster` does with the joins its role sessions send on the sender this returns: `links` learn them,
    /// and the first partition reached sets `readiness`. The channel is unbounded, as `JoinFanOut`'s are: a role joins once
    /// per connection it opens, and task 5.5 removes both.
    #[must_use]
    pub fn new(
        cluster: RoleCluster,
        links: RoleLinks,
        readiness: Readiness,
    ) -> (Self, UnboundedSender<Join>) {
        let (joins, joined) = unbounded();
        let back = Back {
            cluster,
            links,
            readiness,
        };
        let roles = Self {
            back,
            cores: JoinFanOut::new(),
            joined,
        };
        (roles, joins)
    }

    /// What a core does with each join, also sent to the other endpoint cores over `cores`, so every core routes to what
    /// joined (design D14).
    #[must_use]
    pub fn with_cores(mut self, cores: JoinFanOut) -> Self {
        self.cores = cores;
        self
    }

    /// Takes every join to the core's links, to the other cores and to a connection back, until every sender is dropped. A
    /// future dropped at its wait takes no more, and leaves the joins before it as they were taken.
    pub async fn follow(mut self) {
        // An event loop: it ends once every sender is gone, as the frontend stops (Ultra rule 2).
        while let Some(join) = self.joined.next().await {
            self.back.links.joined(&join);
            self.cores.send(&join);
            spawn(connect_back(
                self.back.clone(),
                join.address().to_string(),
                join.kind(),
            ))
            .detach();
        }
    }
}

#[expect(
    clippy::disallowed_methods,
    reason = "a deadline in the shell, which no simulation replays"
)]
fn now() -> Instant {
    Instant::now()
}

/// Connects back to the role that joined at `text`, retrying until the deadline; the first partition reached makes the
/// frontend ready. A connection a later Join of the address brings replaces this one.
/// Spawned detached: a future dropped at the runtime's end leaves no connection back.
async fn connect_back(shared: Back, text: String, kind: RoleKind) {
    let Ok(address) = Address::parse(&text) else {
        tracing::warn!(address = %text, "a role joined at an address that does not parse");
        return;
    };
    let due = now() + shared.cluster.deadline();
    let name = ServiceRole::Frontend.to_string();
    // Retried every `RETRY` until the cluster's deadline, past which it gives up (Ultra rule 2).
    loop {
        let secret = shared.cluster.secret().password();
        match RoleClient::dial(&address, shared.cluster.id(), &name, secret).await {
            Ok(client) => {
                shared.links.connected(text, client);
                if kind == RoleKind::Storage {
                    shared.readiness.set(true);
                }
                return;
            }
            Err(error) if now() >= due => {
                tracing::warn!(address = %text, %error, "the frontend could not connect back");
                return;
            }
            Err(_) => compio::time::sleep(RETRY).await,
        }
    }
}
