/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The compaction filter's rule over one compaction's items, in stored-key order.

use fjall::compaction::filter::Verdict;
use ultragraphdb_store_kv::GcFilter;

use crate::test_store::stored;

/// A stored value: meta byte, flags (discard 1, deletion 2, fenced 4), value.
fn value(flags: u8) -> Vec<u8> {
    vec![0x04, flags, b'v']
}

fn verdicts(filter: &mut GcFilter, items: &[(&[u8], u64, u8)]) -> Vec<&'static str> {
    items
        .iter()
        .map(
            |(key, version, flags)| match filter.verdict(&stored(key, *version), &value(*flags)) {
                Verdict::Keep => "keep",
                Verdict::Remove => "remove",
                Verdict::Destroy => "destroy",
                _ => "other",
            },
        )
        .collect()
}

#[test]
fn test_versions_older_than_a_discard_point_go_and_the_point_stays() {
    let mut f = GcFilter::new(6, false);
    let got = verdicts(
        &mut f,
        &[
            (b"k", 8, 0),
            (b"k", 5, 1),
            (b"k", 4, 0),
            (b"k", 2, 0),
            (b"l", 1, 0),
        ],
    );
    assert_eq!(got, vec!["keep", "keep", "remove", "remove", "keep"]);
    assert_eq!(f.collected().len(), 2);
    let mut last = GcFilter::new(6, true);
    assert_eq!(
        verdicts(&mut last, &[(b"k", 5, 1), (b"k", 4, 0)]),
        vec!["keep", "destroy"]
    );
}

#[test]
fn test_a_discard_point_above_the_discard_timestamp_collects_nothing() {
    let mut f = GcFilter::new(4, true);
    assert_eq!(
        verdicts(&mut f, &[(b"k", 5, 1), (b"k", 3, 0)]),
        vec!["keep", "keep"]
    );
}

#[test]
fn test_a_deletion_cuts_but_is_never_collected() {
    let mut f = GcFilter::new(6, true);
    assert_eq!(
        verdicts(&mut f, &[(b"k", 4, 2), (b"k", 3, 2), (b"k", 2, 0)]),
        vec!["keep", "keep", "destroy"]
    );
}

#[test]
fn test_a_deletion_alone_in_a_compaction_is_kept() {
    // The deletion's table is merged without the table holding the value below it.
    let mut f = GcFilter::new(6, true);
    assert_eq!(verdicts(&mut f, &[(b"k", 4, 2)]), vec!["keep"]);
    assert!(f.collected().is_empty());
}

#[test]
fn test_a_fenced_version_is_never_collected() {
    let mut f = GcFilter::new(9, true);
    assert_eq!(
        verdicts(&mut f, &[(b"k", 5, 1), (b"k", 1, 5)]),
        vec!["keep", "keep"]
    );
}

#[test]
fn test_an_older_copy_of_a_stored_key_shares_its_verdict() {
    let mut f = GcFilter::new(9, true);
    let got = verdicts(
        &mut f,
        &[(b"k", 5, 1), (b"k", 3, 0), (b"k", 3, 1), (b"k", 1, 0)],
    );
    assert_eq!(got, vec!["keep", "destroy", "destroy", "destroy"]);
    assert_eq!(f.collected().len(), 2, "each stored key once");
}

#[test]
fn test_an_undecodable_item_is_kept_and_counted() {
    let mut f = GcFilter::new(9, true);
    assert!(matches!(
        f.verdict(&stored(b"k", 5), &value(1)),
        Verdict::Keep
    ));
    assert!(matches!(
        f.verdict(&stored(b"k", 4), &[0x04]),
        Verdict::Keep
    ));
    assert!(matches!(
        f.verdict(&stored(b"k", 3), &[0x04, 0x80]),
        Verdict::Keep
    ));
    assert!(matches!(f.verdict(b"short", &value(0)), Verdict::Keep));
    assert_eq!(f.undecodable(), 3);
    assert!(
        matches!(f.verdict(&stored(b"k", 2), &value(0)), Verdict::Destroy),
        "the cut survives what it skipped"
    );
}
