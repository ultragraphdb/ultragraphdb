/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{BodyView, CommitView, OperationResult};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::types::commit::commit_tests::{golden_commit, session};
use crate::utils::golden_bodies::{frame_of, golden, put, reframe, view_any, with_command_header};

/// The golden reply's body: the table (24 bytes), the results at 24 (48 bytes), the term record at 72, the text at 96.
const RESULTS: usize = 24;

fn refusal(bytes: &[u8]) -> TestResult<ProtocolErrorEnum> {
    Ok(CommitView::from_frame(&frame_of(bytes)?)
        .err()
        .context("a refusal")?
        .0)
}

#[test]
fn test_the_golden_commit_reads_its_values() -> TestResult {
    let bytes = golden("commit")?.bytes()?;
    let view = CommitView::from_frame(&frame_of(&bytes)?)?;
    assert_eq!(
        (view.session(), view.request(), view.commit_version()),
        (session(), 7, (30 << 22) | 100)
    );
    let token = view.token().context("a token")?;
    assert_eq!(
        (token.partition(), token.position(), token.epoch().get()),
        (2, 41, 30)
    );
    assert_eq!(view.result_count(), 3);
    assert_eq!(view.result(0), Some(OperationResult::for_node(0, false)));
    assert_eq!(view.result(1), Some(OperationResult::counter(-5)));
    assert_eq!(view.result(2), Some(OperationResult::compare_and_set()));
    assert_eq!(view.result(3), None);
    assert_eq!(view.node(0), Some("http://ex.org/x"));
    assert_eq!(view.node(1), None);
    assert_eq!(view.nodes().len(), 1);
    assert_eq!(view.to_body()?, golden_commit()?);
    Ok(())
}

#[test]
fn test_the_golden_request_reads_empty() -> TestResult {
    let bytes = golden("commit_request")?.bytes()?;
    let view = CommitView::from_frame(&frame_of(&bytes)?)?;
    assert_eq!((view.commit_version(), view.token()), (0, None));
    assert_eq!((view.result_count(), view.nodes().len()), (0, 0));
    Ok(())
}

fn request_refused(value: u64) -> ProtocolErrorEnum {
    ProtocolErrorEnum::FieldValue {
        command: 5,
        field: "request",
        value,
    }
}

#[test]
fn test_a_session_with_request_0_is_refused() -> TestResult {
    for name in ["commit", "commit_request"] {
        let bytes = with_command_header(&golden(name)?.bytes()?, |h| h[16..24].fill(0));
        assert_eq!(refusal(&bytes)?, request_refused(0), "{name}");
    }
    Ok(())
}

#[test]
fn test_no_session_with_a_request_is_refused() -> TestResult {
    let golden_request = golden("commit_request")?;
    let bytes = with_command_header(&golden_request.bytes()?, |h| h[..16].fill(0));
    let request = CommitView::from_frame(&frame_of(&golden_request.bytes()?)?)?.request();
    assert_eq!(refusal(&bytes)?, request_refused(request));
    Ok(())
}

#[test]
fn test_no_session_with_request_0_is_read() -> TestResult {
    let bytes = with_command_header(&golden("commit_request")?.bytes()?, |h| h[..24].fill(0));
    let view = CommitView::from_frame(&frame_of(&bytes)?)?;
    assert_eq!((view.session(), view.request()), ([0; 16], 0));
    assert_eq!(view_any(&bytes)?, Ok(()));
    Ok(())
}

/// `request` comes after the unused bytes and before the token.
#[test]
fn test_the_request_lies_between_the_unused_bytes_and_the_token() -> TestResult {
    let no_request = with_command_header(&golden("commit")?.bytes()?, |h| h[16..24].fill(0));
    let unused = with_command_header(&no_request, |h| h[56] = 1);
    assert_eq!(
        refusal(&unused)?,
        ProtocolErrorEnum::ReservedNotZero { offset: 128 + 56 }
    );
    let token = with_command_header(&no_request, |h| h[32..36].fill(0));
    assert_eq!(refusal(&token)?, request_refused(0));
    Ok(())
}

#[test]
fn test_the_token_follows_the_commit_version() -> TestResult {
    let reply = golden("commit")?.bytes()?;
    // A reply's token names the commit version's epoch: 31 is not (30 << 22) | 100's.
    let wrong = with_command_header(&reply, |h| put(h, 48, 31, 8));
    assert_eq!(
        refusal(&wrong)?,
        ProtocolErrorEnum::FieldValue {
            command: 5,
            field: "token",
            value: 31
        }
    );
    // A reply without a token.
    let none = with_command_header(&reply, |h| h[32..56].fill(0));
    assert_eq!(
        refusal(&none)?,
        ProtocolErrorEnum::FieldValue {
            command: 5,
            field: "token",
            value: 0
        }
    );
    // A request (commit version 0) with a token.
    let request = golden("commit_request")?.bytes()?;
    let token = with_command_header(&request, |h| {
        put(h, 32, 2, 4);
        put(h, 40, 41, 8);
        put(h, 48, 30, 8);
    });
    assert_eq!(
        refusal(&token)?,
        ProtocolErrorEnum::FieldValue {
            command: 5,
            field: "token",
            value: 30
        }
    );
    Ok(())
}

#[test]
fn test_a_token_is_checked_byte_by_byte() -> TestResult {
    let reply = golden("commit")?.bytes()?;
    // Token bytes 4 to 7, header bytes 36 to 39, are zero.
    for at in [36usize, 39] {
        let frame = with_command_header(&reply, |h| h[at] = 1);
        assert_eq!(
            refusal(&frame)?,
            ProtocolErrorEnum::ReservedNotZero { offset: 128 + at }
        );
    }
    // Partition 0, position 0, an epoch past 2^42 - 1: each with the other fields set.
    for (at, width, value, named) in [(32, 4, 0, 0), (40, 8, 0, 0), (48, 8, 1 << 42, 1 << 42)] {
        let frame = with_command_header(&reply, |h| put(h, at, value, width));
        assert_eq!(
            refusal(&frame)?,
            ProtocolErrorEnum::FieldValue {
                command: 5,
                field: "token",
                value: named
            }
        );
    }
    // Bytes 56 to 127 are unused, checked before the token.
    let unused = with_command_header(&reply, |h| {
        h[56] = 1;
        h[36] = 1;
    });
    assert_eq!(
        refusal(&unused)?,
        ProtocolErrorEnum::ReservedNotZero { offset: 184 }
    );
    Ok(())
}

#[test]
fn test_a_request_carries_no_results_and_no_nodes() -> TestResult {
    // The golden reply with commit version 0 and no token: a request holding three results and one node.
    let reply = golden("commit")?.bytes()?;
    let request = with_command_header(&reply, |h| h[24..56].fill(0));
    assert_eq!(
        refusal(&request)?,
        ProtocolErrorEnum::CountMismatch {
            command: 5,
            field: "results",
            count: 4,
            expected: 0
        }
    );
    Ok(())
}

#[test]
fn test_every_node_term_is_an_iri() -> TestResult {
    let bytes = golden("commit")?.bytes()?;
    let mut data = golden("commit")?.data()?;
    // The record's kind byte, at 72: a blank node (2) keeps the table well formed.
    data[72] = 2;
    assert_eq!(
        refusal(&reframe(&bytes, &data))?,
        ProtocolErrorEnum::FieldValue {
            command: 5,
            field: "node term",
            value: 2
        }
    );
    Ok(())
}

#[test]
fn test_results_are_whole_records_with_a_flag_and_zeros() -> TestResult {
    let bytes = golden("commit")?.bytes()?;
    let data = golden("commit")?.data()?;
    // Result 0's `created`, its byte 12: 2 is no boolean.
    let mut flag = data.clone();
    flag[RESULTS + 12] = 2;
    assert_eq!(
        refusal(&reframe(&bytes, &flag))?,
        ProtocolErrorEnum::FieldValue {
            command: 5,
            field: "created",
            value: 2
        }
    );
    // Result 1's last zero byte, its byte 15.
    let mut zeros = data.clone();
    zeros[RESULTS + 16 + 15] = 1;
    assert_eq!(
        refusal(&reframe(&bytes, &zeros))?,
        ProtocolErrorEnum::PaddingNotZero {
            command: 5,
            offset: RESULTS + 16 + 15
        }
    );
    Ok(())
}

#[test]
fn test_a_commit_result_names_a_node_or_a_value() -> TestResult {
    let bytes = golden("commit")?.bytes()?;
    let data = golden("commit")?.data()?;
    // Result 2, a compare-and-set, given node 0 beside a value of 4 (PROTOCOL.md §6.3).
    let mut both = data.clone();
    put(&mut both, RESULTS + 32 + 8, 0, 4);
    put(&mut both, RESULTS + 32, 4, 8);
    assert_eq!(
        refusal(&reframe(&bytes, &both))?,
        ProtocolErrorEnum::FieldValue {
            command: 5,
            field: "result",
            value: 4
        }
    );
    // Result 2 created a node it does not name.
    let mut created = data.clone();
    created[RESULTS + 32 + 12] = 1;
    assert_eq!(
        refusal(&reframe(&bytes, &created))?,
        ProtocolErrorEnum::FieldValue {
            command: 5,
            field: "result",
            value: 1
        }
    );
    // Result 0 naming node 1, past the one-entry table.
    let mut past = data.clone();
    put(&mut past, RESULTS + 8, 1, 4);
    assert_eq!(
        refusal(&reframe(&bytes, &past))?,
        ProtocolErrorEnum::IndexOutOfRange {
            command: 5,
            section: 0,
            at: 0,
            index: 1,
            limit: 1
        }
    );
    Ok(())
}
