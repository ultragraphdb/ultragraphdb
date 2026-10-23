/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/hnsw/persistent_hnsw_test.go (TestFlatInMemListAddMutation,
 * TestFlatInMemListAddOverwriteMutation, TestFlatInMemListAddMultipleWritesMutation): the tests of the in-memory
 * store the index's tests use. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_traits_index::KeyValue;

use crate::utils::helpers::{InMemList, TsDbs};

fn kv(value: &str) -> KeyValue {
    KeyValue::new(0, "", value.as_bytes().to_vec())
}

/// Dgraph's `flatInMemListWriteMutation`.
fn write_mutation(dbs: &TsDbs, key: &str, start_ts: u64, finish_ts: u64, value: &str) {
    let list = InMemList::new(dbs, key, start_ts, finish_ts);
    assert_eq!(list.start_ts(), start_ts);
    list.add_mutation(&kv(value)).unwrap();
    let k = key.as_bytes();
    assert_eq!(
        dbs.get(finish_ts - 1, k),
        dbs.get(start_ts, k),
        "not before its commit"
    );
    assert_eq!(dbs.get(finish_ts, k), Some(value.as_bytes().to_vec()));
    assert_eq!(dbs.get(finish_ts, k), dbs.get(99, k), "to the end of time");
}

#[test]
fn test_flat_in_mem_list_add_mutation() {
    let dbs = TsDbs::default();
    for (key, start, finish, value) in [("a", 0, 5, "abc"), ("b", 1, 2, "123"), ("c", 0, 99, "xyz")]
    {
        write_mutation(&dbs, key, start, finish, value);
    }
}

#[test]
fn test_flat_in_mem_list_add_overwrite_mutation() {
    let dbs = TsDbs::default();
    for value in ["abc", "123", "xyz"] {
        write_mutation(&dbs, "a", 0, 5, value);
    }
}

#[test]
fn test_flat_in_mem_list_add_multiple_writes_mutation() {
    let dbs = TsDbs::default();
    let writes = [(0, 2, "abc"), (1, 3, "123"), (2, 4, "xyz")];
    for (i, (start, finish, value)) in writes.into_iter().enumerate() {
        let list = InMemList::new(&dbs, "a", start, finish);
        list.add_mutation(&kv(value)).unwrap();
        if i == 0 {
            write_mutation(&dbs, "a", start, finish, value);
        } else {
            assert_eq!(
                dbs.get(finish - 1, b"a"),
                Some(writes[i - 1].2.as_bytes().to_vec())
            );
            assert_eq!(dbs.get(finish, b"a"), Some(value.as_bytes().to_vec()));
        }
    }
}
