/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{BodyView, ExecuteView, RequestTermView};

use crate::types::execute::execute_tests::golden_execute;
use crate::utils::golden_bodies::{frame_of, golden, put, with_section};

#[test]
fn test_the_golden_execute_reads_its_values() {
    let bytes = golden("execute").bytes();
    let view = ExecuteView::from_frame(&frame_of(&bytes)).unwrap();
    assert_eq!((view.statement(), view.limit()), (1, 0));
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
