/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{BodyBuilder, Install};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::golden_bodies::golden;

/// The golden Install's sources: a schema and a module.
pub fn sources() -> Vec<(String, String)> {
    [
        ("crm", "namespace crm {}"),
        ("crm/people", "module people {}"),
    ]
    .map(|(name, text)| (name.to_string(), text.to_string()))
    .to_vec()
}

#[test]
fn test_a_request_and_its_reply_build_the_golden_frames() -> TestResult {
    let request = Install::new(3, 11, sources());
    assert_eq!((request.tenant(), request.version()), (3, 11));
    assert_eq!(request.epoch(), 0);
    assert_eq!(request.sources(), sources());
    assert_eq!(request.report(), "");
    assert_eq!(
        request.frame(golden("install")?.header())?,
        golden("install")?.bytes()?
    );
    let report = "coordination-free: 2";
    let reply = request.installed(12, 44, report);
    assert_eq!((reply.version(), reply.epoch()), (12, 44));
    assert_eq!(reply.report(), report);
    assert_eq!(
        reply.frame(golden("install_reply")?.header())?,
        golden("install_reply")?.bytes()?
    );
    Ok(())
}

/// Stage 6.6: Install's builder writes `staged`, the migration report and the warnings, checked against frames laid out
/// by hand from `PROTOCOL.md` §8.4 and §4.9.
mod stage_6 {
    use ultragraphdb_protocol_wire::{
        BodyBuilder, BodyView, Command, Header, Install, InstallView, Warning,
    };
    use ultragraphdb_tool_bdd::{Context, TestResult};

    use crate::utils::golden_bodies::{CLUSTER, frame_of};
    use crate::utils::ugs_frames::{UGS0902, UGS0902_MESSAGE, frame, strings, warning_sections};

    fn header() -> Header {
        Header::new(CLUSTER, Command::Install)
            .with_stream(3)
            .with_flags(ultragraphdb_protocol_wire::FLAG_LAST)
    }

    /// A request of one source builds seven sections, sections 2 to 6 empty.
    #[test]
    fn test_a_request_builds_seven_sections() -> TestResult {
        let request = Install::new(3, 11, vec![("s".to_string(), "t".to_string())]);
        let [ends, text] = strings(&["s", "t"]);
        let expected = frame(
            Command::Install,
            &[(0, &3u64.to_le_bytes()), (8, &11u64.to_le_bytes())],
            &[ends, text, vec![], vec![], vec![], vec![], vec![]],
            true,
        )?;
        assert_eq!(request.frame(header())?, expected);
        assert_eq!(request.staged(), 0);
        assert_eq!(request.migration_report(), "");
        assert!(request.warnings().is_empty());
        Ok(())
    }

    /// wire-protocol, "An Install reply carries its warnings": built with one warning `UGS0902` and read back, the
    /// view answers the version, the epoch, the report and the warning, with its source, location and message.
    #[test]
    fn test_an_install_reply_with_a_warning_is_built_and_read_back() -> TestResult {
        let warning = Warning::new(UGS0902, UGS0902_MESSAGE).with_location(10..17);
        let reply = Install::new(3, 11, vec![("s".to_string(), "t".to_string())])
            .installed(12, 44, "r")
            .with_warnings(vec![warning.clone()]);
        assert_eq!(reply.warnings(), &[warning]);
        let [ends, text] = strings(&["s", "t"]);
        let [records, message_ends, message_text] =
            warning_sections(&[(UGS0902, 0, UGS0902_MESSAGE)]);
        let expected = frame(
            Command::Install,
            &[
                (0, &3u64.to_le_bytes()),
                (8, &12u64.to_le_bytes()),
                (16, &44u64.to_le_bytes()),
            ],
            &[
                ends,
                text,
                b"r".to_vec(),
                vec![],
                records,
                message_ends,
                message_text,
            ],
            true,
        )?;
        let bytes = reply.frame(header())?;
        assert_eq!(bytes, expected);
        let view = InstallView::from_frame(&frame_of(&bytes)?)?;
        assert_eq!((view.version(), view.epoch(), view.report()), (12, 44, "r"));
        let read = view.warnings().get(0).context("the warning")?;
        assert_eq!((read.number(), read.language()), (902, 4));
        assert_eq!((read.source(), read.offset(), read.length()), (0, 10, 7));
        assert_eq!(read.message(), UGS0902_MESSAGE);
        assert_eq!(view.to_body()?, reply);
        Ok(())
    }

    /// A reply to sources holding a migration writes `staged` at command-header offset 24 and the migration report in
    /// section 3.
    #[test]
    fn test_a_staged_reply_writes_its_namespace_and_migration_report() -> TestResult {
        let reply = Install::new(3, 11, vec![("s".to_string(), "t".to_string())])
            .installed(12, 44, "")
            .with_staged(5)
            .with_migration_report("m");
        assert_eq!((reply.staged(), reply.migration_report()), (5, "m"));
        let [ends, text] = strings(&["s", "t"]);
        let expected = frame(
            Command::Install,
            &[
                (0, &3u64.to_le_bytes()),
                (8, &12u64.to_le_bytes()),
                (16, &44u64.to_le_bytes()),
                (24, &5u64.to_le_bytes()),
            ],
            &[ends, text, vec![], b"m".to_vec(), vec![], vec![], vec![]],
            true,
        )?;
        assert_eq!(reply.frame(header())?, expected);
        Ok(())
    }
}
