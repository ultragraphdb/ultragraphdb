/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{BodyView, PatternSlot, ReadView, RequestTermView};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::types::read::read_tests::golden_read;
use crate::utils::golden_bodies::{frame_of, golden, put, with_command_header};

#[test]
fn test_the_golden_read_reads_its_values() -> TestResult {
    let bytes = golden("read")?.bytes()?;
    let view = ReadView::from_frame(&frame_of(&bytes)?)?;
    assert_eq!((view.transaction(), view.limit()), (5, 100));
    assert!(view.statement_ids());
    assert_eq!(view.resume(), &[1, 2, 3]);
    assert_eq!(view.pattern().subject(), PatternSlot::Bound(0));
    assert_eq!(view.pattern().predicate(), PatternSlot::Bound(1));
    assert_eq!(
        view.terms().get(0),
        Some(RequestTermView::Iri("http://ex.org/a"))
    );
    assert_eq!(
        view.terms().get(1),
        Some(RequestTermView::Iri("http://ex.org/name"))
    );
    assert_eq!(view.to_body()?, golden_read());
    Ok(())
}

#[test]
fn test_a_flag_other_than_statement_ids_is_refused() -> TestResult {
    let bytes = golden("read")?.bytes()?;
    for flags in [2u64, 4, 0x8000_0000, 3] {
        let frame = with_command_header(&bytes, |h| put(h, 12, flags, 4));
        assert_eq!(
            ReadView::from_frame(&frame_of(&frame)?)
                .err()
                .context("a refusal")?
                .0,
            ProtocolErrorEnum::FieldValue {
                command: 7,
                field: "flags",
                value: flags
            }
        );
    }
    let none = with_command_header(&bytes, |h| put(h, 12, 0, 4));
    assert!(!ReadView::from_frame(&frame_of(&none)?)?.statement_ids());
    // Bytes 16 to 127 are unused, checked before the flags.
    let frame = with_command_header(&bytes, |h| {
        h[16] = 1;
        put(h, 12, 2, 4);
    });
    assert_eq!(
        ReadView::from_frame(&frame_of(&frame)?)
            .err()
            .context("a refusal")?
            .0,
        ProtocolErrorEnum::ReservedNotZero { offset: 144 }
    );
    Ok(())
}
