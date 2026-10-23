/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_types::Value;
use ultragraphdb_store_posting::{List, PostingErrorEnum};

use crate::utils::helpers::{add, del_value, key, set_lang, set_uid, set_value};

fn text(v: &ultragraphdb_core_types::Val) -> Vec<u8> {
    match v.value() {
        Some(Value::Bytes(b)) => b.clone(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn test_language_values_coexist() {
    let mut l = List::empty(key("name", 1));
    add(&mut l, 1, &set_value("A"));
    add(&mut l, 1, &set_lang("Á", "fr"));
    l.commit_mutation(2);
    assert_eq!(text(&l.value(3).unwrap()), b"A");
    assert_eq!(
        text(&l.value_for(3, &["fr".to_string()]).unwrap()),
        "Á".as_bytes()
    );
    assert_eq!(text(&l.value_for_tag(3, "fr").unwrap()), "Á".as_bytes());
    assert_eq!(text(&l.value_for(3, &[]).unwrap()), b"A");
    assert_eq!(
        text(
            &l.value_for(3, &["de".to_string(), ".".to_string()])
                .unwrap()
        ),
        b"A"
    );
    assert_eq!(l.all_values(3).unwrap().len(), 2);
    assert_eq!(l.all_untagged_values(3).unwrap().len(), 1);
    let mut tags = l.get_lang_tags(3).unwrap();
    tags.sort();
    assert_eq!(tags, vec!["".to_string(), "fr".to_string()]);
    assert_eq!(
        l.value_for_tag(3, "de").unwrap_err().0,
        PostingErrorEnum::NoValue
    );
}

#[test]
fn test_any_language_falls_back_to_the_smallest_tagged_value() {
    let mut l = List::empty(key("name", 2));
    add(&mut l, 1, &set_lang("x", "fr"));
    l.commit_mutation(2);
    assert_eq!(l.value(3).unwrap_err().0, PostingErrorEnum::NoValue);
    assert_eq!(text(&l.value_for(3, &[".".to_string()]).unwrap()), b"x");
    assert_eq!(
        l.value_for(3, &["de".to_string()]).unwrap_err().0,
        PostingErrorEnum::NoValue
    );
}

#[test]
fn test_a_scalar_value_overwrites_the_previous_one() {
    let mut l = List::empty(key("name", 3));
    add(&mut l, 1, &set_value("A"));
    l.commit_mutation(2);
    add(&mut l, 3, &set_value("B"));
    l.commit_mutation(4);
    assert_eq!(l.length(5, 0), 1);
    assert_eq!(text(&l.value(5).unwrap()), b"B");
    // The posting layer removes the untagged value whatever value a delete names: the check that the value
    // matches is index maintenance's.
    add(&mut l, 5, &del_value("Z"));
    assert_eq!(l.value(5).unwrap_err().0, PostingErrorEnum::NoValue);
}

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
