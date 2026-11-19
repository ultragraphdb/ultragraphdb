/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_transport::Endpoint;
use ultragraphdb_protocol_wire::{Command, ROLE_COMMANDS};

/// The commands `endpoint` serves, of every command there is.
fn served(endpoint: Endpoint) -> Vec<Command> {
    Command::ALL
        .into_iter()
        .filter(|command| endpoint.serves(*command))
        .collect()
}

/// The client endpoint serves no role command, and every role endpoint serves nothing else but a partition's
/// transaction commands.
#[test]
fn test_the_client_endpoint_serves_no_role_command() {
    assert!(
        served(Endpoint::Client)
            .iter()
            .all(|command| !ROLE_COMMANDS.contains(&command.number()))
    );
    assert!(served(Endpoint::Client).contains(&Command::Begin));
}

/// The table of `PROTOCOL.md` §11.12, each endpoint's list exactly.
#[test]
fn test_each_role_endpoint_serves_its_row_of_the_table() {
    assert_eq!(
        served(Endpoint::System),
        [
            Command::Join,
            Command::Members,
            Command::Subscribe,
            Command::Lease,
            Command::Placement,
            Command::Catalog,
            Command::Challenge,
            Command::Verify,
            Command::CloseRequest,
            Command::DefaultEntry,
            Command::Report,
            Command::Session,
            Command::Check,
            Command::Ended,
            Command::Declare,
            Command::Defaults,
        ]
    );
    assert_eq!(served(Endpoint::Compute), [Command::ComputeRead]);
    let partition = served(Endpoint::Partition);
    for command in [
        Command::DirectWrite,
        Command::ConditionalWrite,
        Command::Bind,
        Command::Write,
        Command::Operation,
        Command::Commit,
        Command::Abort,
        Command::Drop,
        Command::Recall,
        Command::Reference,
        Command::Scan,
        Command::Closed,
    ] {
        assert!(partition.contains(&command), "{command:?}");
    }
    assert_eq!(partition.len(), 12);
}

/// The client endpoint and a partition's role address take DirectWrite and ConditionalWrite, compute relaying them,
/// and no other endpoint does.
#[test]
fn test_a_write_in_one_frame_is_served_by_the_client_endpoint_and_a_partition() {
    for command in [Command::DirectWrite, Command::ConditionalWrite] {
        assert!(Endpoint::Client.serves(command), "{command:?}");
        assert!(Endpoint::Partition.serves(command), "{command:?}");
        assert!(!Endpoint::System.serves(command), "{command:?}");
        assert!(!Endpoint::Compute.serves(command), "{command:?}");
    }
}

/// The system group's commands, Challenge, Verify, Session, Check, Ended, Declare and Defaults, are served at the system group's
/// role address alone, where the storage process hosting the group listens in this stage (`PROTOCOL.md` §11.12).
#[test]
fn test_the_system_groups_commands_are_served_at_its_role_address_alone() {
    for command in [
        Command::Challenge,
        Command::Verify,
        Command::Session,
        Command::Check,
        Command::Ended,
        Command::Declare,
        Command::Defaults,
    ] {
        assert!(Endpoint::System.serves(command), "{command:?}");
        for endpoint in [Endpoint::Client, Endpoint::Partition, Endpoint::Compute] {
            assert!(!endpoint.serves(command), "{endpoint:?} {command:?}");
        }
    }
}
