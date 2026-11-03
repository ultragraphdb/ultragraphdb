/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{BodyView, ExecuteView, RequestTermView};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::types::execute::execute_tests::golden_execute;
use crate::utils::golden_bodies::{frame_of, golden, put, with_command_header, with_section};

#[test]
fn test_the_golden_execute_reads_its_values() -> TestResult {
    let bytes = golden("execute")?.bytes()?;
    let view = ExecuteView::from_frame(&frame_of(&bytes)?)?;
    assert_eq!(
        (view.statement(), view.transaction(), view.limit()),
        (1, 5, 0)
    );
    assert_eq!(view.parameters(), &[0]);
    assert_eq!(
        view.terms().get(0),
        Some(RequestTermView::Iri("http://ex.org/a"))
    );
    assert!(view.resume().is_empty());
    assert_eq!(view.to_body()?, golden_execute());
    Ok(())
}

#[test]
fn test_a_parameter_names_a_term_of_the_table() -> TestResult {
    let bytes = with_section(&golden("execute")?.bytes()?, 4, 2, |p| put(p, 0, 1, 4));
    assert_eq!(
        ExecuteView::from_frame(&frame_of(&bytes)?)
            .err()
            .context("a refusal")?
            .0,
        ProtocolErrorEnum::IndexOutOfRange {
            command: 11,
            section: 2,
            at: 0,
            index: 1,
            limit: 1
        }
    );
    let ragged = with_section(&golden("execute")?.bytes()?, 4, 2, |p| p.push(0));
    assert_eq!(
        ExecuteView::from_frame(&frame_of(&ragged)?)
            .err()
            .context("a refusal")?
            .0,
        ProtocolErrorEnum::SectionLength {
            command: 11,
            section: 2,
            len: 5,
            unit: 4
        }
    );
    Ok(())
}

#[test]
fn test_bytes_after_the_limit_are_unused() -> TestResult {
    let bytes = golden("execute")?.bytes()?;
    // Bytes 20 to 127 are unused: the former flags among them.
    for at in [20usize, 23, 24, 127] {
        let frame = with_command_header(&bytes, |h| h[at] = 1);
        assert_eq!(
            ExecuteView::from_frame(&frame_of(&frame)?)
                .err()
                .context("a refusal")?
                .0,
            ProtocolErrorEnum::ReservedNotZero { offset: 128 + at }
        );
    }
    let limit = with_command_header(&bytes, |h| put(h, 16, 9, 4));
    assert_eq!(ExecuteView::from_frame(&frame_of(&limit)?)?.limit(), 9);
    Ok(())
}
