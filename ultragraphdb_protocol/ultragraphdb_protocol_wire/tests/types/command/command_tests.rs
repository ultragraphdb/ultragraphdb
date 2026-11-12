/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::{ProtocolError, ProtocolErrorEnum};
use ultragraphdb_protocol_wire::{Command, REPLICA_COMMANDS, ROLE_COMMANDS};

/// The registry as assigned on 2026-10-27, in the order of the design note's body table, with Authenticate and Drop
/// added on 2026-10-30, the role commands 512 to 530 of `PROTOCOL.md` §3 on 2026-11-03, DirectWrite and
/// ConditionalWrite on 2026-11-05, and Query, Install and Held later that day. These numbers are never reused; a change here is a protocol break.
const REGISTRY: [(Command, u16); 45] = [
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
    (Command::DirectWrite, 20),
    (Command::ConditionalWrite, 21),
    (Command::Query, 22),
    (Command::Install, 23),
    // PROTOCOL.md §3: stage 6.6's Purge and Migrate.
    (Command::Purge, 24),
    (Command::Migrate, 25),
    (Command::Join, 512),
    (Command::Members, 513),
    (Command::Subscribe, 514),
    (Command::EpochClose, 515),
    (Command::Lease, 516),
    (Command::Placement, 517),
    (Command::Catalog, 518),
    (Command::Challenge, 519),
    (Command::Verify, 520),
    (Command::Bind, 521),
    (Command::Recall, 522),
    (Command::ComputeRead, 523),
    (Command::Scan, 524),
    (Command::Entries, 525),
    (Command::Reference, 526),
    (Command::CloseRequest, 527),
    (Command::Closed, 528),
    (Command::DefaultEntry, 529),
    (Command::Report, 530),
    (Command::Held, 531),
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
    let assigned: Vec<u16> = (1..=25).chain(512..=531).collect();
    assert_eq!(known, assigned);
    for number in (0..=u16::MAX).filter(|n| !assigned.contains(n)) {
        assert_eq!(
            Command::from_number(number).map_err(|e| e.0),
            Err(ProtocolErrorEnum::UnknownCommand { command: number })
        );
    }
}

#[test]
fn test_unassigned_numbers_are_unknown_commands() {
    for number in [0, 26, 255, 532, 767, 1_000, u16::MAX] {
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

#[test]
fn test_exactly_the_commands_numbered_512_to_767_are_role_commands() {
    for (command, number) in REGISTRY {
        assert_eq!(
            command.is_role(),
            (512..=767).contains(&number),
            "{command:?}"
        );
    }
    assert_eq!(ROLE_COMMANDS, 512..=767);
    assert!(!Command::Drop.is_role());
    assert!(Command::Join.is_role());
    assert!(Command::Report.is_role());
}

#[test]
fn test_a_client_connection_refuses_every_role_command_by_number() {
    for (command, number) in REGISTRY {
        let taken = command.on_client_connection();
        if (512..=767).contains(&number) {
            assert_eq!(
                taken.map_err(|e| e.0),
                Err(ProtocolErrorEnum::CommandNotServed { command: number })
            );
        } else {
            assert_eq!(taken, Ok(command));
        }
    }
    assert_eq!(
        Command::Lease.on_client_connection(),
        Err(ProtocolError::command_not_served(516))
    );
}

/// Section 3's "Answered by", each request with the commands that answer it.
const ANSWERS: [(Command, &[Command]); 34] = [
    (Command::Hello, &[Command::Welcome]),
    (Command::Register, &[Command::Register]),
    (Command::Begin, &[Command::Begin]),
    (Command::Commit, &[Command::Commit]),
    (Command::Read, &[Command::Result]),
    (Command::Write, &[Command::Held]),
    (Command::Operation, &[Command::Held]),
    (Command::Prepare, &[Command::Prepare]),
    (Command::Execute, &[Command::Result]),
    (Command::Describe, &[Command::Describe]),
    (Command::Authenticate, &[Command::Authenticate]),
    (Command::Drop, &[Command::Drop]),
    (Command::DirectWrite, &[Command::Commit]),
    (Command::ConditionalWrite, &[Command::Commit]),
    (Command::Query, &[Command::Result]),
    (Command::Install, &[Command::Install]),
    (Command::Purge, &[Command::Purge]),
    (Command::Migrate, &[Command::Migrate]),
    (Command::Join, &[Command::Join]),
    (Command::Members, &[Command::Members]),
    (Command::Subscribe, &[Command::EpochClose]),
    (Command::Lease, &[Command::Lease]),
    (Command::Placement, &[Command::Placement]),
    (Command::Catalog, &[Command::Catalog]),
    (Command::Challenge, &[Command::Challenge]),
    (Command::Verify, &[Command::Verify]),
    (Command::Bind, &[Command::Bind]),
    (Command::Recall, &[Command::Commit, Command::Drop]),
    (Command::ComputeRead, &[Command::Result]),
    (Command::Scan, &[Command::Entries]),
    (Command::Reference, &[Command::Reference]),
    (Command::Closed, &[Command::Closed]),
    (Command::DefaultEntry, &[Command::DefaultEntry]),
    (Command::Report, &[Command::Report]),
];

#[test]
fn test_each_request_is_answered_by_section_3s_commands() {
    for request in Command::ALL {
        let listed = ANSWERS
            .iter()
            .find(|(asked, _)| *asked == request)
            .map_or(&[][..], |(_, answers)| *answers);
        for answer in Command::ALL {
            assert_eq!(
                request.answered_by(answer),
                listed.contains(&answer),
                "{request:?} by {answer:?}"
            );
        }
    }
}

#[test]
fn test_an_error_answers_any_request_and_another_command_is_not_served() {
    for request in Command::ALL {
        assert_eq!(Command::Error.answering(request), Ok(Command::Error));
    }
    assert_eq!(Command::Held.answering(Command::Write), Ok(Command::Held));
    assert_eq!(
        Command::Held.answering(Command::Commit),
        Err(ProtocolError::command_not_served(531))
    );
}

/// PROTOCOL.md §11.3: Result and Entries frames, and the frames of a Describe's or a Catalog's answer, spend credit;
/// every other frame leaves it.
#[test]
fn test_result_entries_describe_and_catalog_frames_spend_credit() {
    let spending = [
        Command::Result,
        Command::Entries,
        Command::Describe,
        Command::Catalog,
    ];
    for (command, _) in REGISTRY {
        assert_eq!(
            command.spends_credit(),
            spending.contains(&command),
            "{command:?}"
        );
    }
}

/// PROTOCOL.md §3: Purge and Migrate are answered by themselves, "both" commands, and are no role commands.
#[test]
fn test_purge_and_migrate_answer_themselves() {
    for command in [Command::Purge, Command::Migrate] {
        assert!(command.answered_by(command), "{command:?}");
        assert!(!command.answered_by(Command::Result), "{command:?}");
        assert!(!command.is_role(), "{command:?}");
    }
}
