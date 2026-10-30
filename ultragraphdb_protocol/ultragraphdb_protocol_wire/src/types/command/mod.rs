/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

/// What a frame's body is: the header's `command`, a closed registry of the protocol's commands.
///
/// A command's number is never reused for another meaning. Retiring a command removes its variant and adds its
/// number to [`crate::RETIRED_COMMANDS`], where it stays; a frame carrying it is then refused as an unknown command.
/// Number 0 is never assigned, so a zeroed header names no command, and [`crate::REPLICA_COMMANDS`] is reserved for
/// stage 10's replica commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Command {
    /// A client opens a connection: its protocol range and release.
    Hello,
    /// The server's answer to Hello: the chosen protocol and its limits.
    Welcome,
    /// A session is registered.
    Register,
    /// A transaction begins on a stream.
    Begin,
    /// A transaction commits, with its session and request number.
    Commit,
    /// A transaction aborts.
    Abort,
    /// A data request: a quad pattern, a limit and a resume key.
    Read,
    /// A changeset.
    Write,
    /// A commit-time operation.
    Operation,
    /// A statement is prepared.
    Prepare,
    /// A prepared statement is executed with its parameters.
    Execute,
    /// The data dictionary's answer.
    Describe,
    /// A columnar result batch.
    Result,
    /// More credit for a result stream.
    Credit,
    /// A stream is cancelled.
    Cancel,
    /// An error: a code, the stream and a message.
    Error,
    /// The log record of a round.
    Round,
    /// One message of a SASL exchange on stream 0.
    Authenticate,
    /// A predicate, a graph, a namespace's data, a namespace or every namespace dropped.
    Drop,
}

impl Command {
    /// Every command, in the order of its number.
    pub const ALL: [Command; 19] = [
        Command::Hello,
        Command::Welcome,
        Command::Register,
        Command::Begin,
        Command::Commit,
        Command::Abort,
        Command::Read,
        Command::Write,
        Command::Operation,
        Command::Prepare,
        Command::Execute,
        Command::Describe,
        Command::Result,
        Command::Credit,
        Command::Cancel,
        Command::Error,
        Command::Round,
        Command::Authenticate,
        Command::Drop,
    ];

    /// The command's number in the header.
    pub fn number(self) -> u16 {
        match self {
            Command::Hello => 1,
            Command::Welcome => 2,
            Command::Register => 3,
            Command::Begin => 4,
            Command::Commit => 5,
            Command::Abort => 6,
            Command::Read => 7,
            Command::Write => 8,
            Command::Operation => 9,
            Command::Prepare => 10,
            Command::Execute => 11,
            Command::Describe => 12,
            Command::Result => 13,
            Command::Credit => 14,
            Command::Cancel => 15,
            Command::Error => 16,
            Command::Round => 17,
            Command::Authenticate => 18,
            Command::Drop => 19,
        }
    }

    /// The command numbered `number`, refused as [`ProtocolError::UnknownCommand`] when no command of this protocol
    /// has it: never assigned, retired, or reserved for the replica commands.
    pub fn from_number(number: u16) -> Result<Command, ProtocolError> {
        match number {
            1 => Ok(Command::Hello),
            2 => Ok(Command::Welcome),
            3 => Ok(Command::Register),
            4 => Ok(Command::Begin),
            5 => Ok(Command::Commit),
            6 => Ok(Command::Abort),
            7 => Ok(Command::Read),
            8 => Ok(Command::Write),
            9 => Ok(Command::Operation),
            10 => Ok(Command::Prepare),
            11 => Ok(Command::Execute),
            12 => Ok(Command::Describe),
            13 => Ok(Command::Result),
            14 => Ok(Command::Credit),
            15 => Ok(Command::Cancel),
            16 => Ok(Command::Error),
            17 => Ok(Command::Round),
            18 => Ok(Command::Authenticate),
            19 => Ok(Command::Drop),
            _ => Err(ProtocolError::UnknownCommand(number)),
        }
    }
}
