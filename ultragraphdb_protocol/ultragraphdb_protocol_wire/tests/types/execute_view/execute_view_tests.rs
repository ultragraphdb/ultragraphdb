/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{BodyView, ExecuteView, RequestTermView};

use crate::types::execute::execute_tests::golden_execute;
use crate::utils::golden_bodies::{frame_of, golden, put, with_command_header, with_section};

#[test]
fn test_the_golden_execute_reads_its_values() {
    let bytes = golden("execute").bytes();
    let view = ExecuteView::from_frame(&frame_of(&bytes)).unwrap();
    assert_eq!(
        (view.statement(), view.transaction(), view.limit()),
        (1, 5, 0)
    );
    assert!(!view.statement_ids());
    assert_eq!(view.parameters(), &[0]);
    assert_eq!(
        view.terms().get(0),
        Some(RequestTermView::Iri("http://ex.org/a"))
    );
    assert!(view.resume().is_empty());
    assert_eq!(view.to_body().unwrap(), golden_execute());
}

#[test]
fn test_a_parameter_names_a_term_of_the_table() {
    let bytes = with_section(&golden("execute").bytes(), 4, 2, |p| put(p, 0, 1, 4));
    assert_eq!(
        ExecuteView::from_frame(&frame_of(&bytes)).unwrap_err().0,
        ProtocolErrorEnum::IndexOutOfRange {
            command: 11,
            section: 2,
            at: 0,
            index: 1,
            limit: 1
        }
    );
    let ragged = with_section(&golden("execute").bytes(), 4, 2, |p| p.push(0));
    assert_eq!(
        ExecuteView::from_frame(&frame_of(&ragged)).unwrap_err().0,
        ProtocolErrorEnum::SectionLength {
            command: 11,
            section: 2,
            len: 5,
            unit: 4
        }
    );
}

#[test]
fn test_flags_take_statement_ids_alone() {
    let bytes = golden("execute").bytes();
    let asked = with_command_header(&bytes, |h| put(h, 20, 1, 4));
    assert!(
        ExecuteView::from_frame(&frame_of(&asked))
            .unwrap()
            .statement_ids()
    );
    for flags in [2u64, 0x8000_0000] {
        let frame = with_command_header(&bytes, |h| put(h, 20, flags, 4));
        assert_eq!(
            ExecuteView::from_frame(&frame_of(&frame)).unwrap_err().0,
            ProtocolErrorEnum::FieldValue {
                command: 11,
                field: "flags",
                value: flags
            }
        );
    }
    let unused = with_command_header(&bytes, |h| h[24] = 1);
    assert_eq!(
        ExecuteView::from_frame(&frame_of(&unused)).unwrap_err().0,
        ProtocolErrorEnum::ReservedNotZero { offset: 152 }
    );
}
