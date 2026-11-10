/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The warnings structure's Order (`PROTOCOL.md` §4.9), over Install replies laid out by hand (sections 4 to 6 the
//! warnings, their messages' ends and text), and the Prepare and Result replies that hold it too.

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{BodyView, InstallView, WARNING_MESSAGE_MAX, WARNINGS_MAX};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::golden_bodies::{body_data, frame_of, section};
use crate::utils::ugs_frames::{
    UGS0902, install_reply, outcome, refused, strings, warning, warning_sections,
};

fn field_value(field: &'static str, value: u64) -> ProtocolErrorEnum {
    ProtocolErrorEnum::FieldValue {
        command: 23,
        field,
        value,
    }
}

/// One warning of `record`'s bytes with the message "m".
fn one(record: Vec<u8>) -> TestResult<Vec<u8>> {
    let [ends, text] = strings(&["m"]);
    install_reply([record, ends, text])
}

/// No warning: the three sections empty, and the reply read.
#[test]
fn test_a_reply_of_no_warning_holds_three_empty_sections() -> TestResult {
    let bytes = install_reply([vec![], vec![], vec![]])?;
    let view = InstallView::from_frame(&frame_of(&bytes)?)?;
    assert!(view.warnings().is_empty());
    assert_eq!(view.warnings().len(), 0);
    assert_eq!(view.warnings().iter().count(), 0);
    Ok(())
}

/// At most 64 warnings: 64 are read, 65 refused as `CountMismatch` naming `warnings`.
#[test]
fn test_a_reply_carries_at_most_64_warnings() -> TestResult {
    let many = |n: usize| -> TestResult<Vec<u8>> {
        let warnings: Vec<(u32, u32, &str)> = (0..n).map(|_| (UGS0902, 0, "m")).collect();
        install_reply(warning_sections(&warnings))
    };
    let at_bound = many(WARNINGS_MAX)?;
    let view = InstallView::from_frame(&frame_of(&at_bound)?)?;
    assert_eq!(view.warnings().len(), 64);
    assert_eq!(view.warnings().iter().count(), 64);
    let past = many(WARNINGS_MAX + 1)?;
    let refusal = refused::<InstallView>(&past)?;
    assert!(
        matches!(
            refusal,
            ProtocolErrorEnum::CountMismatch {
                command: 23,
                field: "warnings",
                count: 65,
                ..
            }
        ),
        "{refusal:?}"
    );
    Ok(())
}

/// The warnings' section is a whole number of 24-byte warnings, checked before the messages' string list.
#[test]
fn test_the_warnings_are_whole_24_byte_records_checked_first() -> TestResult {
    let mut record = warning(UGS0902, 0, 10, 7, 0);
    record.push(0);
    assert_eq!(
        refused::<InstallView>(&one(record.clone())?)?,
        ProtocolErrorEnum::SectionLength {
            command: 23,
            section: 4,
            len: 25,
            unit: 24
        }
    );
    // A broken messages' list too: the warnings' length is still named first.
    let broken = install_reply([record, vec![1, 0, 0], b"m".to_vec()])?;
    assert_eq!(
        refused::<InstallView>(&broken)?,
        ProtocolErrorEnum::SectionLength {
            command: 23,
            section: 4,
            len: 25,
            unit: 24
        }
    );
    // The messages are a string list, refused by §4.3's rules on sections 5 and 6.
    let ends = install_reply([warning(UGS0902, 0, 10, 7, 0), vec![1, 0, 0], b"m".to_vec()])?;
    assert_eq!(
        refused::<InstallView>(&ends)?,
        ProtocolErrorEnum::SectionLength {
            command: 23,
            section: 5,
            len: 3,
            unit: 4
        }
    );
    Ok(())
}

/// A warning's bytes 20 to 23 are zero, refused at the body offset of the first nonzero one.
#[test]
fn test_a_warnings_last_four_bytes_are_zero() -> TestResult {
    let mut record = warning(UGS0902, 0, 10, 7, 0);
    if let Some(byte) = record.get_mut(22) {
        *byte = 1;
    }
    let bytes = one(record)?;
    let (start, _) = section(body_data(&bytes), 0, 4);
    assert_eq!(
        refused::<InstallView>(&bytes)?,
        ProtocolErrorEnum::PaddingNotZero {
            command: 23,
            offset: start + 22
        }
    );
    Ok(())
}

/// `diagnostic`'s bits 24 to 31 are zero and its language one §9.4 lists, 1 to 4.
#[test]
fn test_a_diagnostic_names_a_listed_language_in_bits_16_to_23() -> TestResult {
    let high = (1 << 24) | UGS0902;
    assert_eq!(
        refused::<InstallView>(&one(warning(high, 0, 10, 7, 0))?)?,
        field_value("diagnostic", high.into())
    );
    for language in [0u32, 5, 255] {
        let diagnostic = (language << 16) | 902;
        assert_eq!(
            refused::<InstallView>(&one(warning(diagnostic, 0, 10, 7, 0))?)?,
            field_value("diagnostic", diagnostic.into())
        );
    }
    for language in 1u32..=4 {
        let diagnostic = (language << 16) | 902;
        assert_eq!(
            outcome::<InstallView>(&one(warning(diagnostic, 0, 10, 7, 0))?)?,
            None
        );
    }
    Ok(())
}

/// `offset + length` fits a u32: 0xFFFFFFFE + 1 is read, 0xFFFFFFFF + 1 refused as `FieldValue` naming `location`.
#[test]
fn test_a_location_ends_within_a_u32() -> TestResult {
    assert_eq!(
        outcome::<InstallView>(&one(warning(UGS0902, 0, u32::MAX - 1, 1, 0))?)?,
        None
    );
    let refusal = refused::<InstallView>(&one(warning(UGS0902, 0, u32::MAX, 1, 0))?)?;
    assert!(
        matches!(
            refusal,
            ProtocolErrorEnum::FieldValue {
                command: 23,
                field: "location",
                ..
            }
        ),
        "{refusal:?}"
    );
    Ok(())
}

/// `message` indexes the messages' list: index 1 of one message is refused, naming the warnings' section and the
/// warning.
#[test]
fn test_a_message_index_is_below_the_string_count() -> TestResult {
    assert_eq!(
        refused::<InstallView>(&one(warning(UGS0902, 0, 10, 7, 1))?)?,
        ProtocolErrorEnum::IndexOutOfRange {
            command: 23,
            section: 4,
            at: 0,
            index: 1,
            limit: 1
        }
    );
    Ok(())
}

/// A message is at most 512 bytes: 512 is read, 513 refused as `FieldValue` naming `message`.
#[test]
fn test_a_message_is_at_most_512_bytes() -> TestResult {
    let at_bound = "x".repeat(WARNING_MESSAGE_MAX);
    let bytes = install_reply(warning_sections(&[(UGS0902, 0, &at_bound)]))?;
    let view = InstallView::from_frame(&frame_of(&bytes)?)?;
    assert_eq!(
        view.warnings().get(0).context("a warning")?.message().len(),
        512
    );
    let past = "x".repeat(WARNING_MESSAGE_MAX + 1);
    let refusal =
        refused::<InstallView>(&install_reply(warning_sections(&[(UGS0902, 0, &past)]))?)?;
    assert!(
        matches!(
            refusal,
            ProtocolErrorEnum::FieldValue {
                command: 23,
                field: "message",
                ..
            }
        ),
        "{refusal:?}"
    );
    Ok(())
}

/// Every warning names its own message, two warnings may share one.
#[test]
fn test_two_warnings_may_name_one_message() -> TestResult {
    let records = [warning(UGS0902, 0, 0, 1, 0), warning(UGS0902, 0, 2, 3, 0)].concat();
    let [ends, text] = strings(&["shared"]);
    let bytes = install_reply([records, ends, text])?;
    let view = InstallView::from_frame(&frame_of(&bytes)?)?;
    let messages: Vec<&str> = view.warnings().iter().map(|w| w.message()).collect();
    assert_eq!(messages, ["shared", "shared"]);
    Ok(())
}
