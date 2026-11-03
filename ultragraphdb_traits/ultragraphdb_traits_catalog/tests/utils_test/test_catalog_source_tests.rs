/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_tool_bdd::TestResult;
use ultragraphdb_traits_catalog::{
    CatalogSource, CatalogUnreachable, DefaultEntry, HeldVersion, TestCatalogSource,
};

fn held(namespace: u64, version: u64, effective: u64) -> HeldVersion {
    HeldVersion::new(namespace, version, effective, vec![version as u8])
}

fn default(namespace: u64, predicate: &str, entry: u8) -> DefaultEntry {
    DefaultEntry::new(namespace, format!("http://ex.org/{predicate}"), vec![entry])
}

#[test]
fn test_no_version_is_in_effect_before_the_first() -> TestResult {
    let source = TestCatalogSource::new();
    assert_eq!(source.version_at(0, 0)?, None);
    source.commit(held(0, 1, 10));
    assert_eq!(source.version_at(0, 9)?, None);
    assert_eq!(source.version_at(0, 10)?, Some(held(0, 1, 10)));
    Ok(())
}

#[test]
fn test_the_version_in_effect_is_the_latest_at_or_below_the_epoch() -> TestResult {
    let source = TestCatalogSource::new();
    source.commit(held(0, 2, 40));
    source.commit(held(0, 3, 50));
    source.commit(held(0, 4, 50));
    assert_eq!(source.version_at(0, 39)?, None);
    assert_eq!(source.version_at(0, 40)?, Some(held(0, 2, 40)));
    assert_eq!(source.version_at(0, 49)?, Some(held(0, 2, 40)));
    // Two versions taking effect at one epoch: the later version is in effect from it.
    assert_eq!(source.version_at(0, 50)?, Some(held(0, 4, 50)));
    assert_eq!(source.version_at(0, u64::MAX)?, Some(held(0, 4, 50)));
    Ok(())
}

#[test]
fn test_each_namespace_has_its_own_versions() -> TestResult {
    let source = TestCatalogSource::new();
    source.commit(held(1, 2, 5));
    assert_eq!(source.version_at(0, 5)?, None);
    assert_eq!(source.version_at(2, 5)?, None);
    assert_eq!(source.version_at(1, 5)?, Some(held(1, 2, 5)));
    assert_eq!(source.version(1, 2)?, Some(held(1, 2, 5)));
    assert_eq!(source.version(0, 2)?, None);
    Ok(())
}

#[test]
fn test_a_version_is_found_by_its_number() -> TestResult {
    let source = TestCatalogSource::new();
    source.commit(held(0, 2, 40));
    source.commit(held(0, 3, 41));
    assert_eq!(source.version(0, 2)?, Some(held(0, 2, 40)));
    assert_eq!(source.version(0, 3)?, Some(held(0, 3, 41)));
    assert_eq!(source.version(0, 4)?, None);
    assert_eq!(source.version(0, 1)?, None);
    Ok(())
}

#[test]
fn test_a_version_committed_again_replaces_its_contents() -> TestResult {
    let source = TestCatalogSource::new();
    source.commit(HeldVersion::new(0, 2, 40, vec![1]));
    source.commit(HeldVersion::new(0, 2, 40, vec![2]));
    let read = source.version(0, 2)?.map(|v| v.contents().to_vec());
    assert_eq!(read, Some(vec![2]));
    Ok(())
}

#[test]
fn test_a_default_entry_is_recorded_once() -> TestResult {
    let source = TestCatalogSource::new();
    assert!(source.record_default(&default(0, "score", 1))?);
    // A second first write of the predicate finds the record and leaves it as it was.
    assert!(!source.record_default(&default(0, "score", 2))?);
    assert_eq!(source.defaults(0), vec![default(0, "score", 1)]);
    Ok(())
}

#[test]
fn test_default_entries_are_kept_per_namespace_and_predicate() -> TestResult {
    let source = TestCatalogSource::new();
    assert!(source.record_default(&default(0, "b", 1))?);
    assert!(source.record_default(&default(0, "a", 1))?);
    assert!(source.record_default(&default(1, "a", 1))?);
    assert_eq!(
        source.defaults(0),
        vec![default(0, "a", 1), default(0, "b", 1)]
    );
    assert_eq!(source.defaults(1), vec![default(1, "a", 1)]);
    assert!(source.defaults(2).is_empty());
    Ok(())
}

#[test]
fn test_an_unreachable_source_answers_and_records_nothing() -> TestResult {
    let source = TestCatalogSource::new();
    source.commit(held(0, 1, 1));
    source.set_reachable(false);
    assert_eq!(source.version_at(0, 1), Err(CatalogUnreachable));
    assert_eq!(source.version(0, 1), Err(CatalogUnreachable));
    assert_eq!(
        source.record_default(&default(0, "p", 1)),
        Err(CatalogUnreachable)
    );
    source.set_reachable(true);
    assert!(source.defaults(0).is_empty(), "nothing was recorded");
    assert_eq!(source.version(0, 1)?, Some(held(0, 1, 1)));
    Ok(())
}

#[test]
fn test_clones_share_one_group() -> TestResult {
    let source = TestCatalogSource::new();
    let clone = source.clone();
    clone.commit(held(0, 1, 1));
    assert!(clone.record_default(&default(0, "p", 1))?);
    assert_eq!(source.version(0, 1)?, Some(held(0, 1, 1)));
    assert_eq!(source.defaults(0).len(), 1);
    source.set_reachable(false);
    assert_eq!(clone.version(0, 1), Err(CatalogUnreachable));
    Ok(())
}
