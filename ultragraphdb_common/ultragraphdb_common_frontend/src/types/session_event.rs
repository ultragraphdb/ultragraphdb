/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_placement::PlacementMap;
use ultragraphdb_protocol_wire::ErrorReply;

use crate::types::link_event::LinkEvent;
use crate::types::route_key::RouteKey;

/// What reaches a client session from outside its connection: the one way into it, so nothing it holds is shared.
#[derive(Debug)]
pub enum SessionEvent {
    /// A role connection said `event` of the stream `key`.
    Role {
        /// The stream.
        key: RouteKey,
        /// What it said.
        event: LinkEvent,
    },
    /// The owner's placement map, asked for after a refusal naming another owner: what the request of `stream` waits for.
    Map {
        /// The client's stream.
        stream: u64,
        /// The map; `None` when the owner stopped or holds none.
        map: Option<PlacementMap>,
    },
    /// The catalog version a declaring Commit of `stream` asked for took effect, or was refused.
    Declared {
        /// The client's stream.
        stream: u64,
        /// The commit version answering it, or the refusal.
        outcome: Result<u64, ErrorReply>,
    },
    /// The catalog step of a Drop of `stream` is durable, or was refused; its reply follows.
    DropCatalog {
        /// The client's stream.
        stream: u64,
        /// The refusal, if any.
        outcome: Result<(), ErrorReply>,
    },
    /// Whether the registry still holds the session of the retried Commit of `stream`.
    SessionHeld {
        /// The client's stream.
        stream: u64,
        /// Whether it does.
        held: bool,
    },
}
