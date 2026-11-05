/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::utils::constants::ROLE_COMMANDS;

/// What a frame's body is: the header's `command`, a closed registry of the protocol's commands.
///
/// A command's number is never reused for another meaning. Retiring a command removes its variant and adds its
/// number to [`crate::RETIRED_COMMANDS`], where it stays; a frame carrying it is then refused as an unknown command.
/// Number 0 is never assigned, so a zeroed header names no command, and [`crate::REPLICA_COMMANDS`] is reserved for
/// stage 8's replica commands. [`crate::ROLE_COMMANDS`] holds the role commands, which roles exchange and a client
/// never sends or receives (`PROTOCOL.md` §3, §13).
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
    /// A changeset committed as one transaction in one frame.
    DirectWrite,
    /// Commit-time operations, with any changes, committed as one transaction in one frame.
    ConditionalWrite,
    /// Role command 512: a storage or compute process registers with the frontend: its kind, role address and partitions.
    Join,
    /// Role command 513: the roles that joined: each storage partition's process address and each compute process's.
    Members,
    /// Role command 514: storage asks for the sequencer's epoch closes, from the last it holds.
    Subscribe,
    /// Role command 515: one of the sequencer's closes: its epoch, time, catalog announcements and session evictions.
    EpochClose,
    /// Role command 516: a partition leases a block of sequences of one type.
    Lease,
    /// Role command 517: the placement map in effect at an epoch.
    Placement,
    /// Role command 518: a namespace's catalog version, its bytes opaque.
    Catalog,
    /// Role command 519: the first half of verifying a principal's SCRAM proof: its salt and iteration count.
    Challenge,
    /// Role command 520: the second half: the proof checked against the principal's stored key.
    Verify,
    /// Role command 521: the frontend binds a read-write transaction, or a Drop, to a partition on a stream of its own.
    Bind,
    /// Role command 522: the frontend asks a partition for a session's stored reply.
    Recall,
    /// Role command 523: the frontend sends a client's read to compute, with what compute needs of its transaction.
    ComputeRead,
    /// Role command 524: storage's read kernel: key ranges of the partitions one process hosts, each at a version.
    Scan,
    /// Role command 525: a batch of one scanned range's entries, in key order.
    Entries,
    /// Role command 526: registrations of references at an entity's home, answered with the entity's ID and generation.
    Reference,
    /// Role command 527: a partition asks the sequencer to close its open epoch now.
    CloseRequest,
    /// Role command 528: compute asks the latest closed epoch of partitions a storage process hosts.
    Closed,
    /// Role command 529: a partition asks the metadata group to record an undeclared predicate's default entry.
    DefaultEntry,
    /// Role command 530: a partition reports a type change's check, or a failed index build, at a catalog version's epoch.
    Report,
}

impl Command {
    /// Every command, in the order of its number.
    pub const ALL: [Command; 40] = [
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
        Command::DirectWrite,
        Command::ConditionalWrite,
        Command::Join,
        Command::Members,
        Command::Subscribe,
        Command::EpochClose,
        Command::Lease,
        Command::Placement,
        Command::Catalog,
        Command::Challenge,
        Command::Verify,
        Command::Bind,
        Command::Recall,
        Command::ComputeRead,
        Command::Scan,
        Command::Entries,
        Command::Reference,
        Command::CloseRequest,
        Command::Closed,
        Command::DefaultEntry,
        Command::Report,
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
            Command::DirectWrite => 20,
            Command::ConditionalWrite => 21,
            Command::Join => 512,
            Command::Members => 513,
            Command::Subscribe => 514,
            Command::EpochClose => 515,
            Command::Lease => 516,
            Command::Placement => 517,
            Command::Catalog => 518,
            Command::Challenge => 519,
            Command::Verify => 520,
            Command::Bind => 521,
            Command::Recall => 522,
            Command::ComputeRead => 523,
            Command::Scan => 524,
            Command::Entries => 525,
            Command::Reference => 526,
            Command::CloseRequest => 527,
            Command::Closed => 528,
            Command::DefaultEntry => 529,
            Command::Report => 530,
        }
    }

    /// The command numbered `number`, refused as [`ProtocolError::unknown_command`] when no command of this protocol
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
            20 => Ok(Command::DirectWrite),
            21 => Ok(Command::ConditionalWrite),
            512 => Ok(Command::Join),
            513 => Ok(Command::Members),
            514 => Ok(Command::Subscribe),
            515 => Ok(Command::EpochClose),
            516 => Ok(Command::Lease),
            517 => Ok(Command::Placement),
            518 => Ok(Command::Catalog),
            519 => Ok(Command::Challenge),
            520 => Ok(Command::Verify),
            521 => Ok(Command::Bind),
            522 => Ok(Command::Recall),
            523 => Ok(Command::ComputeRead),
            524 => Ok(Command::Scan),
            525 => Ok(Command::Entries),
            526 => Ok(Command::Reference),
            527 => Ok(Command::CloseRequest),
            528 => Ok(Command::Closed),
            529 => Ok(Command::DefaultEntry),
            530 => Ok(Command::Report),
            _ => Err(ProtocolError::unknown_command(number)),
        }
    }

    /// Whether this is a role command, numbered in [`crate::ROLE_COMMANDS`]: one roles exchange on role connections,
    /// which a client endpoint refuses.
    pub fn is_role(self) -> bool {
        ROLE_COMMANDS.contains(&self.number())
    }

    /// The command as a client connection takes it: refused as [`ProtocolError::command_not_served`], naming its
    /// number, when it is a role command (`PROTOCOL.md` §2.5, step 3). The client endpoint applies it to every request,
    /// and a client to every frame the server sends.
    pub fn on_client_connection(self) -> Result<Command, ProtocolError> {
        if self.is_role() {
            return Err(ProtocolError::command_not_served(self.number()));
        }
        Ok(self)
    }
}
