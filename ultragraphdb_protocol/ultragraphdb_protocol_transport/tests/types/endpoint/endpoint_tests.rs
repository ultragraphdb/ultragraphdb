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
        served(Endpoint::Frontend),
        [
            Command::Join,
            Command::Members,
            Command::Subscribe,
            Command::Lease,
            Command::Placement,
            Command::Catalog,
            Command::CloseRequest,
            Command::DefaultEntry,
            Command::Report,
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

/// The client endpoint and a partition's role address take DirectWrite and ConditionalWrite, the frontend relaying them,
/// and no other endpoint does.
#[test]
fn test_a_write_in_one_frame_is_served_by_the_client_endpoint_and_a_partition() {
    for command in [Command::DirectWrite, Command::ConditionalWrite] {
        assert!(Endpoint::Client.serves(command), "{command:?}");
        assert!(Endpoint::Partition.serves(command), "{command:?}");
        assert!(!Endpoint::Frontend.serves(command), "{command:?}");
        assert!(!Endpoint::Compute.serves(command), "{command:?}");
    }
}

/// Challenge and Verify are served by no endpoint in this stage.
#[test]
fn test_challenge_and_verify_are_served_nowhere() {
    for endpoint in [
        Endpoint::Client,
        Endpoint::Frontend,
        Endpoint::Partition,
        Endpoint::Compute,
    ] {
        assert!(!endpoint.serves(Command::Challenge), "{endpoint:?}");
        assert!(!endpoint.serves(Command::Verify), "{endpoint:?}");
    }
}
