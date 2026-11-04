/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::Command;

/// How an endpoint's stream rules (`PROTOCOL.md` §11.2) take a command arriving on a stream other than 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RequestKind {
    /// Credit for a result stream.
    Credit,
    /// A stream cancelled.
    Cancel,
    /// Begin or Bind, which opens a transaction's stream.
    Begin,
    /// Register, Prepare, Describe, Drop or a role command, each opening a stream of its own.
    Open,
    /// Read or Execute, opening a stream for the transaction it names.
    Read,
    /// Write, Operation or Abort, on a transaction's stream.
    InTransaction,
    /// Commit, on a transaction's stream or a stream of its own.
    Commit,
    /// A command no client sends on a stream: Hello, Welcome, Result, Error, Round, Authenticate.
    Unexpected,
}

impl RequestKind {
    /// The kind of `command`.
    pub(crate) fn of(command: Command) -> Self {
        match command {
            Command::Credit => Self::Credit,
            Command::Cancel => Self::Cancel,
            Command::Begin | Command::Bind => Self::Begin,
            Command::Read | Command::Execute => Self::Read,
            Command::Write | Command::Operation | Command::Abort => Self::InTransaction,
            Command::Commit => Self::Commit,
            Command::Register
            | Command::Prepare
            | Command::Describe
            | Command::Drop
            | Command::Join
            | Command::Members
            | Command::Subscribe
            | Command::EpochClose
            | Command::Lease
            | Command::Placement
            | Command::Catalog
            | Command::Challenge
            | Command::Verify
            | Command::Recall
            | Command::ComputeRead
            | Command::Scan
            | Command::Entries
            | Command::Reference
            | Command::CloseRequest
            | Command::Closed
            | Command::DefaultEntry
            | Command::Report => Self::Open,
            Command::Hello
            | Command::Welcome
            | Command::Result
            | Command::Error
            | Command::Round
            | Command::Authenticate => Self::Unexpected,
        }
    }
}
