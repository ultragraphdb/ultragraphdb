/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeSet;

use ultragraphdb_common_service::{allowed_in_status, cpu_for, parse_cpu_list, usable_cpus};
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

/// Six CPUs as a VM gives them, 0 the one its interrupts land on.
fn six() -> BTreeSet<usize> {
    (0..6).collect()
}

#[test]
fn test_each_runtime_takes_its_own_cpu() {
    let allowed = six();
    let taken: Vec<Option<usize>> = (0..6).map(|core| cpu_for(&allowed, 0, core)).collect();
    let expected: Vec<Option<usize>> = allowed.iter().copied().map(Some).collect();
    assert_eq!(taken, expected);
}

#[test]
fn test_skipped_cpus_are_left_free_and_the_rest_taken_in_turn() {
    let allowed = six();
    let skip = 1;
    let usable: Vec<usize> = allowed.iter().copied().skip(skip).collect();
    for core in 0..12 {
        let expected = usable.get(core % usable.len()).copied();
        assert_eq!(cpu_for(&allowed, skip, core), expected, "core {core}");
        assert_ne!(cpu_for(&allowed, skip, core), allowed.first().copied());
    }
}

#[test]
fn test_a_permitted_set_with_gaps_is_taken_by_position() {
    let allowed = BTreeSet::from([2, 3, 8, 9]);
    assert_eq!(cpu_for(&allowed, 1, 0), Some(3));
    assert_eq!(cpu_for(&allowed, 1, 2), Some(9));
    assert_eq!(cpu_for(&allowed, 1, 3), Some(3));
}

#[test]
fn test_a_skip_past_every_cpu_leaves_the_last() {
    let allowed = six();
    let last = allowed.last().copied();
    for skip in [6, 7, usize::MAX] {
        for core in 0..3 {
            assert_eq!(cpu_for(&allowed, skip, core), last);
        }
    }
    assert_eq!(cpu_for(&BTreeSet::from([4]), 1, 5), Some(4));
}

#[test]
fn test_no_permitted_cpu_gives_none() {
    assert_eq!(cpu_for(&BTreeSet::new(), 0, 0), None);
    assert_eq!(cpu_for(&BTreeSet::new(), 3, 2), None);
}

#[test]
fn test_the_usable_cpus_are_the_permitted_ones_past_the_skipped() {
    let allowed = six();
    assert_eq!(usable_cpus(&allowed, 0), allowed);
    let past_first: BTreeSet<usize> = allowed.iter().copied().skip(1).collect();
    assert_eq!(usable_cpus(&allowed, 1), past_first);
    let last: BTreeSet<usize> = allowed.last().copied().into_iter().collect();
    assert_eq!(usable_cpus(&allowed, 9), last);
    assert_eq!(usable_cpus(&BTreeSet::new(), 1), BTreeSet::new());
}

#[test]
fn test_every_runtime_binds_inside_the_usable_cpus() {
    let allowed = six();
    for skip in 0..8 {
        let usable = usable_cpus(&allowed, skip);
        for core in 0..16 {
            let cpu = cpu_for(&allowed, skip, core);
            assert!(
                cpu.is_some_and(|cpu| usable.contains(&cpu)),
                "skip {skip} core {core}"
            );
        }
    }
}
