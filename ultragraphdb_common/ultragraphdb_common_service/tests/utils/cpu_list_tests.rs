/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_service::{allowed_in_status, parse_cpu_list};
use ultragraphdb_tool_bdd::{Context, TestResult};

#[test]
fn test_a_cpu_list_reads_as_the_kernel_writes_it() -> TestResult {
    let cpus = parse_cpu_list("0-2,8\n").context("a list")?;
    assert_eq!(cpus.into_iter().collect::<Vec<_>>(), [0, 1, 2, 8]);
    assert_eq!(parse_cpu_list("5").context("one CPU")?.len(), 1);
    Ok(())
}

#[test]
fn test_a_malformed_or_oversized_list_is_none() {
    for list in ["", "a", "1-", "-1", "3-1", "1,,2", "0-65536", "65536"] {
        assert_eq!(parse_cpu_list(list), None, "{list}");
    }
    assert!(
        parse_cpu_list("0-65535").is_some(),
        "the largest list a role reads"
    );
}

#[test]
fn test_the_status_line_names_the_allowed_cpus() -> TestResult {
    let status = "Name:\tx\nCpus_allowed:\tff\nCpus_allowed_list:\t2-3\nMems_allowed:\t1\n";
    let allowed = allowed_in_status(status).context("the line")?;
    assert_eq!(allowed.into_iter().collect::<Vec<_>>(), [2, 3]);
    assert_eq!(allowed_in_status("Name:\tx\n"), None);
    Ok(())
}
