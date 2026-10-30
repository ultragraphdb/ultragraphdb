/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::{ProtocolError, ProtocolErrorEnum};
use ultragraphdb_protocol_wire::{Command, REPLICA_COMMANDS};

/// The registry as assigned on 2026-10-27, in the order of the design note's body table, with Authenticate and Drop
/// added on 2026-10-30. These numbers are never reused; a change here is a protocol break.
const REGISTRY: [(Command, u16); 19] = [
    (Command::Hello, 1),
    (Command::Welcome, 2),
    (Command::Register, 3),
    (Command::Begin, 4),
    (Command::Commit, 5),
    (Command::Abort, 6),
    (Command::Read, 7),
    (Command::Write, 8),
    (Command::Operation, 9),
    (Command::Prepare, 10),
    (Command::Execute, 11),
    (Command::Describe, 12),
    (Command::Result, 13),
    (Command::Credit, 14),
    (Command::Cancel, 15),
    (Command::Error, 16),
    (Command::Round, 17),
    (Command::Authenticate, 18),
    (Command::Drop, 19),
];

#[test]
fn test_each_command_keeps_its_number() {
    for (command, number) in REGISTRY {
        assert_eq!(command.number(), number, "{command:?}");
        assert_eq!(Command::from_number(number), Ok(command), "{number}");
    }
}

#[test]
fn test_all_lists_every_command_in_number_order() {
    let all: Vec<Command> = REGISTRY.iter().map(|(command, _)| *command).collect();
    assert_eq!(Command::ALL.to_vec(), all);
}

#[test]
fn test_exactly_the_registry_is_known() {
    // Every number outside the registry is unknown: a retired number leaves the registry with its variant, so this
    // covers RETIRED_COMMANDS (empty today, and asserted disjoint from the registry in constants_tests).
    let known: Vec<u16> = (0..=u16::MAX)
        .filter(|n| Command::from_number(*n).is_ok())
        .collect();
    assert_eq!(known, (1..=19).collect::<Vec<u16>>());
    for number in (0..=u16::MAX).filter(|n| !(1..=19).contains(n)) {
        assert_eq!(
            Command::from_number(number).map_err(|e| e.0),
            Err(ProtocolErrorEnum::UnknownCommand { command: number })
        );
    }
}

#[test]
fn test_unassigned_numbers_are_unknown_commands() {
    for number in [0, 20, 255, 512, 1_000, u16::MAX] {
        assert_eq!(
            Command::from_number(number),
            Err(ProtocolError(ProtocolErrorEnum::UnknownCommand {
                command: number
            }))
        );
    }
}

#[test]
fn test_replica_numbers_are_unknown_until_stage_10() {
    for number in REPLICA_COMMANDS {
        assert_eq!(
            Command::from_number(number).map_err(|e| e.0),
            Err(ProtocolErrorEnum::UnknownCommand { command: number })
        );
    }
}
