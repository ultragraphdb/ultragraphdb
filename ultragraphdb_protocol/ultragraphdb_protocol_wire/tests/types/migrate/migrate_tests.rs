/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Migrate's builder (`PROTOCOL.md` §8.6) against frames laid out by hand: `tenant` at 0, `staged` at 8, `action` at
//! 16, `state` at 17, `refusal` at 20, `epoch` at 24; the message and the migration report.

use ultragraphdb_protocol_wire::{
    BodyBuilder, Command, FLAG_LAST, Header, Migrate, MigrateAction, MigrateState,
};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::golden_bodies::CLUSTER;
use crate::utils::ugs_frames::frame;

fn header() -> Header {
    Header::new(CLUSTER, Command::Migrate)
        .with_stream(3)
        .with_flags(FLAG_LAST)
}

/// A request asks an action of a staged namespace's run; the reply answers its state, epoch and report; a failed
/// run's, its refusal and message.
#[test]
fn test_a_request_and_its_replies_are_built() -> TestResult {
    let request = Migrate::new(2, 5, MigrateAction::DryRun);
    assert_eq!((request.tenant(), request.staged()), (2, 5));
    assert_eq!(request.action(), MigrateAction::DryRun);
    assert_eq!(
        (request.state(), request.refusal(), request.epoch()),
        (None, 0, 0)
    );
    assert_eq!((request.message(), request.report()), ("", ""));
    let expected = frame(
        Command::Migrate,
        &[
            (0, &2u64.to_le_bytes()),
            (8, &5u64.to_le_bytes()),
            (16, &[2]),
        ],
        &[vec![], vec![]],
        true,
    )?;
    assert_eq!(request.frame(header())?, expected);
    let copying = Migrate::new(2, 5, MigrateAction::Start).answered(MigrateState::Copying, 30, "r");
    assert_eq!(
        (copying.state(), copying.epoch(), copying.report()),
        (Some(MigrateState::Copying), 30, "r")
    );
    let expected = frame(
        Command::Migrate,
        &[
            (0, &2u64.to_le_bytes()),
            (8, &5u64.to_le_bytes()),
            (16, &[1, 2]),
            (24, &30u64.to_le_bytes()),
        ],
        &[vec![], b"r".to_vec()],
        true,
    )?;
    assert_eq!(copying.frame(header())?, expected);
    let failed = Migrate::new(2, 5, MigrateAction::ReadState)
        .answered(MigrateState::Failed, 31, "")
        .refused(2_205, "UGS2205 m");
    assert_eq!((failed.refusal(), failed.message()), (2_205, "UGS2205 m"));
    let expected = frame(
        Command::Migrate,
        &[
            (0, &2u64.to_le_bytes()),
            (8, &5u64.to_le_bytes()),
            (16, &[5, 9]),
            (20, &2_205u32.to_le_bytes()),
            (24, &31u64.to_le_bytes()),
        ],
        &[b"UGS2205 m".to_vec(), vec![]],
        true,
    )?;
    assert_eq!(failed.frame(header())?, expected);
    Ok(())
}
