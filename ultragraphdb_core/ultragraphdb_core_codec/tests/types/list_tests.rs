/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_codec::List;

#[test]
fn a_list_holds_its_uids() {
    let list = List::new(vec![3, 1, 2]);
    assert_eq!(list.uids(), &[3, 1, 2]);
    assert_eq!(list.into_uids(), vec![3, 1, 2]);
    assert!(List::default().uids().is_empty());
}

#[test]
fn a_list_is_changed_in_place() {
    let mut list = List::new(vec![1, 2]);
    list.uids_mut().push(9);
    list.uids_mut().retain(|&u| u != 1);
    assert_eq!(list, List::new(vec![2, 9]));
}
