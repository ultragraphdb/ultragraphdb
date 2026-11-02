/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_codec::{Posting, PostingList, UidPack};
use ultragraphdb_tool_bdd::TestResult;

use crate::codec_tests::{id, packed, sequences};

fn pack() -> TestResult<UidPack> {
    packed(&sequences(3..=3)?, 256)
}

#[test]
fn test_a_list_returns_what_it_was_built_with() -> TestResult {
    let postings = vec![Posting::default().with_uid(id(1, 0, 3)?.to_u128())];
    let splits = vec![1, id(1, 0, 8)?.to_u128()];
    let plist = PostingList::default()
        .with_pack(pack()?)
        .with_postings(postings.clone())
        .with_commit_ts(5)
        .with_splits(splits.clone());
    assert_eq!(plist.pack(), Some(&pack()?));
    assert_eq!(plist.postings(), &postings[..]);
    assert_eq!(plist.commit_ts(), 5);
    assert_eq!(plist.splits(), &splits[..]);
    Ok(())
}

#[test]
fn test_the_default_list_holds_nothing() {
    let plist = PostingList::default();
    assert_eq!(plist.pack(), None);
    assert!(plist.postings().is_empty() && plist.splits().is_empty());
    assert_eq!(plist.commit_ts(), 0);
}

#[test]
fn test_a_list_is_changed_in_place() -> TestResult {
    let mut plist = PostingList::default().with_pack(pack()?);
    plist.set_pack(None);
    assert_eq!(plist.pack(), None);
    plist.set_pack(Some(pack()?));
    assert_eq!(plist.pack(), Some(&pack()?));
    plist.postings_mut().push(Posting::default().with_uid(9));
    plist.set_commit_ts(12);
    plist.set_splits(vec![1]);
    assert_eq!(
        plist,
        PostingList::default()
            .with_pack(pack()?)
            .with_postings(vec![Posting::default().with_uid(9)])
            .with_commit_ts(12)
            .with_splits(vec![1])
    );
    Ok(())
}
