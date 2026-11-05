/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Install, InstallView, MAX_EPOCH, SOURCE_NAME_MAX,
};
use ultragraphdb_tool_bdd::TestResult;

use crate::types::install::install_tests::sources;
use crate::utils::golden_bodies::{
    frame_of, golden, put, view_any, with_command_header, with_section,
};
use crate::utils::term_records::refusal;

fn request() -> TestResult<Vec<u8>> {
    golden("install")?.bytes()
}

fn reply() -> TestResult<Vec<u8>> {
    golden("install_reply")?.bytes()
}

fn field(field: &'static str, value: u64) -> ProtocolErrorEnum {
    ProtocolErrorEnum::FieldValue {
        command: 23,
        field,
        value,
    }
}

#[test]
fn test_the_golden_frames_read_their_values() -> TestResult {
    let (asked, answered) = (request()?, reply()?);
    let view = InstallView::from_frame(&frame_of(&asked)?)?;
    assert_eq!((view.tenant(), view.version(), view.epoch()), (3, 11, 0));
    assert_eq!(view.source_count(), 2);
    assert_eq!(view.source(0), Some(("crm", "namespace crm {}")));
    assert_eq!(view.source(1), Some(("crm/people", "module people {}")));
    assert_eq!(view.source(2), None);
    assert_eq!(view.report(), "");
    assert_eq!(view.to_body()?, Install::new(3, 11, sources()));
    let answer = InstallView::from_frame(&frame_of(&answered)?)?;
    assert_eq!((answer.version(), answer.epoch()), (12, 44));
    assert_eq!(answer.report(), "coordination-free: 2");
    assert_eq!(view_any(&asked)?, Ok(()));
    assert_eq!(view_any(&answered)?, Ok(()));
    Ok(())
}

#[test]
fn test_unused_bytes_come_before_the_fields() -> TestResult {
    for at in [24usize, 127] {
        let frame = with_command_header(&reply()?, |h| {
            h[at] = 1;
            put(h, 8, 0, 8);
        });
        assert_eq!(
            refusal::<InstallView>(&frame)?,
            ProtocolErrorEnum::ReservedNotZero { offset: 128 + at }
        );
    }
    Ok(())
}

/// A tenant with no version installs on 0, so a request's `version` is free; a reply's is the version installed.
#[test]
fn test_a_replys_version_is_at_least_one_and_a_requests_any() -> TestResult {
    let frame = with_command_header(&reply()?, |h| put(h, 8, 0, 8));
    assert_eq!(refusal::<InstallView>(&frame)?, field("version", 0));
    let first = with_command_header(&request()?, |h| put(h, 8, 0, 8));
    assert_eq!(InstallView::from_frame(&frame_of(&first)?)?.version(), 0);
    Ok(())
}

#[test]
fn test_a_replys_epoch_is_an_epoch() -> TestResult {
    let past = MAX_EPOCH + 1;
    let frame = with_command_header(&reply()?, |h| put(h, 16, past, 8));
    assert_eq!(refusal::<InstallView>(&frame)?, field("epoch", past));
    Ok(())
}

#[test]
fn test_a_request_holds_no_report() -> TestResult {
    let frame = with_section(&request()?, 3, 2, |report| report.push(b'x'));
    assert_eq!(
        refusal::<InstallView>(&frame)?,
        ProtocolErrorEnum::CountMismatch {
            command: 23,
            field: "report",
            count: 1,
            expected: 0
        }
    );
    Ok(())
}

#[test]
fn test_sources_come_in_pairs_at_least_one() -> TestResult {
    let header = golden("install")?.header();
    let empty = Install::new(3, 11, Vec::new()).frame(header)?;
    assert_eq!(
        refusal::<InstallView>(&empty)?,
        ProtocolErrorEnum::CountMismatch {
            command: 23,
            field: "sources",
            count: 0,
            expected: 2
        }
    );
    // Three strings: the last end dropped and the text cut to the third string's end.
    let frame = with_section(&request()?, 3, 0, |ends| ends.truncate(12));
    let third = u32::try_from("crmnamespace crm {}crm/people".len())? as usize;
    let odd = with_section(&frame, 3, 1, |text| text.truncate(third));
    assert_eq!(
        refusal::<InstallView>(&odd)?,
        ProtocolErrorEnum::CountMismatch {
            command: 23,
            field: "sources",
            count: 3,
            expected: 4
        }
    );
    Ok(())
}

#[test]
fn test_a_source_name_is_at_most_its_bound() -> TestResult {
    let header = golden("install")?.header();
    let named = |len: usize| vec![("n".repeat(len), "text".to_string())];
    let longest = Install::new(3, 11, named(SOURCE_NAME_MAX)).frame(header.clone())?;
    assert_eq!(view_any(&longest)?, Ok(()));
    let past = Install::new(3, 11, named(SOURCE_NAME_MAX + 1)).frame(header)?;
    assert_eq!(
        refusal::<InstallView>(&past)?,
        field("source name", SOURCE_NAME_MAX as u64 + 1)
    );
    Ok(())
}

#[test]
fn test_a_report_is_utf8() -> TestResult {
    let frame = with_section(&reply()?, 3, 2, |report| report[0] = 0xFF);
    assert_eq!(
        refusal::<InstallView>(&frame)?,
        ProtocolErrorEnum::NotUtf8 {
            command: 23,
            section: 2,
            offset: 0
        }
    );
    Ok(())
}
