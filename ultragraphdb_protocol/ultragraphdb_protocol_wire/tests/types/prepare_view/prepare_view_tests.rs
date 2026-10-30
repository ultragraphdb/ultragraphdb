/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{BodyView, ColumnType, PatternSlot, Prepare, PrepareView};

use crate::types::prepare::prepare_tests::pattern;
use crate::utils::golden_bodies::{body_data, frame_of, golden, reframe, section, with_section};

#[test]
fn test_the_golden_prepares_read_their_values() {
    let bytes = golden("prepare_reply").bytes();
    let view = PrepareView::from_frame(&frame_of(&bytes)).unwrap();
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
        view.to_body().unwrap(),
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
    let bytes = golden("prepare").bytes();
    let view = PrepareView::from_frame(&frame_of(&bytes)).unwrap();
    assert!(view.result_types().is_empty());
    assert_eq!(view.statement(), 0);
}

#[test]
fn test_types_are_known_and_counted() {
    let bytes = golden("prepare_reply").bytes();
    let data = body_data(&bytes).to_vec();
    let (params, _) = section(&data, 0, 3);
    let (results, _) = section(&data, 0, 4);
    let mut unknown = data.clone();
    unknown[params] = 3;
    assert_eq!(
        PrepareView::from_frame(&frame_of(&reframe(&bytes, &unknown)))
            .unwrap_err()
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
        PrepareView::from_frame(&frame_of(&reframe(&bytes, &result)))
            .unwrap_err()
            .0,
        ProtocolErrorEnum::FieldValue {
            command: 10,
            field: "column type",
            value: 0
        }
    );
    let none = with_section(&bytes, 5, 3, |types| types.clear());
    assert_eq!(
        PrepareView::from_frame(&frame_of(&none)).unwrap_err().0,
        ProtocolErrorEnum::CountMismatch {
            command: 10,
            field: "parameter types",
            count: 0,
            expected: 1
        }
    );
    let two = with_section(&bytes, 5, 4, |types| types.push(2));
    assert_eq!(
        PrepareView::from_frame(&frame_of(&two)).unwrap_err().0,
        ProtocolErrorEnum::CountMismatch {
            command: 10,
            field: "result types",
            count: 2,
            expected: 1
        }
    );
}

#[test]
fn test_a_request_carries_no_result_types_and_an_answer_one_per_unbound_slot() {
    let request = golden("prepare").bytes();
    let reply = golden("prepare_reply").bytes();
    let request_view = PrepareView::from_frame(&frame_of(&request)).unwrap();
    let reply_view = PrepareView::from_frame(&frame_of(&reply)).unwrap();
    assert_eq!(request_view.check_request(), Ok(()));
    assert_eq!(reply_view.check_reply(), Ok(()));
    assert_eq!(
        reply_view.check_request().unwrap_err().0,
        ProtocolErrorEnum::CountMismatch {
            command: 10,
            field: "result types",
            count: 1,
            expected: 0
        }
    );
    // The request read as an answer: no result type for its one unbound slot.
    assert_eq!(
        request_view.check_reply().unwrap_err().0,
        ProtocolErrorEnum::CountMismatch {
            command: 10,
            field: "result types",
            count: 0,
            expected: 1
        }
    );
}
