/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{Command, ROLE_COMMANDS};

/// The endpoint a connection arrived at, which decides the requests it takes (`PROTOCOL.md` §11.12). Credit and
/// Cancel, like the handshake, belong to every endpoint and are not listed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Endpoint {
    /// The frontend's client endpoint: every client command and no role command.
    Client,
    /// The frontend's role address.
    Frontend,
    /// A partition's role address.
    Partition,
    /// A compute process's role address.
    Compute,
}

impl Endpoint {
    /// Whether the endpoint serves `command` as a request. Every other request is refused as `CommandNotServed`.
    #[must_use]
    pub fn serves(self, command: Command) -> bool {
        match self {
            Self::Client => !ROLE_COMMANDS.contains(&command.number()),
            Self::Frontend => matches!(
                command,
                Command::Join
                    | Command::Members
                    | Command::Placement
                    | Command::Catalog
                    | Command::Subscribe
                    | Command::Lease
                    | Command::CloseRequest
                    | Command::DefaultEntry
                    | Command::Report
            ),
            Self::Partition => matches!(
                command,
                Command::DirectWrite
                    | Command::ConditionalWrite
                    | Command::Bind
                    | Command::Write
                    | Command::Operation
                    | Command::Commit
                    | Command::Abort
                    | Command::Drop
                    | Command::Recall
                    | Command::Reference
                    | Command::Scan
                    | Command::Closed
            ),
            Self::Compute => command == Command::ComputeRead,
        }
    }

    /// Whether `command` arrives on the stream a Bind opened, and nowhere else: a partition takes no Commit or Drop
    /// on a stream of its own, which the client endpoint does.
    pub(crate) fn only_bound(self, command: Command) -> bool {
        self == Self::Partition && matches!(command, Command::Commit | Command::Drop)
    }
}
