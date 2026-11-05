/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_tool_bdd::TestResult;
use ultragraphdb_traits_catalog::HeldVersion;

#[test]
fn test_a_held_version_reads_back_what_it_was_made_of() -> TestResult {
    let held = HeldVersion::new(3, 7, 41, vec![9, 8]);
    assert_eq!(held.tenant(), 3);
    assert_eq!(held.version(), 7);
    assert_eq!(held.effective(), 41);
    assert_eq!(held.contents(), &[9, 8]);
    Ok(())
}

#[test]
fn test_held_versions_differ_in_each_field() -> TestResult {
    let held = HeldVersion::new(0, 1, 2, vec![3]);
    assert_ne!(held, HeldVersion::new(1, 1, 2, vec![3]));
    assert_ne!(held, HeldVersion::new(0, 2, 2, vec![3]));
    assert_ne!(held, HeldVersion::new(0, 1, 3, vec![3]));
    assert_ne!(held, HeldVersion::new(0, 1, 2, vec![]));
    assert_eq!(held, HeldVersion::new(0, 1, 2, vec![3]));
    Ok(())
}

#[test]
fn test_a_version_at_the_bounds_of_its_fields_keeps_them() -> TestResult {
    let held = HeldVersion::new(u64::MAX, u64::MAX, u64::MAX, Vec::new());
    assert_eq!(
        (held.tenant(), held.version(), held.effective()),
        (u64::MAX, u64::MAX, u64::MAX)
    );
    assert!(held.contents().is_empty());
    Ok(())
}
