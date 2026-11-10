/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A received warning read in place (`PROTOCOL.md` §4.9): its diagnostic split into number and language as code 29's
//! `detail` encodes it (§9.4).

use ultragraphdb_protocol_wire::{BodyView, InstallView};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::golden_bodies::frame_of;
use crate::utils::ugs_frames::{install_reply, warning_sections};

/// `GQL0210` is 210 in language 1 (§9.4's example); `UQL0110` 110 in language 3; each read with its fields.
#[test]
fn test_a_warning_reads_its_number_language_and_location() -> TestResult {
    let gql = (1 << 16) | 210;
    let uql = (3 << 16) | 110;
    let bytes = install_reply(warning_sections(&[
        (gql, 1, "GQL0210 a"),
        (uql, 0, "UQL0110 b"),
    ]))?;
    let view = InstallView::from_frame(&frame_of(&bytes)?)?;
    let first = view.warnings().get(0).context("the first warning")?;
    assert_eq!(
        (first.diagnostic(), first.number(), first.language()),
        (gql, 210, 1)
    );
    assert_eq!((first.source(), first.offset(), first.length()), (1, 10, 7));
    assert_eq!(first.message(), "GQL0210 a");
    let second = view.warnings().get(1).context("the second warning")?;
    assert_eq!((second.number(), second.language()), (110, 3));
    assert_eq!(second.message(), "UQL0110 b");
    Ok(())
}
