/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The command table (`PROTOCOL.md` §3): a command's number is its own, and a number no command of this stage has is
//! refused as an unknown command.

use hegel::TestCase;
use hegel::generators as gs;
use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::Command;

/// The numbers §3's table assigns a command the crate reads at this stage: 1 to 25, the role commands 512 to 531, and
/// stage 5.8's 534, 535, 536, 538 and 539. 27, 532, 533, 537 and 540 are first assigned at later stages.
fn assigned(number: u16) -> bool {
    (1..=25).contains(&number)
        || (512..=531).contains(&number)
        || [534, 535, 536, 538, 539].contains(&number)
}

/// L1.26: the table reads a drawn number as a command exactly when §3 assigns it at this stage, gives the command its
/// number back, and reads every other number, 537 and 540 among them, as no command.
#[hegel::test]
fn test_a_number_is_a_command_exactly_when_the_table_assigns_it(tc: TestCase) {
    let near = tc.draw(gs::sampled_from(
        (510u16..=545).chain(0..=30).collect::<Vec<u16>>(),
    ));
    let number = if tc.draw(gs::booleans()) {
        near
    } else {
        tc.draw(gs::integers::<u16>())
    };
    let read = Command::from_number(number).map_err(|refusal| refusal.0);
    if assigned(number) {
        assert_eq!(read.as_ref().map(|command| command.number()), Ok(number));
        assert_eq!(
            read.map(Command::is_role),
            Ok((512..=767).contains(&number))
        );
    } else {
        assert_eq!(
            read,
            Err(ProtocolErrorEnum::UnknownCommand { command: number })
        );
    }
}

/// L1.26: stage 5.8's five numbers are their commands, and 537 and 540 are none.
#[hegel::test]
fn test_the_system_groups_numbers_are_their_commands(tc: TestCase) {
    let table = vec![
        (534u16, Some(Command::Session)),
        (535, Some(Command::Check)),
        (536, Some(Command::Ended)),
        (537, None),
        (538, Some(Command::Declare)),
        (539, Some(Command::Defaults)),
        (540, None),
    ];
    let (number, command) = tc.draw(gs::sampled_from(table));
    assert_eq!(Command::from_number(number).ok(), command);
    assert_eq!(command.map(Command::number), command.map(|_| number));
    assert_eq!(
        Command::ALL.iter().any(|known| known.number() == number),
        command.is_some()
    );
}
