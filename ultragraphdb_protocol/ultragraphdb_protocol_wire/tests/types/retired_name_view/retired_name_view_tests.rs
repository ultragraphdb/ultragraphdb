/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A received retired name read in place, an entry of kind 20 (`PROTOCOL.md` §8.3).

use ultragraphdb_protocol_wire::{BodyView, DescribeView, RetiredFate, RetiredItem};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::golden_bodies::frame_of;
use crate::utils::ugs_frames::{describe_frame, entry_sections, retired};

/// `Company`, renamed to `Org` (string 1) by version 3's cutover until 1,700,000,000,000,000 µs; `Old`, a graph retired
/// by version 4.
#[test]
fn test_retired_names_read_their_fields() -> TestResult {
    let renamed = retired(1, 1, 1, 3, 1_700_000_000_000_000);
    let gone = retired(5, 2, u32::MAX, 4, 0);
    let bytes = describe_frame(
        &["Company", "Org", "Old"],
        entry_sections(&[(20, 0, &renamed), (20, 2, &gone)]),
        true,
    )?;
    let view = DescribeView::from_frame(&frame_of(&bytes)?)?;
    let first = view
        .entry(0)
        .context("an entry")?
        .retired_name()
        .context("a retired name")?;
    assert_eq!(
        (first.item(), first.fate()),
        (RetiredItem::Type, RetiredFate::Renamed)
    );
    assert_eq!(first.current(), Some("Org"));
    assert_eq!(
        (first.version(), first.deprecated_until()),
        (3, 1_700_000_000_000_000)
    );
    let second = view
        .entry(1)
        .context("an entry")?
        .retired_name()
        .context("a retired name")?;
    assert_eq!(
        (second.item(), second.fate()),
        (RetiredItem::Graph, RetiredFate::Retired)
    );
    assert_eq!(second.current(), None);
    assert_eq!((second.version(), second.deprecated_until()), (4, 0));
    Ok(())
}
