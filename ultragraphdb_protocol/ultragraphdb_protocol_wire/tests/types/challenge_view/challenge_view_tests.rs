/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{BodyView, Challenge, ChallengeView};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::types::challenge::challenge_tests::SALT;
use crate::utils::golden_bodies::{
    frame_of, put, view_any, with_command, with_command_header, with_section,
};
use crate::utils::golden_roles::{golden_role, role_bytes};

fn bytes(name: &str) -> TestResult<Vec<u8>> {
    role_bytes(&golden_role(name)?)
}

fn refused(bytes: &[u8]) -> TestResult<ProtocolErrorEnum> {
    Ok(ChallengeView::from_frame(&frame_of(bytes)?)
        .err()
        .context("a refusal")?
        .0)
}

fn iterations(value: u64) -> ProtocolErrorEnum {
    ProtocolErrorEnum::FieldValue {
        command: 519,
        field: "iterations",
        value,
    }
}

#[test]
fn test_the_golden_challenges_read_back_to_their_bodies() -> TestResult {
    let request = bytes("challenge")?;
    let view = ChallengeView::from_frame(&frame_of(&request)?)?;
    assert_eq!(
        (view.name(), view.iterations(), view.salt()),
        ("user", 0, &[][..])
    );
    assert_eq!(view.to_body()?, Challenge::request("user"));
    let reply = bytes("challenge_reply")?;
    let view_2 = ChallengeView::from_frame(&frame_of(&reply)?)?;
    assert_eq!((view_2.iterations(), view_2.salt()), (4_096, &SALT[..]));
    assert_eq!(
        view_2.to_body()?,
        Challenge::request("user").with_salt(4_096, SALT.to_vec())
    );
    for frame in [request, reply] {
        view_any(&frame)??;
    }
    Ok(())
}

#[test]
fn test_the_unused_bytes_come_before_the_count() -> TestResult {
    let reply = bytes("challenge_reply")?;
    for at in [4usize, 127] {
        let bad = with_command_header(&reply, |h| {
            h[at] = 1;
            put(h, 0, 1, 4);
        });
        assert_eq!(
            refused(&bad)?,
            ProtocolErrorEnum::ReservedNotZero { offset: 128 + at }
        );
    }
    let other = with_command(&reply, 520);
    assert_eq!(
        refused(&other)?,
        ProtocolErrorEnum::WrongCommand {
            expected: 519,
            found: 520
        }
    );
    Ok(())
}

#[test]
fn test_a_reply_counts_4096_to_ten_million_iterations() -> TestResult {
    let reply = bytes("challenge_reply")?;
    for value in [1u64, 4_095, 10_000_001, u32::MAX.into()] {
        let bad = with_command_header(&reply, |h| put(h, 0, value, 4));
        assert_eq!(refused(&bad)?, iterations(value));
    }
    let most = with_command_header(&reply, |h| put(h, 0, 10_000_000, 4));
    assert_eq!(
        ChallengeView::from_frame(&frame_of(&most)?)?.iterations(),
        10_000_000
    );
    Ok(())
}

#[test]
fn test_a_request_holds_no_salt_and_a_reply_one() -> TestResult {
    let salted = with_command_header(&bytes("challenge_reply")?, |h| put(h, 0, 0, 4));
    assert_eq!(
        refused(&salted)?,
        ProtocolErrorEnum::CountMismatch {
            command: 519,
            field: "salt",
            count: 16,
            expected: 0
        }
    );
    let unsalted = with_command_header(&bytes("challenge")?, |h| put(h, 0, 4_096, 4));
    assert_eq!(
        refused(&unsalted)?,
        ProtocolErrorEnum::CountMismatch {
            command: 519,
            field: "salt",
            count: 0,
            expected: 1
        }
    );
    Ok(())
}

#[test]
fn test_the_name_is_utf8_and_checked_before_the_salt() -> TestResult {
    // A request whose name is not UTF-8 and which holds a salt: the name's refusal comes first.
    let bad = with_section(&bytes("challenge_reply")?, 2, 0, |name| name[1] = 0xFF);
    let bad_2 = with_command_header(&bad, |h| put(h, 0, 0, 4));
    assert_eq!(
        refused(&bad_2)?,
        ProtocolErrorEnum::NotUtf8 {
            command: 519,
            section: 0,
            offset: 1
        }
    );
    Ok(())
}
