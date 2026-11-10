/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A warning as its sender holds it (`PROTOCOL.md` §4.9).

use ultragraphdb_protocol_wire::Warning;
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::ugs_frames::{UGS0902, UGS0902_MESSAGE};

/// A new warning names its diagnostic and message, source 0 and an empty location until they are set.
#[test]
fn test_a_warning_holds_its_diagnostic_source_location_and_message() -> TestResult {
    let warning = Warning::new(UGS0902, UGS0902_MESSAGE);
    assert_eq!(warning.diagnostic(), UGS0902);
    assert_eq!(warning.source(), 0);
    assert_eq!(warning.location(), 0..0);
    assert_eq!(warning.message(), UGS0902_MESSAGE);
    let located = warning.with_source(2).with_location(10..17);
    assert_eq!(located.source(), 2);
    assert_eq!(located.location(), 10..17);
    assert_eq!(located.diagnostic(), UGS0902);
    Ok(())
}
