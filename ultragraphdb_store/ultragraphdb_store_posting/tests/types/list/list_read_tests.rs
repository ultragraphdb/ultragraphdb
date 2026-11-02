/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_codec::PostingList;
use ultragraphdb_error_store::StoreError;
use ultragraphdb_store_posting::split_key_of;
use ultragraphdb_store_posting::{
    BIT_COMPLETE_POSTING, BIT_DELTA_POSTING, BIT_SCHEMA_POSTING, List, PostingErrorEnum, marshal,
};
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_store::{Entry, Item, Store};

use crate::utils::helpers::{Failure, key, new_store, set_uid, uids, write_delta, write_entries};

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
fn test_a_split_part_cannot_be_read_as_a_list() -> TestResult {
    let part = split_key_of(&key("parts", 1), 1)?;
    let error = List::read_versions(&part, Vec::new()).failure("a read of a split part")?;
    assert_eq!(error.0, PostingErrorEnum::InvalidKey);
    assert_eq!(
        error.to_string(),
        "cannot read posting list using multi-part list key"
    );
    Ok(())
}

#[test]
fn test_schema_and_unknown_versions_are_refused() -> TestResult {
    let k = key("meta", 1);
    let schema = List::read_versions(&k, vec![item(&k, 3, BIT_SCHEMA_POSTING, false, vec![])]);
    assert_eq!(
        schema.failure("a read of a schema entry")?.0,
        PostingErrorEnum::SchemaPosting { key: k.clone() }
    );
    let unknown = List::read_versions(&k, vec![item(&k, 3, 0x20, false, vec![])]);
    assert_eq!(
        unknown.failure("a read of an unknown meta byte")?.0,
        PostingErrorEnum::UnexpectedMeta { meta: 0x20, key: k }
    );
    Ok(())
}

#[test]
fn test_deltas_are_read_down_to_the_discard_flag() -> TestResult {
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
    let (list, deltas) = List::read_versions(&k, versions)?;
    assert_eq!(deltas, 2);
    assert_eq!((list.min_ts(), list.max_ts()), (0, 9));
    assert_eq!(uids(&list, 9, 0)?, vec![7, 9]);
    Ok(())
}

#[test]
fn test_a_complete_version_ends_the_read() -> TestResult {
    let store = new_store();
    let k = key("complete", 1);
    write_delta(&store, &k, 2, &[set_uid(2)])?;
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
    )?;
    write_delta(&store, &k, 6, &[set_uid(6)])?;
    let (list, deltas) = List::read(&k, &store.snapshot(10)?)?;
    assert_eq!((list.min_ts(), deltas), (4, 1));
    assert_eq!(uids(&list, 10, 0)?, vec![6]);
    assert_eq!(list.mutation().read_ts(), 10);
    Ok(())
}

#[test]
fn test_a_missing_part_is_an_error() -> TestResult {
    let store = new_store();
    let k = key("missing-part", 1);
    let split = PostingList::default().with_splits(vec![1, 500]);
    write_entries(
        &store,
        vec![Entry::put(
            k.clone(),
            3,
            BIT_COMPLETE_POSTING,
            marshal(&split),
        )],
    )?;
    write_entries(
        &store,
        vec![Entry::put(
            split_key_of(&k, 1)?,
            3,
            BIT_COMPLETE_POSTING,
            marshal(&PostingList::default()),
        )],
    )?;
    let error = List::read(&k, &store.snapshot(5)?).failure("the read")?;
    assert_eq!(
        error.0,
        PostingErrorEnum::MissingPart {
            key: split_key_of(&k, 500)?
        }
    );
    // A list built without its parts cannot be iterated either.
    let bare = List::new(k.clone(), split, 3);
    assert!(matches!(bare.length(4, 0), -1));
    Ok(())
}

#[test]
fn test_a_length_over_a_missing_part_is_an_error_where_dgraph_panics() -> TestResult {
    // Deviation: Dgraph's GetLength and getPostingAndLengthNoSort panic when a part cannot be read.
    let k = key("missing-part-length", 1);
    let split = PostingList::default().with_splits(vec![1, 500]);
    let bare = List::new(k.clone(), split, 3);
    let missing = PostingErrorEnum::MissingPart {
        key: split_key_of(&k, 1)?,
    };
    assert_eq!(
        bare.get_length(4)
            .failure("the length of a list missing a part")?
            .0,
        missing
    );
    assert_eq!(
        bare.get_posting_and_length_no_sort(4, 0, 7, false)
            .failure("the postings of a list missing a part")?
            .0,
        missing
    );
    Ok(())
}

#[test]
fn test_a_part_that_is_not_a_complete_or_empty_list_is_refused() -> TestResult {
    let store = new_store();
    let k = key("part-meta", 1);
    let split = PostingList::default().with_splits(vec![1, 500]);
    let part = |start: u64| split_key_of(&k, start);
    write_entries(
        &store,
        vec![
            Entry::put(k.clone(), 3, BIT_COMPLETE_POSTING, marshal(&split)),
            Entry::put(
                part(1)?,
                3,
                ultragraphdb_store_posting::BIT_EMPTY_POSTING,
                Vec::new(),
            ),
            Entry::put(part(500)?, 3, BIT_DELTA_POSTING, Vec::new()),
        ],
    )?;
    let error = List::read(&k, &store.snapshot(5)?).failure("the read")?;
    assert_eq!(
        error.0,
        PostingErrorEnum::UnexpectedMeta {
            meta: BIT_DELTA_POSTING,
            key: part(500)?
        }
    );
    // An empty first part reads as no uids.
    write_entries(
        &store,
        vec![Entry::put(
            part(500)?,
            4,
            BIT_COMPLETE_POSTING,
            marshal(
                &PostingList::default().with_pack(
                    ultragraphdb_core_codec::encode(&[600], ultragraphdb_store_posting::BLOCK_SIZE)
                        .context("the uids pack")?,
                ),
            ),
        )],
    )?;
    let (list, _) = List::read(&k, &store.snapshot(5)?)?;
    assert_eq!(uids(&list, 5, 0)?, vec![600]);
    Ok(())
}
