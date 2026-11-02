/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{BodyView, Commit, CommitView, OperationResult};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::golden_bodies::{frame_of, golden, reframe};

#[test]
fn test_the_golden_commit_reads_its_values() -> TestResult {
    let bytes = golden("commit")?.bytes()?;
    let view = CommitView::from_frame(&frame_of(&bytes)?)?;
    assert_eq!(
        (view.session(), view.request(), view.commit_version()),
        (9, 7, 1000)
    );
    assert_eq!(view.result_count(), 2);
    assert_eq!(view.result(0), Some(OperationResult::new(0x10, true)));
    assert_eq!(view.result(1), Some(OperationResult::new(0x20, false)));
    assert_eq!(view.result(2), None);
    assert_eq!(
        view.to_body()?,
        Commit::new(
            9,
            7,
            1000,
            vec![
                OperationResult::new(0x10, true),
                OperationResult::new(0x20, false)
            ]
        )
    );
    Ok(())
}

#[test]
fn test_results_are_whole_records_with_a_flag_and_zeros() -> TestResult {
    let bytes = golden("commit")?.bytes()?;
    let data = golden("commit")?.data()?;
    // The results section starts at 8 and holds 32 bytes.
    let mut short = data[..data.len() - 1].to_vec();
    short[4] = 31;
    assert_eq!(
        CommitView::from_frame(&frame_of(&reframe(&bytes, &short))?)
            .err()
            .context("a refusal")?
            .0,
        ProtocolErrorEnum::SectionLength {
            command: 5,
            section: 0,
            len: 31,
            unit: 16
        }
    );
    let mut flag = data.clone();
    flag[8 + 8] = 2;
    assert_eq!(
        CommitView::from_frame(&frame_of(&reframe(&bytes, &flag))?)
            .err()
            .context("a refusal")?
            .0,
        ProtocolErrorEnum::FieldValue {
            command: 5,
            field: "created",
            value: 2
        }
    );
    let mut zeros = data.clone();
    zeros[8 + 16 + 15] = 1;
    assert_eq!(
        CommitView::from_frame(&frame_of(&reframe(&bytes, &zeros))?)
            .err()
            .context("a refusal")?
            .0,
        ProtocolErrorEnum::PaddingNotZero {
            command: 5,
            offset: 8 + 16 + 15
        }
    );
    Ok(())
}

#[test]
fn test_a_commit_request_carries_no_results() -> TestResult {
    // The golden Commit is a reply, with two results: as a request it is refused.
    let reply = golden("commit")?.bytes()?;
    let view = CommitView::from_frame(&frame_of(&reply)?)?;
    assert_eq!(
        view.check_request().err().context("a refusal")?.0,
        ProtocolErrorEnum::CountMismatch {
            command: 5,
            field: "results",
            count: 2,
            expected: 0
        }
    );
    use ultragraphdb_protocol_wire::BodyBuilder;
    // One result is one too many as well.
    let one = Commit::new(9, 7, 0, vec![OperationResult::new(1, false)])
        .frame(golden("commit")?.header())?;
    assert_eq!(
        CommitView::from_frame(&frame_of(&one)?)?
            .check_request()
            .err()
            .context("a refusal")?
            .0,
        ProtocolErrorEnum::CountMismatch {
            command: 5,
            field: "results",
            count: 1,
            expected: 0
        }
    );
    let request = Commit::new(9, 7, 0, Vec::new()).frame(golden("commit")?.header())?;
    assert_eq!(
        CommitView::from_frame(&frame_of(&request)?)?.check_request(),
        Ok(())
    );
    Ok(())
}
