/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A received Describe entry read in place (`PROTOCOL.md` §8.3).

use ultragraphdb_protocol_wire::{BodyView, DescribeView, EntryKind};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::golden_bodies::frame_of;
use crate::utils::ugs_frames::{describe_frame, entry_sections};

/// A Type entry's opaque bytes and a source's text, each borrowed from the answer with its kind and name.
#[test]
fn test_an_entry_reads_its_kind_name_and_bytes() -> TestResult {
    let bytes = describe_frame(
        &["gen5.Org", "gen5"],
        entry_sections(&[(3, 0, &[1, 2, 3]), (19, 1, b"namespace gen5 {}")]),
        true,
    )?;
    let view = DescribeView::from_frame(&frame_of(&bytes)?)?;
    assert_eq!((view.tenant(), view.read_version()), (9, 42));
    assert_eq!(view.entry_count(), 2);
    let ty = view.entry(0).context("the type")?;
    assert_eq!(
        (ty.kind(), ty.name(), ty.bytes()),
        (EntryKind::Type, "gen5.Org", &[1u8, 2, 3][..])
    );
    assert!(ty.retired_name().is_none());
    let source = view.entry(1).context("the source")?;
    assert_eq!(source.kind(), EntryKind::Source);
    assert_eq!(source.name(), "gen5");
    assert_eq!(source.bytes(), b"namespace gen5 {}");
    assert!(view.entry(2).is_none());
    assert_eq!(view.entries().count(), 2);
    Ok(())
}
