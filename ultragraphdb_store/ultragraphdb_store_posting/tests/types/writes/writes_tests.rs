/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_traits_store::{Entry, VersionlessEntry};

use crate::utils::helpers::{commit_edges, del_uid, key, new_lists, set_uid};

/// A request's writes leave it whole: its deltas by key, only those holding postings, its entries at the commit version
/// and at their own, its versionless writes, its epoch mark and the rebuild records it removes.
#[test]
fn test_a_requests_writes_are_everything_it_writes() {
    let lists = new_lists();
    let (read, written, cancelled) = (key("w", 1), key("w", 2), key("w", 3));
    commit_edges(&lists, &read, &[set_uid(1)]);
    let mut request = lists.request(lists.applied_ts());
    request.get(&read).unwrap();
    request
        .get_from_delta(&written)
        .add_posting(set_uid(2), false);
    let list = request.get_from_delta(&cancelled);
    list.add_posting(set_uid(3), true);
    list.add_posting(del_uid(3), true);
    request.add_entry(Entry::deletion(b"\x01a".to_vec(), 0));
    request.add_entry_at_version(Entry::deletion(b"\x01b".to_vec(), 3));
    request.add_versionless(b"term".to_vec(), b"uid".to_vec());
    request.advance_epoch_on_commit();
    request.remove_rebuild_record_on_commit(7);
    let writes = request.into_writes();
    assert_eq!(
        writes
            .deltas()
            .iter()
            .map(|(k, _)| k.clone())
            .collect::<Vec<_>>(),
        vec![written.clone()]
    );
    assert_eq!(writes.deltas()[0].1.postings().len(), 1);
    assert_eq!(
        writes.written_keys().collect::<Vec<_>>(),
        vec![written.as_slice()]
    );
    assert_eq!(writes.entries(), &[Entry::deletion(b"\x01a".to_vec(), 0)]);
    assert_eq!(
        writes.versioned_entries(),
        &[Entry::deletion(b"\x01b".to_vec(), 3)]
    );
    assert_eq!(
        writes.entry_keys().collect::<Vec<_>>(),
        vec![b"\x01a".as_slice(), b"\x01b"]
    );
    assert_eq!(
        writes.versionless(),
        &[VersionlessEntry::put(b"term".to_vec(), b"uid".to_vec())]
    );
    assert!(writes.advances_epoch());
    assert_eq!(writes.removed_records(), &[7]);
}

/// Taking a request's writes closes its read, which then holds no discard timestamp back.
#[test]
fn test_taking_the_writes_closes_the_read() {
    let lists = new_lists();
    let ts = commit_edges(&lists, &key("w", 4), &[set_uid(1)]);
    let request = lists.request(ts);
    assert_eq!(lists.oldest_open_read(), Some(ts));
    let writes = request.into_writes();
    assert_eq!(lists.oldest_open_read(), None);
    assert!(writes.deltas().is_empty() && writes.entries().is_empty());
    assert!(!writes.advances_epoch());
}
