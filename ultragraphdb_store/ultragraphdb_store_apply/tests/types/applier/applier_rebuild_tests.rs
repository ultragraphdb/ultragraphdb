/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, posting/index_test.go (TestCountReverseIndexWithData,
 * TestCountReverseIndexEmptyPosting, TestRebuildTokIndex, TestRebuildTokIndexWithDeletion, TestRebuildReverseEdges).
 * Changed for ultragraphdb: translated to Rust; a rebuild builds every index the change needs and swaps them in one
 * batch, where Go's tests drop the old prefixes and run one rebuilder each; an index removed is deleted at the swap's
 * commit timestamp, where Dgraph's prefix drop removes it at every timestamp, so the removal is read there.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_keys::{index_key, parse};
use ultragraphdb_proto_pb::pb::{SchemaUpdate, posting};
use ultragraphdb_store_apply::IndexRebuild;
use ultragraphdb_store_posting::BIT_EMPTY_POSTING;
use ultragraphdb_traits_store::{Scan, Store, StoreSnapshot};

use crate::utils::helpers::{
    TestApplier, attr, ckey, del_uid, dkey, index_prefix, new_applier, rkey, set_str, set_uid,
    uids_at, write_raw,
};

const SCHEMA_VAL: &str = "
name: string @index(term) .
name2: string @index(term) .
dob: dateTime @index(year) .
friend: [uid] @reverse .
";

const MUTATED_SCHEMA_VAL: &str = "
name:string @index(term) .
name2:string .
dob:dateTime @index(year) .
friend:[uid] @reverse .
";

fn uid_list() -> SchemaUpdate {
    SchemaUpdate {
        value_type: posting::ValType::Uid as i32,
        list: true,
        ..SchemaUpdate::default()
    }
}

/// The rebuild of `name` from no schema to its schema in the state, at `start_ts`.
fn rebuild_of(a: &TestApplier, name: &str, start_ts: u64) -> IndexRebuild {
    let current = a
        .schema()
        .get(ultragraphdb_core_schema::SchemaMode::Query, &attr(name))
        .expect("a schema");
    IndexRebuild::new(&attr(name), start_ts, None, current)
}

/// The index keys of `name` at `read_ts` whose newest version is not empty, and their uids.
fn index_entries(a: &TestApplier, name: &str, read_ts: u64) -> Vec<(Vec<u8>, Vec<u64>)> {
    let snapshot = a.lists().store().snapshot(read_ts).unwrap();
    let keys: Vec<Vec<u8>> = snapshot
        .scan(&Scan::prefix(index_prefix(name)))
        .map(|i| i.unwrap())
        .filter(|i| i.meta() & BIT_EMPTY_POSTING == 0)
        .map(|i| i.key().to_vec())
        .collect();
    keys.into_iter()
        .map(|k| {
            let u = uids_at(a, &k, read_ts);
            (k, u)
        })
        .collect()
}

#[test]
fn test_count_reverse_index_with_data() {
    let a = new_applier();
    let k = dkey("testcount", 1);
    write_raw(&a, &k, &[set_uid("testcount", 1, 23)], Some(&uid_list()), 9);
    write_raw(
        &a,
        &k,
        &[set_uid("testcount", 1, 23)],
        Some(&uid_list()),
        11,
    );
    assert_eq!(a.lists().read(&k, 12).unwrap().get_length(12).unwrap(), 2);
    a.schema()
        .parse_text("testcount: [uid] @count @reverse .")
        .unwrap();
    a.build_indexes(&rebuild_of(&a, "testcount", 12)).unwrap();
    assert_eq!(a.lists().read(&k, 14).unwrap().get_length(14).unwrap(), 1);
}

/// Deviation: Dgraph's count rebuild lists a subject whose list is empty under count 0 (`index.go:1601-1610`); the
/// port lists it under no count key, as count maintenance does.
#[test]
fn test_count_reverse_index_empty_posting() {
    let a = new_applier();
    let k = dkey("testcount", 1);
    write_raw(
        &a,
        &k,
        &[del_uid("testcount", 1, 23)],
        Some(&uid_list()),
        11,
    );
    assert_eq!(a.lists().read(&k, 12).unwrap().get_length(12).unwrap(), -1);
    a.schema()
        .parse_text("testcount: [uid] @count @reverse .")
        .unwrap();
    a.build_indexes(&rebuild_of(&a, "testcount", 12)).unwrap();
    assert_eq!(a.lists().read(&k, 14).unwrap().get_length(14).unwrap(), 0);
    assert!(uids_at(&a, &ckey("testcount", 0, false), 14).is_empty());
}

#[test]
fn test_rebuild_tok_index() {
    let a = new_applier();
    write_raw(
        &a,
        &dkey("name2", 91),
        &[set_str("name2", 91, "Michonne")],
        None,
        2,
    );
    write_raw(
        &a,
        &dkey("name2", 92),
        &[set_str("name2", 92, "David")],
        None,
        4,
    );
    a.schema().parse_text(SCHEMA_VAL).unwrap();
    a.build_indexes(&rebuild_of(&a, "name2", 5)).unwrap();
    let entries = index_entries(&a, "name2", 6);
    assert_eq!(
        entries,
        vec![
            (index_key(&attr("name2"), b"\x01david").unwrap(), vec![92]),
            (
                index_key(&attr("name2"), b"\x01michonne").unwrap(),
                vec![91]
            ),
        ]
    );
}

#[test]
fn test_rebuild_tok_index_with_deletion() {
    let a = new_applier();
    write_raw(
        &a,
        &dkey("name2", 91),
        &[set_str("name2", 91, "Michonne")],
        None,
        2,
    );
    write_raw(
        &a,
        &dkey("name2", 92),
        &[set_str("name2", 92, "David")],
        None,
        4,
    );
    a.schema().parse_text(SCHEMA_VAL).unwrap();
    let rebuild = rebuild_of(&a, "name2", 5);
    let current = rebuild.current_schema().clone();
    a.build_indexes(&rebuild).unwrap();

    // The schema mutated: the index of name2 is deleted.
    a.schema().parse_text(MUTATED_SCHEMA_VAL).unwrap();
    let new = rebuild_of(&a, "name2", 6).current_schema().clone();
    let swapped = a
        .build_indexes(&IndexRebuild::new(&attr("name2"), 6, Some(current), new))
        .unwrap();
    assert!(index_entries(&a, "name2", swapped).is_empty());
    assert_eq!(
        index_entries(&a, "name2", swapped - 1).len(),
        2,
        "reads below the swap keep the index"
    );
}

#[test]
fn test_rebuild_reverse_edges() {
    let a = new_applier();
    write_raw(
        &a,
        &dkey("friend", 1),
        &[set_uid("friend", 1, 23)],
        Some(&uid_list()),
        11,
    );
    write_raw(
        &a,
        &dkey("friend", 1),
        &[set_uid("friend", 1, 24)],
        Some(&uid_list()),
        13,
    );
    write_raw(
        &a,
        &dkey("friend", 2),
        &[set_uid("friend", 2, 23)],
        Some(&uid_list()),
        15,
    );
    a.schema().parse_text(SCHEMA_VAL).unwrap();
    a.build_indexes(&rebuild_of(&a, "friend", 16)).unwrap();
    let snapshot = a.lists().store().snapshot(17).unwrap();
    let mut reverse = rkey("friend", 0);
    reverse.truncate(reverse.len() - 8);
    let keys: Vec<Vec<u8>> = snapshot
        .scan(&Scan::prefix(reverse))
        .map(|i| i.unwrap().key().to_vec())
        .collect();
    assert_eq!(keys, vec![rkey("friend", 23), rkey("friend", 24)]);
    assert_eq!(uids_at(&a, &keys[0], 17), vec![1, 2]);
    assert_eq!(uids_at(&a, &keys[1], 17), vec![1]);
    assert!(keys.iter().all(|k| parse(k).unwrap().is_reverse()));
}
