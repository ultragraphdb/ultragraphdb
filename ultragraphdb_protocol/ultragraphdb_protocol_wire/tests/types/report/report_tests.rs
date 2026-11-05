/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{BodyBuilder, Report, ReportKind, RequestTerm};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::golden_roles::{golden_role, role_bytes};

pub const AGE: &str = "http://ex.org/age";
pub const INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";

/// The golden refusal: version 12's change of `<age>`, refused by `"abc"^^xsd:integer`.
pub fn golden_refused() -> TestResult<Report> {
    Ok(Report::refused(
        6,
        4,
        12,
        AGE,
        RequestTerm::Literal {
            lexical: "abc".into(),
            datatype: Some(INTEGER.into()),
        },
    )?)
}

#[test]
fn test_the_golden_reports_are_built_byte_for_byte() -> TestResult {
    let refused = golden_refused()?;
    assert_eq!(
        (
            refused.partition(),
            refused.kind(),
            refused.tenant(),
            refused.version()
        ),
        (6, ReportKind::Refused, 4, 12)
    );
    assert_eq!(refused.terms().len(), 2);
    for (report, name) in [
        (refused, "report_refused"),
        (Report::passed(6, 4, 13), "report_passed"),
        (
            Report::build_failed(6, 4, 14, "http://ex.org/embedding"),
            "report_build_failed",
        ),
    ] {
        let golden = golden_role(name)?;
        assert_eq!(
            report.frame(golden.header())?,
            role_bytes(&golden)?,
            "{name}"
        );
    }
    assert!(Report::passed(1, 1, 1).terms().is_empty());
    assert_eq!(
        Report::build_failed(1, 1, 1, AGE).kind(),
        ReportKind::BuildFailed
    );
    Ok(())
}

#[test]
fn test_a_term_at_fault_that_is_the_predicate_is_named_once() -> TestResult {
    let report = Report::refused(1, 2, 3, AGE, RequestTerm::Iri(AGE.into()))?;
    assert_eq!(report.terms().len(), 1);
    Ok(())
}

#[test]
fn test_a_triple_term_at_fault_is_refused() -> TestResult {
    let refusal = Report::refused(
        1,
        2,
        3,
        AGE,
        RequestTerm::Triple {
            subject: 0,
            predicate: 0,
            object: 0,
        },
    )
    .err()
    .context("a refusal")?;
    assert_eq!(
        refusal.0,
        ProtocolErrorEnum::BadTerm {
            command: 530,
            section: 0,
            at: 1
        }
    );
    Ok(())
}

#[test]
fn test_the_fields_sit_at_their_offsets() {
    let header =
        Report::passed(0x0403_0201, 0x0C0B_0A09_0807_0605, 0x1413_1211_100F_0E0D).command_header();
    assert_eq!(&header[0..4], &[1, 2, 3, 4]);
    assert_eq!(header[4], 1);
    assert_eq!(&header[5..8], &[0; 3]);
    assert_eq!(&header[8..16], &[5, 6, 7, 8, 9, 10, 11, 12]);
    assert_eq!(&header[16..24], &[13, 14, 15, 16, 17, 18, 19, 20]);
    assert!(header[24..].iter().all(|b| *b == 0));
}
