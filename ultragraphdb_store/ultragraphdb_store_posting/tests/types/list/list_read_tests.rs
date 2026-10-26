/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_store::StoreError;
use ultragraphdb_proto_pb::pb::PostingList;
use ultragraphdb_store_posting::split_key_of;
use ultragraphdb_store_posting::{
    BIT_COMPLETE_POSTING, BIT_DELTA_POSTING, BIT_SCHEMA_POSTING, List, PostingErrorEnum, marshal,
};
use ultragraphdb_traits_store::{Entry, Item, Store};

use crate::utils::helpers::{key, new_store, set_uid, uids, write_delta, write_entries};

fn item(
    key: &[u8],
    version: u64,
    meta: u8,
    discard: bool,
    value: Vec<u8>,
) -> Result<Item, StoreError> {
    Ok(Item::new(key.to_vec(), version, meta, discard, value))
}

#[test]
fn test_a_split_part_cannot_be_read_as_a_list() {
    let part = split_key_of(&key("parts", 1), 1).unwrap();
    let error = List::read_versions(&part, Vec::new()).unwrap_err();
    assert_eq!(error.0, PostingErrorEnum::InvalidKey);
    assert_eq!(
        error.to_string(),
        "cannot read posting list using multi-part list key"
    );
}

#[test]
fn test_schema_and_unknown_versions_are_refused() {
    let k = key("meta", 1);
    let schema = List::read_versions(&k, vec![item(&k, 3, BIT_SCHEMA_POSTING, false, vec![])]);
    assert_eq!(
        schema.unwrap_err().0,
        PostingErrorEnum::SchemaPosting { key: k.clone() }
    );
    let unknown = List::read_versions(&k, vec![item(&k, 3, 0x20, false, vec![])]);
    assert_eq!(
        unknown.unwrap_err().0,
        PostingErrorEnum::UnexpectedMeta { meta: 0x20, key: k }
    );
}

#[test]
fn test_deltas_are_read_down_to_the_discard_flag() {
    let k = key("discard-flag", 1);
    let delta = |uid: u64| {
        let mut l = List::empty(k.clone());
        l.add_posting(set_uid(uid).clone(), false);
        l.marshal_delta()
    };
    let versions = vec![
        item(&k, 9, BIT_DELTA_POSTING, false, delta(9)),
        item(&k, 7, BIT_DELTA_POSTING, true, delta(7)),
        item(&k, 5, BIT_DELTA_POSTING, false, delta(5)),
    ];
    let (list, deltas) = List::read_versions(&k, versions).unwrap();
    assert_eq!(deltas, 2);
    assert_eq!((list.min_ts(), list.max_ts()), (0, 9));
    assert_eq!(uids(&list, 9, 0), vec![7, 9]);
}

#[test]
fn test_a_complete_version_ends_the_read() {
    let store = new_store();
    let k = key("complete", 1);
    write_delta(&store, &k, 2, &[set_uid(2)]);
    write_entries(
        &store,
        vec![
            Entry::put(
                k.clone(),
                4,
                BIT_COMPLETE_POSTING,
                marshal(&PostingList::default()),
            )
            .with_discard(),
        ],
    );
    write_delta(&store, &k, 6, &[set_uid(6)]);
    let (list, deltas) = List::read(&k, &store.snapshot(10).unwrap()).unwrap();
    assert_eq!((list.min_ts(), deltas), (4, 1));
    assert_eq!(uids(&list, 10, 0), vec![6]);
    assert_eq!(list.mutation().read_ts(), 10);
}

#[test]
fn test_a_missing_part_is_an_error() {
    let store = new_store();
    let k = key("missing-part", 1);
    let split = PostingList {
        splits: vec![1, 500],
        ..PostingList::default()
    };
    write_entries(
        &store,
        vec![Entry::put(
            k.clone(),
            3,
            BIT_COMPLETE_POSTING,
            marshal(&split),
        )],
    );
    write_entries(
        &store,
        vec![Entry::put(
            split_key_of(&k, 1).unwrap(),
            3,
            BIT_COMPLETE_POSTING,
            marshal(&PostingList::default()),
        )],
    );
    let error = List::read(&k, &store.snapshot(5).unwrap()).unwrap_err();
    assert_eq!(
        error.0,
        PostingErrorEnum::MissingPart {
            key: split_key_of(&k, 500).unwrap()
        }
    );
    // A list built without its parts cannot be iterated either.
    let bare = List::new(k.clone(), split, 3);
    assert!(matches!(bare.length(4, 0), -1));
}

#[test]
fn test_a_length_over_a_missing_part_is_an_error_where_dgraph_panics() {
    // Deviation: Dgraph's GetLength and getPostingAndLengthNoSort panic when a part cannot be read.
    let k = key("missing-part-length", 1);
    let split = PostingList {
        splits: vec![1, 500],
        ..PostingList::default()
    };
    let bare = List::new(k.clone(), split, 3);
    let missing = PostingErrorEnum::MissingPart {
        key: split_key_of(&k, 1).unwrap(),
    };
    assert_eq!(bare.get_length(4).unwrap_err().0, missing);
    assert_eq!(
        bare.get_posting_and_length_no_sort(4, 0, 7, false)
            .unwrap_err()
            .0,
        missing
    );
}

#[test]
fn test_a_part_that_is_not_a_complete_or_empty_list_is_refused() {
    let store = new_store();
    let k = key("part-meta", 1);
    let split = PostingList {
        splits: vec![1, 500],
        ..PostingList::default()
    };
    let part = |start: u64| split_key_of(&k, start).unwrap();
    write_entries(
        &store,
        vec![
            Entry::put(k.clone(), 3, BIT_COMPLETE_POSTING, marshal(&split)),
            Entry::put(
                part(1),
                3,
                ultragraphdb_store_posting::BIT_EMPTY_POSTING,
                Vec::new(),
            ),
            Entry::put(part(500), 3, BIT_DELTA_POSTING, Vec::new()),
        ],
    );
    let error = List::read(&k, &store.snapshot(5).unwrap()).unwrap_err();
    assert_eq!(
        error.0,
        PostingErrorEnum::UnexpectedMeta {
            meta: BIT_DELTA_POSTING,
            key: part(500)
        }
    );
    // An empty first part reads as no uids.
    write_entries(
        &store,
        vec![Entry::put(
            part(500),
            4,
            BIT_COMPLETE_POSTING,
            marshal(&PostingList {
                pack: ultragraphdb_core_codec::encode(
                    &[600],
                    ultragraphdb_store_posting::BLOCK_SIZE,
                ),
                ..PostingList::default()
            }),
        )],
    );
    let (list, _) = List::read(&k, &store.snapshot(5).unwrap()).unwrap();
    assert_eq!(uids(&list, 5, 0), vec![600]);
}
