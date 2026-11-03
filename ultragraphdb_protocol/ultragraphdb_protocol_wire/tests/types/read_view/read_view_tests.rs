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
fn test_bytes_after_the_limit_are_unused() -> TestResult {
    let bytes = golden("read")?.bytes()?;
    // Bytes 12 to 127 are unused: the former flags among them.
    for at in [12usize, 15, 16, 127] {
        let frame = with_command_header(&bytes, |h| h[at] = 1);
        assert_eq!(
            ReadView::from_frame(&frame_of(&frame)?)
                .err()
                .context("a refusal")?
                .0,
            ProtocolErrorEnum::ReservedNotZero { offset: 128 + at }
        );
    }
    let limit = with_command_header(&bytes, |h| put(h, 8, u32::MAX.into(), 4));
    assert_eq!(ReadView::from_frame(&frame_of(&limit)?)?.limit(), u32::MAX);
    Ok(())
}
