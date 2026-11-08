/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_frontend::ReportAsked;
use ultragraphdb_core_ids::Tenant;
use ultragraphdb_tool_bdd::TestResult;

const TENANT: u64 = 3;
const VERSION: u64 = 7;
const PARTITION: u32 = 2;
const PREDICATE: &str = "http://ex.org/p";
const AT_FAULT: &str = "\"x\"";

#[test]
fn test_a_report_reads_back_what_it_was_built_from() -> TestResult {
    let refused = Some((PREDICATE.to_string(), AT_FAULT.to_string()));
    let report = ReportAsked::new(Tenant::new(TENANT), VERSION, PARTITION, refused.clone());
    assert_eq!(report.tenant(), Tenant::new(TENANT));
    assert_eq!(report.version(), VERSION);
    assert_eq!(report.partition(), PARTITION);
    assert_eq!(report.refused(), Some((PREDICATE, AT_FAULT)));
    assert_eq!(report.into_refused(), refused);
    Ok(())
}

#[test]
fn test_a_passed_check_refuses_nothing() -> TestResult {
    let report = ReportAsked::new(Tenant::new(TENANT), VERSION, PARTITION, None);
    assert_eq!(report.refused(), None);
    assert_eq!(report.into_refused(), None);
    Ok(())
}
