/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_store_posting::List;

use crate::utils::helpers::{add, key, set_uid};

#[test]
fn test_lengths_and_postings_without_sorting() {
    let mut l = List::empty(key("len", 1));
    (1..=5).for_each(|u| add(&mut l, 1, &set_uid(u)));
    l.commit_mutation(2);
    assert_eq!(l.get_length(3).unwrap(), 5);
    let (len, found) = l.get_posting_and_length(3, 0, 4);
    assert_eq!((len, found.map(|p| p.uid)), (5, Some(4)));
    let (len, found) = l.get_posting_and_length_no_sort(3, 0, 6, false).unwrap();
    assert_eq!((len, found), (5, None));
    let (len, found) = l.get_posting_and_length_no_sort(3, 0, 2, true).unwrap();
    assert_eq!((len, found.map(|p| p.uid)), (5, Some(2)));
    assert!(!l.is_empty(3, 0).unwrap());
    assert!(l.is_empty(3, 5).unwrap());
    assert!(l.is_empty(1, 0).unwrap());
}
