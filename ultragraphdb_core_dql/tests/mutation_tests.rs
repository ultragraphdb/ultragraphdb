/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use serde::Deserialize;
use ultragraphdb_core_dql::{parse_uid, type_val_from};
use ultragraphdb_core_types::{TypeId, Value};
use ultragraphdb_proto_dgraph::api;
use ultragraphdb_proto_dgraph::api::value::Val;

#[derive(Debug, Deserialize)]
struct UidCase {
    input: String,
    uid: u64,
    err: String,
}

/// Uids with Go's base prefixes and underscores, zero, malformed and out of range, as Dgraph's `ParseUid` takes
/// them.
#[test]
fn test_golden_parse_uid() {
    let path =
        ultragraphdb_test_bdd::testdata_dir("ultragraphdb_core_dql").join("golden/dql_uids.json");
    let golden = ultragraphdb_test_bdd::load_golden::<UidCase>(&path);
    assert!(!golden.cases().is_empty());
    for c in golden.cases() {
        match parse_uid(&c.input) {
            Ok(uid) => assert!(c.err.is_empty() && uid == c.uid, "{c:?}: got {uid}"),
            Err(e) => assert_eq!(e.to_string(), c.err, "{c:?}"),
        }
    }
}

#[test]
fn test_type_val_from() {
    let v = |val| api::Value { val: Some(val) };
    assert_eq!(type_val_from(&v(Val::IntVal(3))).tid(), TypeId::INT);
    assert_eq!(
        type_val_from(&v(Val::DefaultVal("a".into()))).tid(),
        TypeId::DEFAULT
    );
    assert_eq!(
        type_val_from(&v(Val::PasswordVal("a".into()))).tid(),
        TypeId::PASSWORD
    );
    assert_eq!(
        type_val_from(&v(Val::Vfloat32Val(1f32.to_le_bytes().to_vec()))).value(),
        Some(&Value::VFloat(vec![1.0]))
    );
    let none = type_val_from(&api::Value { val: None });
    assert_eq!(
        (none.tid(), none.value()),
        (TypeId::STRING, Some(&Value::str("")))
    );
    assert_eq!(type_val_from(&v(Val::UidVal(5))).tid(), TypeId::STRING);
}
