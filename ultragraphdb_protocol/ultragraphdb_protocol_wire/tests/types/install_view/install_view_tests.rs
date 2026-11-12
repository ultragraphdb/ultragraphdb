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
    // PROTOCOL.md §8.4: `staged` at 24 is the last field; unused bytes are 32 to 127.
    for at in [32usize, 127] {
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
    let frame = with_section(&request()?, 7, 2, |report| report.push(b'x'));
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
    let frame = with_section(&request()?, 7, 0, |ends| ends.truncate(12));
    let third = u32::try_from("crmnamespace crm {}crm/people".len())? as usize;
    let odd = with_section(&frame, 7, 1, |text| text.truncate(third));
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
    let frame = with_section(&reply()?, 7, 2, |report| report[0] = 0xFF);
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

/// Stage 6.6: Install's `staged`, its migration report and its warnings (`PROTOCOL.md` §8.4, §4.9), over frames laid out
/// by hand from §8.4's tables: the command header `tenant` 3, `version`, `epoch`, `staged`; seven sections.
mod stage_6 {
    use ultragraphdb_error_protocol::ProtocolErrorEnum;
    use ultragraphdb_protocol_wire::{BodyView, Command, InstallView};
    use ultragraphdb_tool_bdd::{Context, TestResult};

    use crate::utils::golden_bodies::{frame_of, section};
    use crate::utils::ugs_frames::{
        UGS0902, UGS0902_MESSAGE, frame, outcome, refused, strings, warning_sections,
    };

    /// An Install of `version`, `epoch` and `staged` in tenant 3, the source `s` holding `t`, and `rest`, sections 2
    /// to 6.
    fn install(version: u64, epoch: u64, staged: u64, rest: [Vec<u8>; 5]) -> TestResult<Vec<u8>> {
        let [ends, text] = strings(&["s", "t"]);
        let mut sections = vec![ends, text];
        sections.extend(rest);
        frame(
            Command::Install,
            &[
                (0, &3u64.to_le_bytes()),
                (8, &version.to_le_bytes()),
                (16, &epoch.to_le_bytes()),
                (24, &staged.to_le_bytes()),
            ],
            &sections,
            true,
        )
    }

    fn empty() -> [Vec<u8>; 5] {
        [vec![], vec![], vec![], vec![], vec![]]
    }

    fn count_field(refusal: &ProtocolErrorEnum) -> Option<(u16, &'static str)> {
        match refusal {
            ProtocolErrorEnum::CountMismatch { command, field, .. } => Some((*command, *field)),
            _ => None,
        }
    }

    /// wire-protocol, "An Install reply carries its warnings": the version, the epoch, the report and one warning of
    /// `diagnostic` 902 in language 4, with its source, location and message.
    #[test]
    fn test_an_install_reply_reads_its_version_epoch_report_and_warning() -> TestResult {
        let [records, ends, text] = warning_sections(&[(UGS0902, 0, UGS0902_MESSAGE)]);
        let bytes = install(12, 44, 0, [b"r".to_vec(), vec![], records, ends, text])?;
        let view = InstallView::from_frame(&frame_of(&bytes)?)?;
        assert_eq!((view.version(), view.epoch(), view.report()), (12, 44, "r"));
        assert_eq!((view.staged(), view.migration_report()), (0, ""));
        let warnings = view.warnings();
        assert_eq!(warnings.len(), 1);
        let warning = warnings.get(0).context("one warning")?;
        assert_eq!((warning.number(), warning.language()), (902, 4));
        assert_eq!(warning.diagnostic(), UGS0902);
        assert_eq!(
            (warning.source(), warning.offset(), warning.length()),
            (0, 10, 7)
        );
        assert_eq!(warning.message(), UGS0902_MESSAGE);
        assert!(warnings.get(1).is_none());
        Ok(())
    }

    /// A reply's warning names its source by its place among the request's sources: source 1 is taken.
    #[test]
    fn test_an_install_reply_names_a_warnings_source() -> TestResult {
        let [records, ends, text] = warning_sections(&[(UGS0902, 1, "m")]);
        let bytes = install(12, 44, 0, [vec![], vec![], records, ends, text])?;
        let view = InstallView::from_frame(&frame_of(&bytes)?)?;
        assert_eq!(view.warnings().get(0).context("a warning")?.source(), 1);
        Ok(())
    }

    /// wire-protocol, "A request with a report is refused": in a request, sections 2 to 6 are empty, each refused as
    /// `CountMismatch` naming `report`.
    #[test]
    fn test_a_request_holding_any_reply_section_is_refused() -> TestResult {
        for index in 0..5 {
            let mut rest = empty();
            if let Some(section) = rest.get_mut(index) {
                section.extend_from_slice(b"xxxxxxxxxxxxxxxxxxxxxxxx");
            }
            let bytes = install(11, 0, 0, rest)?;
            let refusal = refused::<InstallView>(&bytes)?;
            assert_eq!(count_field(&refusal), Some((23, "report")), "{refusal:?}");
        }
        // The request of no reply section is read.
        assert_eq!(outcome::<InstallView>(&install(11, 0, 0, empty())?)?, None);
        Ok(())
    }

    /// `staged` is 0 in a request; a reply's migration report needs a staged namespace.
    #[test]
    fn test_staged_is_zero_in_a_request_and_names_a_replys_migration() -> TestResult {
        assert_eq!(
            refused::<InstallView>(&install(11, 0, 5, empty())?)?,
            ProtocolErrorEnum::FieldValue {
                command: 23,
                field: "staged",
                value: 5
            }
        );
        let unstaged = install(12, 44, 0, [vec![], b"m".to_vec(), vec![], vec![], vec![]])?;
        let refusal = refused::<InstallView>(&unstaged)?;
        assert_eq!(
            count_field(&refusal),
            Some((23, "migration report")),
            "{refusal:?}"
        );
        let staged = install(12, 44, 5, [vec![], b"m".to_vec(), vec![], vec![], vec![]])?;
        let view = InstallView::from_frame(&frame_of(&staged)?)?;
        assert_eq!((view.staged(), view.migration_report()), (5, "m"));
        Ok(())
    }

    /// The report and the migration report are UTF-8, each refused at its section's first invalid byte.
    #[test]
    fn test_the_reports_are_utf8() -> TestResult {
        let report = install(
            12,
            44,
            5,
            [vec![b'a', 0xFF], vec![], vec![], vec![], vec![]],
        )?;
        assert_eq!(
            refused::<InstallView>(&report)?,
            ProtocolErrorEnum::NotUtf8 {
                command: 23,
                section: 2,
                offset: 1
            }
        );
        let migration = install(12, 44, 5, [vec![], vec![0xFF], vec![], vec![], vec![]])?;
        assert_eq!(
            refused::<InstallView>(&migration)?,
            ProtocolErrorEnum::NotUtf8 {
                command: 23,
                section: 3,
                offset: 0
            }
        );
        Ok(())
    }

    /// `staged` takes bytes 24 to 31 of the command header, a reply's top byte among them; bytes 32 to 127 are zero,
    /// refused at the frame offset 128 + 32.
    #[test]
    fn test_unused_bytes_past_staged_are_zero() -> TestResult {
        let high = install(
            12,
            44,
            5 << 56,
            [vec![], b"m".to_vec(), vec![], vec![], vec![]],
        )?;
        assert_eq!(outcome::<InstallView>(&high)?, None);
        let bytes = crate::utils::golden_bodies::with_command_header(
            &install(12, 44, 0, empty())?,
            |header| header[32] = 1,
        );
        assert_eq!(
            refused::<InstallView>(&bytes)?,
            ProtocolErrorEnum::ReservedNotZero { offset: 160 }
        );
        Ok(())
    }

    /// Seven sections: the body of a request of one source holds the table of 56 bytes, then the two strings' ends
    /// and text, then five empty sections at the text's end rounded up to 8.
    #[test]
    fn test_an_install_body_holds_seven_sections() -> TestResult {
        let bytes = install(11, 0, 0, empty())?;
        let data = crate::utils::golden_bodies::body_data(&bytes);
        // Ends at 56 (8 bytes), text at 64 (2 bytes, "st"), sections 2 to 6 at align8(66) = 72.
        assert_eq!(section(data, 0, 0), (56, 8));
        assert_eq!(section(data, 0, 1), (64, 2));
        assert_eq!(section(data, 0, 6), (72, 0));
        assert_eq!(outcome::<InstallView>(&bytes)?, None);
        Ok(())
    }
}
