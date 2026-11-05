/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_tool_bdd::TestResult;
use ultragraphdb_traits_catalog::DefaultEntry;

#[test]
fn test_a_default_entry_reads_back_what_it_was_made_of() -> TestResult {
    let entry = DefaultEntry::new(4, "http://ex.org/score".to_string(), vec![2, 1]);
    assert_eq!(entry.tenant(), 4);
    assert_eq!(entry.predicate(), "http://ex.org/score");
    assert_eq!(entry.entry(), &[2, 1]);
    Ok(())
}

#[test]
fn test_default_entries_order_by_tenant_then_predicate() -> TestResult {
    let a = DefaultEntry::new(0, "http://ex.org/b".to_string(), vec![]);
    let b = DefaultEntry::new(1, "http://ex.org/a".to_string(), vec![]);
    let c = DefaultEntry::new(1, "http://ex.org/b".to_string(), vec![]);
    assert!(a < b && b < c);
    assert_ne!(
        c,
        DefaultEntry::new(1, "http://ex.org/b".to_string(), vec![0])
    );
    Ok(())
}
