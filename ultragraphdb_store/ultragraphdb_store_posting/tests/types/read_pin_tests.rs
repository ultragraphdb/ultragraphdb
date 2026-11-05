/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A read pin: an open read with no request, keeping its version readable until dropped.

use ultragraphdb_store_posting::PostingError;
use ultragraphdb_tool_bdd::TestResult;
use ultragraphdb_traits_store::Store;

use crate::utils::helpers::{Failure, commit, commit_edges, key, new_lists, rids, set_uid, uids};

#[test]
fn test_a_pin_keeps_its_version_readable_across_a_discard() -> TestResult {
    let lists = new_lists();
    let k = key("pinned", 1);
    let at = commit_edges(&lists, &k, &[set_uid(1)])?;
    let pin = lists.pin(at)?;
    assert_eq!(pin.version(), at);
    assert_eq!(lists.oldest_open_read(), Some(at));
    commit(&lists, &k, &[set_uid(2)])?;
    let later = lists.applied_ts();
    lists.set_discard_ts(later)?;
    assert!(lists.store().discard_ts() <= at, "the pin holds it back");
    lists.store().compact()?;
    assert_eq!(uids(&lists.read(&k, at)?, at, 0)?, rids(&[1]));
    drop(pin);
    assert_eq!(lists.oldest_open_read(), None);
    Ok(())
}

#[test]
fn test_a_pin_of_a_version_already_discarded_is_refused_and_holds_nothing() -> TestResult {
    let lists = new_lists();
    let k = key("pinned", 2);
    let at = commit_edges(&lists, &k, &[set_uid(1)])?;
    commit(&lists, &k, &[set_uid(2)])?;
    let later = lists.applied_ts();
    lists.set_discard_ts(later)?;
    let refused = lists.pin(at).failure("a pin below the discard")?;
    assert_eq!(refused, PostingError::ts_too_old());
    assert_eq!(lists.oldest_open_read(), None);
    let held = lists.pin(later)?;
    assert_eq!(held.version(), later, "the discard itself stays readable");
    Ok(())
}

#[test]
fn test_pins_of_one_version_close_one_at_a_time() -> TestResult {
    let lists = new_lists();
    let at = commit_edges(&lists, &key("pinned", 3), &[set_uid(1)])?;
    let (first, second) = (lists.pin(at)?, lists.pin(at)?);
    drop(first);
    assert_eq!(lists.oldest_open_read(), Some(at));
    drop(second);
    assert_eq!(lists.oldest_open_read(), None);
    Ok(())
}
