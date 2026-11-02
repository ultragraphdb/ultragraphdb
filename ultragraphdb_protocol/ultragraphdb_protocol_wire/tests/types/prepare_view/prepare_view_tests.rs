/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{BodyView, ColumnType, PatternSlot, Prepare, PrepareView};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::types::prepare::prepare_tests::pattern;
use crate::utils::golden_bodies::{body_data, frame_of, golden, reframe, section, with_section};

#[test]
fn test_the_golden_prepares_read_their_values() -> TestResult {
    let bytes = golden("prepare_reply")?.bytes()?;
    let view = PrepareView::from_frame(&frame_of(&bytes)?)?;
    assert_eq!(
        (view.namespace(), view.statement(), view.schema()),
        (0, 1, 0x5C4E_3A2B_1D0F_7E6A)
    );
    assert_eq!(view.pattern().subject(), PatternSlot::Parameter(0));
    assert_eq!(view.terms().len(), 1);
    assert_eq!(view.parameter_types(), &[ColumnType::Node]);
    assert_eq!(view.result_types(), &[ColumnType::Term]);
    let (terms, p) = pattern();
    assert_eq!(
        view.to_body()?,
        Prepare::new(
            0,
            1,
            0x5C4E_3A2B_1D0F_7E6A,
            terms,
            p,
            vec![ColumnType::Node],
            vec![ColumnType::Term]
        )
    );
    let request_bytes = golden("prepare")?.bytes()?;
    let request_view = PrepareView::from_frame(&frame_of(&request_bytes)?)?;
    assert!(request_view.result_types().is_empty());
    assert_eq!(request_view.statement(), 0);
    Ok(())
}

#[test]
fn test_types_are_known_and_counted() -> TestResult {
    let bytes = golden("prepare_reply")?.bytes()?;
    let data = body_data(&bytes).to_vec();
    let (params, _) = section(&data, 0, 3);
    let (results, _) = section(&data, 0, 4);
    let mut unknown = data.clone();
    unknown[params] = 3;
    assert_eq!(
        PrepareView::from_frame(&frame_of(&reframe(&bytes, &unknown))?)
            .err()
            .context("a refusal")?
            .0,
        ProtocolErrorEnum::FieldValue {
            command: 10,
            field: "column type",
            value: 3
        }
    );
    let mut result = data.clone();
    result[results] = 0;
    assert_eq!(
        PrepareView::from_frame(&frame_of(&reframe(&bytes, &result))?)
            .err()
            .context("a refusal")?
            .0,
        ProtocolErrorEnum::FieldValue {
            command: 10,
            field: "column type",
            value: 0
        }
    );
    let none = with_section(&bytes, 5, 3, |types| types.clear());
    assert_eq!(
        PrepareView::from_frame(&frame_of(&none)?)
            .err()
            .context("a refusal")?
            .0,
        ProtocolErrorEnum::CountMismatch {
            command: 10,
            field: "parameter types",
            count: 0,
            expected: 1
        }
    );
    let two = with_section(&bytes, 5, 4, |types| types.push(2));
    assert_eq!(
        PrepareView::from_frame(&frame_of(&two)?)
            .err()
            .context("a refusal")?
            .0,
        ProtocolErrorEnum::CountMismatch {
            command: 10,
            field: "result types",
            count: 2,
            expected: 1
        }
    );
    Ok(())
}

#[test]
fn test_a_request_carries_no_result_types_and_an_answer_one_per_unbound_slot() -> TestResult {
    let request = golden("prepare")?.bytes()?;
    let reply = golden("prepare_reply")?.bytes()?;
    let request_view = PrepareView::from_frame(&frame_of(&request)?)?;
    let reply_view = PrepareView::from_frame(&frame_of(&reply)?)?;
    assert_eq!(request_view.check_request(), Ok(()));
    assert_eq!(reply_view.check_reply(), Ok(()));
    assert_eq!(
        reply_view.check_request().err().context("a refusal")?.0,
        ProtocolErrorEnum::CountMismatch {
            command: 10,
            field: "result types",
            count: 1,
            expected: 0
        }
    );
    // The request read as an answer: no result type for its one unbound slot.
    assert_eq!(
        request_view.check_reply().err().context("a refusal")?.0,
        ProtocolErrorEnum::CountMismatch {
            command: 10,
            field: "result types",
            count: 0,
            expected: 1
        }
    );
    Ok(())
}
